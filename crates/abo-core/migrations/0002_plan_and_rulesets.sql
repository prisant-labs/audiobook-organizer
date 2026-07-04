-- 0002_plan_and_rulesets.sql - v0.3.0 (planning) data model, additive over
-- 0001_init.sql (the v0.1.0/v0.2.0 spine). Adds the tables F-403 (plan
-- builder), F-404 (plan validation), F-405 (plan persistence and approval),
-- F-701 (duplicate candidates), and F-801 (ruleset model) need, per the
-- feature breakdown Section 7 draft shapes and spec.md's F-507 (pack
-- provenance) plan_ops note. Pre-v1 the schema stays resettable, but this
-- migration is additive (new CREATE TABLE statements plus one ALTER TABLE
-- ADD COLUMN) rather than a reset, matching 0001's stated policy and the
-- v1.0.0 additive-only freeze it is rehearsing.
--
-- ## plan_ops column freeze (implementation-plan.md Phase 1 decision gate)
--
-- This is the frozen `plan_ops` column set that Phase 3 (structure
-- policies), Phase 4 (plan builder), Phase 5 (validation), and Phase 6
-- (provenance) build on for the rest of v0.3.0. Two write regimes on the
-- same row, both intentional (breakdown Section 7 lists `approval` as a
-- plan_ops field alongside the descriptive columns, and F-405 is explicit
-- that only the OPERATION LIST is immutable, not the review state riding
-- beside it):
--   - seq, op_group, kind, kind_reason, source_path, target_path, rationale,
--     rule_id, confidence, byte_size, validation_state, validation_reason,
--     provenance_json: written ONCE, in the single insert that creates the
--     plan (F-403 builder plus F-404 validation, which runs before a plan is
--     ever persisted, Phase 4/5/6). Never UPDATEd afterward: regenerating a
--     plan after a ruleset change INSERTs an entirely new `plans` row and a
--     fresh set of `plan_ops` rows rather than mutating these columns on an
--     existing row (F-405 AC-16).
--   - approval, approval_updated_at: the ONE deliberately mutable pair. The
--     F-405 approve/reject/exclude state machine (Phase 5) UPDATEs these in
--     place as the user reviews; a `blocked` op cannot move to `approved`
--     (enforced in application code, since it depends on reading
--     validation_state, not a CHECK constraint).
--
-- op_group carries one of the EIGHT internal passes (staging-separation,
-- loose-root-books, strip-noise, split-multi-book, flatten-packs,
-- normalize-series, dedupe-quarantine, empty-cleanup); the seven
-- user-facing campaign-group labels (FD-26) are a pure fold over this column
-- computed in Rust (normalize-series folds into "messy names"), not a
-- second stored column, so there is exactly one source of truth for which
-- pass produced an op.

-- Named, schema-versioned rule bundles (F-801): naming templates + structure
-- policies + cleanup toggles, as one validated JSON body, so abo-core and
-- any future CLI share the same persisted shape. Body validation against the
-- schema version is `ruleset.rs`'s job (v0.3.0 Phase 3, not yet landed);
-- this table only stores what already passed validation (AC-29: a failing
-- body is never persisted, so there is no "invalid but stored" state here).
CREATE TABLE rulesets (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    body_json      TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

-- One row per generated plan (F-403/F-405): the immutable header.
-- Regenerating after a ruleset tweak INSERTs a new row; an existing row's
-- scan_id/ruleset_id/created_at are never UPDATEd (AC-16). `status`
-- (draft | reviewed | stale | superseded) is the one header field the app
-- transitions over the plan's lifecycle; that lifecycle flag is a separate
-- concern from the operation-list immutability contract AC-16 is about.
CREATE TABLE plans (
    id          INTEGER PRIMARY KEY,
    scan_id     INTEGER NOT NULL REFERENCES scans(id),
    ruleset_id  INTEGER NOT NULL REFERENCES rulesets(id),
    created_at  TEXT NOT NULL,
    status      TEXT NOT NULL,
    stats_json  TEXT
);

-- One row per operation in a plan (F-403). See the column-freeze note above
-- for which columns are write-once versus the one mutable approval pair.
-- `plan_id` keeps a hard FK: a plan's ops are always inserted in the same
-- transaction as the plan header, so there is no bulk-insert ordering
-- hazard like `entries.parent_id` (migration 0001) has.
--
--   kind            move | rename | mkdir | rmdir-empty | quarantine | no-op
--   kind_reason     the no-op parameter (manual-review, user-excluded);
--                   NULL for every other kind
--   confidence      high | medium | low (matches parse::extract::Confidence)
--   validation_state   valid | warning | blocked (F-404)
--   validation_reason  the AppError-style machine code detail for a
--                      warning/blocked verdict; NULL for valid
--   provenance_json    F-507 pack/award provenance (source pack id/title,
--                      award/rank marker) for flatten-packs ops; NULL for
--                      every non-pack op. Hand-rolled JSON text (same
--                      convention as db::activity::json_object), so
--                      production code needs no JSON crate dependency to
--                      write it.
--   approval           pending | approved | rejected | excluded (F-405
--                      AC-17); defaults to pending at insert time
--   approval_updated_at   NULL until the first approval-state change
CREATE TABLE plan_ops (
    id                  INTEGER PRIMARY KEY,
    plan_id             INTEGER NOT NULL REFERENCES plans(id),
    seq                 INTEGER NOT NULL,
    op_group            TEXT NOT NULL,
    kind                TEXT NOT NULL,
    kind_reason         TEXT,
    source_path         TEXT NOT NULL,
    target_path         TEXT NOT NULL,
    rationale           TEXT NOT NULL,
    rule_id             TEXT NOT NULL,
    confidence          TEXT NOT NULL,
    byte_size           INTEGER NOT NULL DEFAULT 0,
    validation_state    TEXT NOT NULL DEFAULT 'valid',
    validation_reason   TEXT,
    provenance_json     TEXT,
    approval            TEXT NOT NULL DEFAULT 'pending',
    approval_updated_at TEXT
);
CREATE INDEX idx_plan_ops_plan_seq ON plan_ops (plan_id, seq);

-- Duplicate candidate groups (F-701): the GROUP is the canonical unit
-- (FD-08), never the pairwise comparison. `method` (exact-basename-size |
-- normalized-title) is the single source of truth for exact-vs-version
-- labeling; a report derives the "exact" / "version" label from it in Rust
-- rather than storing a second redundant column, matching the op_group /
-- campaign-group fold pattern above.
CREATE TABLE duplicate_groups (
    id           INTEGER PRIMARY KEY,
    scan_id      INTEGER NOT NULL REFERENCES scans(id),
    method       TEXT NOT NULL,
    group_key    TEXT NOT NULL,
    total_bytes  INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

-- One row per member ("copy") of a duplicate group (F-701). entry_id keeps a
-- hard FK: duplicate detection runs over an already-committed scan's
-- entries, so (unlike entries.parent_id at bulk-insert time) there is no
-- insert-ordering hazard to work around.
CREATE TABLE duplicate_members (
    id        INTEGER PRIMARY KEY,
    group_id  INTEGER NOT NULL REFERENCES duplicate_groups(id),
    entry_id  INTEGER NOT NULL REFERENCES entries(id),
    path      TEXT NOT NULL,
    size      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_duplicate_members_group ON duplicate_members (group_id);

-- FD-20 snapshot retention: bounds DB growth by keeping only the last N
-- scans. The setting is written now (default 10, per FD-20); its settings
-- UI lands with F-803 (v0.4.0). ADD COLUMN with a NOT NULL DEFAULT backfills
-- the existing singleton settings row from 0001, so this stays additive.
ALTER TABLE settings ADD COLUMN scan_retention_count INTEGER NOT NULL DEFAULT 10;
