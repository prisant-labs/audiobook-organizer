-- F-905 (duplicates surface, v0.6.0 P5): make the duplicate write path
-- idempotent, and give a user's confirmation somewhere to live that cannot
-- outlive the scan it was made against.
--
-- TWO PROBLEMS, ONE MIGRATION, because they are the same problem seen twice: a
-- write path nobody had called yet, and a decision nobody had recorded yet, both
-- keyed to a snapshot whose ids stop meaning anything after a re-scan.

-- 1. `insert_duplicate_groups` INSERTS BLINDLY, so calling it twice for one scan
-- writes every group and every member a second time. That was harmless while it
-- had no production caller (its only callers today are tests and one benchmark),
-- and stops being harmless the moment P5 gives it one: the verification job runs
-- whenever the duplicates surface opens, and the second open would double the
-- rows the first one wrote.
--
-- NO BACKFILL AND NO DEDUP STEP, and that is a fact rather than an assumption:
-- because nothing in the product has ever called the insert, no shipped database
-- has a single `duplicate_groups` row for this index to conflict with.
--
-- WHY `method` IS IN THE KEY. It costs nothing and it closes a keyspace
-- collision. The two detectors build keys in different shapes: the exact
-- detector uses "basename|size" (`Dune.m4b|900`) and the version detector uses a
-- normalized title (`dresden files`). A normalized title that happened to read
-- like an exact key would otherwise be the same row. `method` is safe to key on
-- because it is a per-detector constant that does not drift: `F-1110` raises the
-- match TIER, which lives in the separate `book_match` field, not the method.
CREATE UNIQUE INDEX idx_duplicate_groups_scan_key
    ON duplicate_groups (scan_id, method, group_key);

-- 2. A CONFIRMED RESOLUTION (`AC-24`) has had nowhere to live. The core type
-- `ConfirmedResolution` exists and the plan builder already consumes a slice of
-- them; every caller passes an empty slice, because nothing persists one.
--
-- KEYED TO `scan_id`, WHICH IS THE WHOLE POINT. `entries.id` is unique only
-- within one snapshot, and `FD-39` re-plans from a FRESH scan after an
-- interruption rather than replaying. A confirmation carried across a re-scan
-- would archive whatever file happens to hold that id next, which is the worst
-- failure this product can have. The `scan_id` column plus a read path that
-- filters on it makes a stale confirmation UNREACHABLE BY CONSTRUCTION rather
-- than by remembering to run a cleanup.
--
-- KEYED BY (method, group_key) RATHER THAN `duplicate_groups(id)`, deliberately.
-- Group rows are persisted lazily, as a side effect of the first hash job, so a
-- group the user confirmed may have no row: `AC-12`'s override exists precisely
-- so a group nobody hashed can still be resolved. A confirmation therefore
-- stands on the SCAN, which is the thing that actually bounds its validity, not
-- on a row that may never be written.
CREATE TABLE duplicate_confirmations (
    id              INTEGER PRIMARY KEY,
    scan_id         INTEGER NOT NULL REFERENCES scans(id),
    method          TEXT NOT NULL,
    group_key       TEXT NOT NULL,
    -- `entries.id` of the copy to keep. Never a path: the executor validates
    -- against the snapshot, and a path would be a second source of truth.
    keeper_entry_id INTEGER NOT NULL REFERENCES entries(id),
    confirmed_at    TEXT NOT NULL
);

-- One confirmation per group per scan. Re-confirming replaces rather than
-- accumulates, which is what makes the write an upsert and the read unambiguous:
-- two confirmations for one group would need a rule for which one wins, and any
-- such rule is a coin toss about which files get archived.
CREATE UNIQUE INDEX idx_duplicate_confirmations_group
    ON duplicate_confirmations (scan_id, method, group_key);

-- One row per copy the user confirmed for the Archive.
--
-- STORED EXPLICITLY, never re-derived by taking the group's members and removing
-- the keeper. What gets archived must be exactly what the user confirmed, and
-- fresh detection can legitimately return a different member set on a later read
-- (`F-1110` subsumption changed grouping once already). Re-deriving would let a
-- copy the user never saw inherit a confirmation they never gave.
CREATE TABLE duplicate_confirmation_losers (
    id              INTEGER PRIMARY KEY,
    confirmation_id INTEGER NOT NULL REFERENCES duplicate_confirmations(id) ON DELETE CASCADE,
    entry_id        INTEGER NOT NULL REFERENCES entries(id)
);

CREATE INDEX idx_duplicate_confirmation_losers_confirmation
    ON duplicate_confirmation_losers (confirmation_id);
