-- 0005_journal_and_manifests.sql - v0.5.0 (acting) Phase 2 (journal + manifest,
-- journal-before-act), additive over 0001-0004. RECON CORRECTION: the v0.1.0
-- migration was believed to already carry `journal` and `manifests` tables; in
-- reality 0001-0004 create only scans/entries/jobs/settings/activity_records
-- (0001), rulesets/plans/plan_ops/duplicate_* (0002), the F-803 settings columns
-- (0003), and rulesets.is_active (0004). This migration adds the two tables the
-- executor's safety spine needs, in the same additive style as
-- 0002_plan_and_rulesets.sql (new CREATE TABLEs, never a reset; the pre-v1
-- additive-only posture the whole schema rehearses).
--
-- ## journal (F-602, R-5): append-only, journal-before-act
--
-- One row per lifecycle event of one executed operation. The columns are the
-- P1 `abo_core::exec::JournalEntry` shape stored ONE-TO-ONE (job_id, seq, op_id,
-- phase, at, detail_json), plus one surrogate `id`:
--   - id          - integer primary key (autoincrement). A stable append identity
--                   and the deterministic insertion-order tiebreaker when two rows
--                   share (job_id, seq) - the intent row and its terminal row. It
--                   is NOT part of the JournalEntry shape; it is bookkeeping.
--   - job_id      - the apply jobs.id this row belongs to (hard FK: the jobs row
--                   is always inserted before the walk begins, so the ordering a
--                   hard FK requires always holds - unlike entries.parent_id).
--   - seq         - the operation's plan_ops.seq: walk order forward, the order a
--                   rollback reverses.
--   - op_id       - the plan_ops.id this row is about. Deliberately WITHOUT a hard
--                   FK (a LOGICAL reference, exactly like entries.parent_id): the
--                   journal is an append-only progress log written on the apply
--                   path, and keeping the append a single unconstrained INSERT lets
--                   a later reconciliation pass (v0.6.0) still read the log even if
--                   a plan row were ever pruned. Integrity of op_id is the
--                   executor's job, not the schema's.
--   - phase       - 'intent' before the filesystem call, 'done' or 'failed' after
--                   (journal-before-act). CHECK-constrained to those three values,
--                   which are exactly JournalPhase's serde kebab-case tags.
--   - at          - ISO-8601 UTC timestamp, supplied by the caller (the core stays
--                   clock-free, like every other TEXT timestamp column).
--   - detail_json - per-op JSON detail: the F-507 pack/award provenance on the
--                   intent row (FD-01, AC-12), the failure text on a failed row;
--                   NULL when there is nothing to carry.
--
-- Append-only by contract (AC-10): rows are only ever INSERTed. Nothing in the
-- codebase UPDATEs or DELETEs a `journal` row - the intent row must survive a
-- kill so a later reconciliation can find an intent with no terminal row.
CREATE TABLE journal (
    id          INTEGER PRIMARY KEY,
    job_id      INTEGER NOT NULL REFERENCES jobs(id),
    seq         INTEGER NOT NULL,
    op_id       INTEGER NOT NULL,          -- logical plan_ops.id; no hard FK (append-only log)
    phase       TEXT NOT NULL CHECK (phase IN ('intent', 'done', 'failed')),
    at          TEXT NOT NULL,
    detail_json TEXT
);

-- Reads are always "every row for one apply job, in walk order": the intent and
-- terminal rows an apply produced, ordered by seq (then id for the two rows that
-- share a seq). This one index backs both the apply-time append locality and a
-- reconciliation scan.
CREATE INDEX idx_journal_job_seq ON journal (job_id, seq);

-- ## manifests (AC-11): the self-contained undo file's index row
--
-- One row per exported manifest (the user-facing "undo file"). The self-contained
-- recovery data lives in the JSON file at json_path (op ids, source, target, kind,
-- order, provenance, and the schema version), NOT in this row: the row is only a
-- pointer so the app can find a job's undo file. Recovery never depends on this
-- row or on any app-database table being healthy - the JSON file alone is enough.
--   - id         - integer primary key.
--   - job_id     - the apply jobs.id whose completion produced this manifest (hard
--                  FK, jobs written first).
--   - plan_id    - the plans.id that was applied (hard FK, the plan predates apply).
--   - json_path  - absolute path to the exported undo-file JSON in the Reports
--                  folder.
--   - reversible - 1 when every operation in the manifest can be reversed, else 0
--                  (FD-10 guarantees reversibility in the current op set; the flag
--                  is stored so a future non-reversible op is recorded honestly).
--   - mode       - 'dry-run' or 'real' (the ApplyMode this manifest records). A
--                  dry-run manifest describes a REHEARSAL: no file actually moved,
--                  so a future undo/reconciliation flow must never offer to reverse
--                  it. The same marker rides the undo-file JSON (self-contained
--                  recovery) and the `jobs.mode` column (DB-side recovery when a
--                  walk journaled but failed before exporting a manifest), so all
--                  three agree.
--
-- Append-only, same caveat as `journal`: manifest rows are only ever INSERTed.
CREATE TABLE manifests (
    id         INTEGER PRIMARY KEY,
    job_id     INTEGER NOT NULL REFERENCES jobs(id),
    plan_id    INTEGER NOT NULL REFERENCES plans(id),
    json_path  TEXT NOT NULL,
    reversible INTEGER NOT NULL,
    mode       TEXT NOT NULL CHECK (mode IN ('dry-run', 'real'))
);

CREATE INDEX idx_manifests_job ON manifests (job_id);

-- ## jobs.mode (dry-run vs Real apply marker)
--
-- The `jobs` row is the apply lifecycle record and ALWAYS exists, even when the
-- walk journaled some ops but failed before exporting a manifest. Reconciliation
-- (v0.6.0 F-606) scanning the journal by job_id therefore needs to know each apply
-- job's mode from the DB even with no manifest row, so a dry-run rehearsal is never
-- mistaken for a real apply. This is the DB-side single source of truth for a
-- job's mode; the manifests row and the undo file carry the same value derived
-- from it. NULL for non-apply jobs (scan/rollback), which have no dry-run/Real
-- distinction. AC-3 note: this marker (the "RealFs/MemFs marker") lives on the job,
-- NOT on the journal rows, so a dry-run and a Real apply produce byte-identical
-- journal sequences (modulo the `at` phase-timing), exactly as AC-3 requires.
ALTER TABLE jobs ADD COLUMN mode TEXT;
