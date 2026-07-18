-- 0006_verification_blocks.sql - v0.5.0 (acting) Phase 6 (post-apply
-- verification + block-further-groups), additive over 0001-0005.
--
-- ## verification_blocks (F-604, AC-20): the after-the-fact check's block state
--
-- After any apply the tool checks that the moves happened as planned (the
-- per-op verifier, `abo_core::exec::verify`). When that check finds a
-- discrepancy - a move the executor journaled as done that reality contradicts
-- (target missing, size wrong, source still present) - forward tidying is
-- BLOCKED until a human acknowledges it (AC-20). This table is the DURABLE home
-- of that block state, so it survives an app restart: a blocked library stays
-- blocked across relaunch until acknowledged, never silently forgotten.
--
-- Why a dedicated table (not a jobs column): the acknowledgement is APPEND-ONLY.
-- Recording "raised" then rewriting the same row to "acknowledged" would erase
-- the history that a check ever failed - exactly what a jobs-column flag would
-- do. Instead a discrepancy INSERTs a `raised` row and an acknowledgement
-- INSERTs a separate `acknowledged` row for the same job, so both events stay on
-- the record. The block is ACTIVE for a job iff it has a `raised` row with no
-- later `acknowledged` row; forward tidying is blocked iff ANY job is active.
-- This mirrors the `journal` table's append-only contract (migration 0005):
-- history is added to, never rewritten.
--
--   - id          - integer primary key. A stable append identity and the
--                   deterministic ordering tiebreaker: a later `acknowledged`
--                   row for a job always has a higher id than the `raised` row
--                   it clears, so "raised with no later acknowledged" is an
--                   id-ordered NOT EXISTS.
--   - job_id      - the apply jobs.id whose after-the-fact check this row is
--                   about (hard FK: the jobs row is always inserted before the
--                   walk, so it exists before any check runs).
--   - state       - 'raised' (a discrepancy blocked further tidying) or
--                   'acknowledged' (a human acknowledged that job's discrepancy,
--                   clearing the block). CHECK-constrained to those two values.
--   - at          - ISO-8601 UTC timestamp, supplied by the caller (the core
--                   stays clock-free, like every other TEXT timestamp column).
--   - detail_json - on a `raised` row, a compact JSON summary of the
--                   discrepancies (how many, and the offending op ids/paths and
--                   which checks failed) so the activity surface can describe the
--                   block without re-reading the check report; NULL on an
--                   `acknowledged` row.
--
-- Append-only by contract (never UPDATEd or DELETEd): the raised row must
-- survive an acknowledgement so the fact a check once failed is always
-- recoverable.
CREATE TABLE verification_blocks (
    id          INTEGER PRIMARY KEY,
    job_id      INTEGER NOT NULL REFERENCES jobs(id),
    state       TEXT NOT NULL CHECK (state IN ('raised', 'acknowledged')),
    at          TEXT NOT NULL,
    detail_json TEXT
);

-- Reads are always "the block rows for one job" (is this job's check
-- outstanding?) and "is any job outstanding?" (the forward gate). This index
-- backs the per-job lookup; the forward-gate scan is small (one row per apply
-- that ever found a discrepancy).
CREATE INDEX idx_verification_blocks_job ON verification_blocks (job_id);
