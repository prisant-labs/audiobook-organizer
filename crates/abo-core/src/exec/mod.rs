//! The executor (F-601/F-607): apply an approved plan through the [`Vfs`] seam.
//!
//! v0.5.0 (acting) Phase 1 lands the SEAM and a SKELETON walk. The [`vfs`]
//! submodule holds the [`Vfs`] trait with [`RealFs`] and [`MemFs`]; this module
//! holds [`Executor`], generic over `V: Vfs`, plus the journal row shape the next
//! phase persists. The executor reaches the filesystem ONLY through its `Vfs`; it
//! makes no direct standard-library filesystem call, and a unit test
//! ([`tests::executor_operation_logic_has_no_direct_std_filesystem_call`]) greps
//! this module's own operation logic to keep that true (AC-1).
//!
//! What is real and what is deferred (as of v0.5.0 Phase 2):
//! - Real: the seam ([`Vfs`]/[`RealFs`]/[`MemFs`]), the generic [`Executor`], the
//!   journal-before-act walk over the plan's APPROVED operations, the journal
//!   seam ([`journal::Journal`]/[`SqliteJournal`]/[`MemJournal`]) that persists
//!   [`JournalEntry`] rows, and the self-contained undo [`manifest`].
//! - Skeleton: per-operation dispatch ([`Executor::dispatch`]) performs no
//!   filesystem mutation - it only exercises the seam with a read-only probe, so
//!   the walk journals `intent` then `done` per op "as if executed". The
//!   rename-first / copy+verify+delete / TOCTOU / never-overwrite operation logic
//!   (and its `failed` journal branch) is Phase 3, written test-first against
//!   [`MemFs`]; it swaps only the `dispatch` body, not the journaling around it.
//!
//! # Journal row shape (Phase 1 decision gate)
//!
//! Defined here now, in the abstract, so the journal phase and the executor-core
//! phase agree before either writes a row. A journal row is
//! `(job_id, seq, op_id, phase, at, detail_json)`:
//! - `job_id` - the apply `jobs.id` this row belongs to.
//! - `seq` - the operation's position in the plan (`plan_ops.seq`), which is the
//!   dependency order the executor walks and the order a rollback reverses.
//! - `op_id` - the `plan_ops.id` the row is about.
//! - `phase` - [`JournalPhase::Intent`] before the filesystem call,
//!   [`JournalPhase::Done`] or [`JournalPhase::Failed`] after (journal-before-act,
//!   R-5).
//! - `at` - ISO-8601 UTC timestamp, supplied by the caller (the core stays
//!   clock-free, like the db layers that take a `now` parameter).
//! - `detail_json` - per-op JSON detail: the F-507 pack/award provenance on the
//!   intent row (FD-01, AC-12), and the failure code/text on a failed row.
//!
//! The next phase owns the `journal` database table (an additive migration) and
//! the write-and-flush path; this phase deliberately creates no table. The
//! skeleton walk produces the INTENT rows in memory so the shape is exercised and
//! the later phase implements persistence against a fixed type.

pub mod journal;
pub mod manifest;
pub mod vfs;

use serde::{Deserialize, Serialize};

use crate::db::plans::PlanOpRow;
use crate::error::AppError;

pub use journal::{Journal, MemJournal, SqliteJournal};
pub use manifest::{
    build_manifest, Manifest, ManifestError, ManifestOp, ReverseOp, MANIFEST_JSON_BASENAME,
    MANIFEST_SCHEMA_VERSION,
};
pub use vfs::{MemFs, RealFs, SeedEntry, Vfs, VfsError, VfsMetadata};

/// The `plan_ops.approval` value that marks an operation executable. Only
/// approved operations are walked (the plan review's include/skip decision is the
/// gate; everything else stays put).
const APPROVED: &str = "approved";

/// Which filesystem an apply runs against - the `mode` argument of `apply_start`.
///
/// [`DryRun`](ApplyMode::DryRun) walks the plan against a [`MemFs`] seeded from
/// the snapshot (a first-class preview, D-04); [`Real`](ApplyMode::Real) would
/// walk it against [`RealFs`]. In v0.5.0 Phase 1 a Real apply is refused at the
/// command boundary while the operation logic is a skeleton (D-09 safety
/// invariant), so no intermediate build can half-apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyMode {
    /// Walk against memory; touch no real path.
    DryRun,
    /// Walk against the real filesystem (not available in this build yet).
    Real,
}

impl ApplyMode {
    /// The stable lowercase tag stored in `jobs.mode` / `manifests.mode` and the
    /// undo file (equals the serde kebab-case tag).
    pub fn as_str(&self) -> &'static str {
        match self {
            ApplyMode::DryRun => "dry-run",
            ApplyMode::Real => "real",
        }
    }
}

/// The lifecycle phase of one journal row (see the module-level decision gate).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JournalPhase {
    /// Written and flushed BEFORE the filesystem call (journal-before-act, R-5).
    Intent,
    /// Written after the operation succeeded.
    Done,
    /// Written after the operation failed, carrying the failure detail.
    Failed,
}

impl JournalPhase {
    /// The stable, lowercase tag stored in the `journal.phase` column (and checked
    /// by that column's CHECK constraint). Equals the serde kebab-case tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            JournalPhase::Intent => "intent",
            JournalPhase::Done => "done",
            JournalPhase::Failed => "failed",
        }
    }
}

/// One journal row (the Phase 1 decision-gate shape): the durable record of one
/// operation's progress that also becomes the undo manifest (R-5). Persisted to the
/// `journal` table with the intent row flushed before acting (Phase 2).
///
/// # AC-3 excluded fields (dry-run == Real journal sequence)
///
/// A journal row deliberately carries NO dry-run/Real marker. The two fields AC-3
/// documents as the only permitted differences between a dry-run and a Real apply's
/// journal sequence are: (1) `at`, the phase-timing metadata, and (2) the
/// RealFs/MemFs (mode) marker, which lives on the `jobs.mode` / `manifests.mode`
/// row and the undo file, NOT here. So the intent/done row sequences are otherwise
/// byte-identical across modes, exactly as AC-3 requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The apply `jobs.id` this row belongs to.
    pub job_id: i64,
    /// The operation's position in the plan (`plan_ops.seq`): walk order forward,
    /// reverse order for rollback.
    pub seq: i64,
    /// The `plan_ops.id` this row is about.
    pub op_id: i64,
    /// Where the operation is in its lifecycle.
    pub phase: JournalPhase,
    /// ISO-8601 UTC timestamp, supplied by the caller.
    pub at: String,
    /// Per-operation JSON detail: F-507 provenance on the intent row (FD-01), the
    /// failure code/text on a failed row. `None` when there is nothing to carry.
    pub detail_json: Option<String>,
}

/// The result of an executor walk: how many approved operations were walked and
/// the journal rows produced (an in-memory echo of what was flushed through the
/// [`Journal`] seam). This phase produces an [`JournalPhase::Intent`] and a
/// [`JournalPhase::Done`] row per walked op; the `failed` branch lands with the
/// real operation logic in Phase 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The number of approved operations the executor walked.
    pub ops_walked: usize,
    /// The journal rows produced this walk (intent + done per op).
    pub journal: Vec<JournalEntry>,
}

/// The executor: consumes an approved plan and a [`Vfs`], and walks the plan's
/// operations against that filesystem. Generic over `V` so the identical code
/// serves a dry run ([`MemFs`]) and a Real apply ([`RealFs`]) - the seam that
/// makes the dry run a first-class product (R-1).
pub struct Executor<V: Vfs> {
    vfs: V,
    job_id: i64,
    ops: Vec<PlanOpRow>,
}

impl<V: Vfs> Executor<V> {
    /// Build an executor over `vfs` for apply `job_id`, holding the plan's `ops`
    /// (all of them; the walk selects the approved ones).
    pub fn new(vfs: V, job_id: i64, ops: Vec<PlanOpRow>) -> Self {
        Self { vfs, job_id, ops }
    }

    /// Borrow the underlying filesystem (so a caller that seeded a [`MemFs`] can
    /// inspect its final state after a walk).
    pub fn vfs(&self) -> &V {
        &self.vfs
    }

    /// Borrow the plan's operation rows (so a caller can build the undo manifest
    /// from exactly what the executor was given after a walk).
    pub fn ops(&self) -> &[PlanOpRow] {
        &self.ops
    }

    /// Walk the plan's APPROVED operations in `seq` order, dispatching each through
    /// the `journal` seam, and return the walk outcome. `now` is the ISO-8601 UTC
    /// timestamp stamped on the journal rows (the core stays clock-free).
    ///
    /// Journal-before-act (R-5, AC-10): each operation's [`JournalPhase::Intent`]
    /// row is FLUSHED through [`Journal::write_intent`] BEFORE
    /// [`dispatch`](Self::dispatch) touches the filesystem. The sequencing is
    /// STRUCTURAL, not advisory: the walk cannot reach the `dispatch` call except
    /// past a committed intent row, because a failed intent flush short-circuits
    /// with `?` ([`AppError::JournalWriteFailed`], AC-13) before `dispatch` runs.
    ///
    /// This phase's `dispatch` is a no-mutation skeleton that always succeeds, so
    /// the walk journals `intent` then `done` per op ("as if executed"); Phase 3
    /// swaps only the `dispatch` body (adding the real operation and its
    /// [`Journal::write_failed`] branch), leaving this journaling in place.
    pub async fn run<J: Journal>(&self, journal: &J, now: &str) -> Result<ExecOutcome, AppError> {
        let mut produced = Vec::new();
        let mut ops_walked = 0usize;
        for op in self.ops.iter().filter(|o| o.approval == APPROVED) {
            let intent = JournalEntry {
                job_id: self.job_id,
                seq: op.seq,
                op_id: op.id,
                phase: JournalPhase::Intent,
                at: now.to_string(),
                // The F-507 provenance captured at plan time rides the intent row
                // (FD-01, AC-12): pack_path / pack_name / optional award_marker.
                detail_json: op.provenance_json.clone(),
            };
            // Flush the intent BEFORE acting. A failed flush is a hard stop: `?`
            // returns here, so `dispatch` is never reached (AC-13).
            journal.write_intent(&intent).await?;
            produced.push(intent);

            // The act. SKELETON this phase (no mutation); Phase 3 swaps this body.
            self.dispatch(op);

            // Terminal row after the act succeeded.
            let done = JournalEntry {
                job_id: self.job_id,
                seq: op.seq,
                op_id: op.id,
                phase: JournalPhase::Done,
                at: now.to_string(),
                detail_json: None,
            };
            journal.write_done(&done).await?;
            produced.push(done);

            ops_walked += 1;
        }
        Ok(ExecOutcome {
            ops_walked,
            journal: produced,
        })
    }

    /// Per-operation dispatch. SKELETON (Phase 1): it performs NO mutation - it
    /// only exercises the seam with a read-only existence probe through
    /// `self.vfs`, which is also where the later phase's TOCTOU "source exists,
    /// target absent" re-checks live. Everything routes through the `Vfs`; there
    /// is deliberately no direct standard-library filesystem call here (AC-1).
    fn dispatch(&self, op: &PlanOpRow) {
        let _source_present = self.vfs.exists(std::path::Path::new(&op.source_path));
        // Later phase: match on op.kind and route move/rename/quarantine/rmdir
        // through self.vfs (rename same-volume; copy + verify + delete cross-
        // volume), writing the terminal journal row per outcome.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use sqlx::SqlitePool;
    use std::path::Path;
    use tempfile::TempDir;

    /// A `Vfs` whose every method panics: it models a process kill at the exact
    /// moment the executor reaches the filesystem call. Used two ways:
    /// - the kill test lets the intent flush COMMIT, then panics in `dispatch`, so
    ///   the durable journal is left with an intent and no terminal row (AC-10);
    /// - the AC-13 hard-stop test proves `dispatch` is never reached when the
    ///   intent flush fails first - if it were, this would panic.
    struct PanicFs;
    impl Vfs for PanicFs {
        fn exists(&self, _path: &Path) -> bool {
            panic!("simulated kill: the filesystem was reached")
        }
        fn is_dir(&self, _path: &Path) -> bool {
            panic!("simulated kill: the filesystem was reached")
        }
        fn metadata(&self, _path: &Path) -> Result<VfsMetadata, VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
        fn rename(&self, _from: &Path, _to: &Path) -> Result<(), VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
        fn copy_file(&self, _from: &Path, _to: &Path) -> Result<u64, VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
        fn remove_file(&self, _path: &Path) -> Result<(), VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
        fn remove_dir(&self, _path: &Path) -> Result<(), VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
        fn create_dir_all(&self, _path: &Path) -> Result<(), VfsError> {
            panic!("simulated kill: the filesystem was reached")
        }
    }

    /// Open a fresh migrated database and insert the one `jobs` row the journal's
    /// `job_id` foreign key needs, returning the pool and that job id.
    async fn fresh_pool_and_job() -> (TempDir, SqlitePool, i64) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        let job_id =
            sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply','running',?)")
                .bind("2026-07-18T00:00:00Z")
                .execute(&pool)
                .await
                .expect("insert jobs row")
                .last_insert_rowid();
        (dir, pool, job_id)
    }

    fn approved_op(id: i64, seq: i64, source: &str, target: &str) -> PlanOpRow {
        PlanOpRow {
            id,
            plan_id: 1,
            seq,
            op_group: "loose-root-books".to_string(),
            kind: "move".to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test rationale.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    /// AC-1: the executor's operation logic reaches the filesystem ONLY through
    /// the `Vfs` trait, never a direct standard-library filesystem call. This
    /// greps this module's operation logic (everything before the test module);
    /// `vfs.rs`, the one place the real filesystem is touched, is a different file
    /// and excluded by construction. The needle is assembled so the check never
    /// matches its own source.
    #[test]
    fn executor_operation_logic_has_no_direct_std_filesystem_call() {
        const SRC: &str = include_str!("mod.rs");
        let logic = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        let needle = concat!("std", "::", "fs");
        assert!(
            !logic.contains(needle),
            "exec/mod.rs operation logic must go through the Vfs seam, never a \
             direct standard-library filesystem call"
        );
    }

    /// AC-2 substance at the seam level: a dry run walks the approved plan against
    /// a `MemFs` seeded from a snapshot and creates NO real path. The temp-dir
    /// watcher seeds the memory tree with paths UNDER a real temp dir that do not
    /// exist on disk; `MemFs` answers `exists` from memory while the real path is
    /// absent, and after the walk the temp dir is still empty.
    #[tokio::test]
    async fn dry_run_walk_over_memfs_touches_no_real_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        let lib = base.join("lib");
        let src = lib.join("Book.m4b");
        let target = lib.join("Author").join("Book.m4b");

        let seed = vec![
            SeedEntry {
                path: lib.to_string_lossy().into_owned(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: src.to_string_lossy().into_owned(),
                size: 123,
                is_dir: false,
            },
        ];
        let memfs = MemFs::from_seed(&seed);

        // MemFs answers from MEMORY: the seeded path "exists" in the tree even
        // though it was never created on the real filesystem.
        assert!(memfs.exists(&src), "MemFs must answer from its seed");
        assert!(
            !src.exists(),
            "the seeded path must NOT exist on the real filesystem"
        );

        let op = approved_op(1, 0, &src.to_string_lossy(), &target.to_string_lossy());
        let executor = Executor::new(memfs, 7, vec![op]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-17T00:00:00Z")
            .await
            .expect("dry-run walk");

        assert_eq!(outcome.ops_walked, 1);
        // An intent then a done row per walked op.
        assert_eq!(outcome.journal.len(), 2);
        assert_eq!(outcome.journal[0].phase, JournalPhase::Intent);
        assert_eq!(outcome.journal[1].phase, JournalPhase::Done);
        assert_eq!(outcome.journal[0].job_id, 7);
        assert_eq!(outcome.journal[0].op_id, 1);
        assert_eq!(outcome.journal[0].seq, 0);
        // The seam saw the same rows the outcome echoes.
        assert_eq!(journal.entries(), outcome.journal);

        // The temp-dir watcher: a dry run must have created no real path.
        let created = base.read_dir().unwrap().count();
        assert_eq!(created, 0, "a dry run must not create any real path");
    }

    /// Only `approved` operations are walked: pending, rejected, and excluded ops
    /// stay put (the plan review's include/skip decision is the gate).
    #[tokio::test]
    async fn only_approved_ops_are_walked() {
        let approved = approved_op(1, 0, "E:/lib/A.m4b", "E:/lib/Author/A.m4b");
        let mut pending = approved_op(2, 1, "E:/lib/B.m4b", "E:/lib/Author/B.m4b");
        pending.approval = "pending".to_string();
        let mut excluded = approved_op(3, 2, "E:/lib/C.m4b", "E:/lib/Author/C.m4b");
        excluded.approval = "excluded".to_string();

        let executor = Executor::new(MemFs::new(), 1, vec![approved, pending, excluded]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-17T00:00:00Z")
            .await
            .expect("walk");

        assert_eq!(outcome.ops_walked, 1, "only the approved op is executable");
        assert_eq!(outcome.journal.len(), 2, "intent + done for the one op");
        assert_eq!(outcome.journal[0].op_id, 1);
    }

    /// The executor is generic over `RealFs` too (AC-1): it compiles and runs
    /// against the real filesystem backend. With no approved ops the walk does
    /// nothing and touches nothing - the phase never runs a Real op walk (D-09).
    #[tokio::test]
    async fn executor_is_generic_over_realfs() {
        let executor = Executor::new(RealFs::new(), 1, vec![]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-17T00:00:00Z")
            .await
            .expect("empty walk");
        assert_eq!(outcome.ops_walked, 0);
        assert!(outcome.journal.is_empty());
    }

    /// The intent row carries the op's plan-time provenance in `detail_json`
    /// (FD-01, AC-12): the shape the journal persists and the manifest re-exports.
    #[tokio::test]
    async fn intent_row_carries_provenance_detail() {
        let mut op = approved_op(9, 3, "E:/lib/Pack/x.m4b", "E:/lib/Author/x.m4b");
        op.provenance_json = Some(r#"{"pack_path":"E:/lib/Pack","pack_name":"Pack"}"#.to_string());
        let executor = Executor::new(MemFs::new(), 4, vec![op]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-17T00:00:00Z")
            .await
            .expect("walk");
        assert_eq!(outcome.journal[0].phase, JournalPhase::Intent);
        assert_eq!(
            outcome.journal[0].detail_json.as_deref(),
            Some(r#"{"pack_path":"E:/lib/Pack","pack_name":"Pack"}"#)
        );
        // The done row carries no detail.
        assert!(outcome.journal[1].detail_json.is_none());
    }

    /// AC-10 (the brief-mandated kill test): a process killed between the intent
    /// flush and the act leaves EXACTLY ONE op with an intent and NO terminal row.
    /// The intent is flushed and committed through a real `SqliteJournal`; then a
    /// `PanicFs` panics inside `dispatch` (the simulated kill), unwinding past the
    /// `write_done` call that never runs. The panic is caught at the spawned task
    /// boundary so the test can then read the durable journal back.
    #[tokio::test]
    async fn kill_between_intent_and_act_leaves_one_intent_and_no_terminal_row() {
        let (_dir, pool, job_id) = fresh_pool_and_job().await;
        let journal = SqliteJournal::new(pool.clone());
        let op = approved_op(1, 0, "E:\\Books\\Book.m4b", "E:\\Books\\Author\\Book.m4b");
        let executor = Executor::new(PanicFs, job_id, vec![op]);

        // Run on a spawned task so the simulated-kill panic is contained and
        // observable as a JoinError, rather than aborting the test.
        let joined = tokio::spawn(async move {
            executor
                .run(&journal, "2026-07-18T00:00:00Z")
                .await
                .map(|_| ())
        })
        .await;
        assert!(
            joined.is_err(),
            "the walk must panic mid-op (the simulated kill), not return"
        );

        // The intent row is durably committed; there is exactly one, and no
        // terminal (done/failed) row exists for the job. Reconciliation (v0.6.0)
        // is what acts on this shape; this phase only guarantees the shape.
        let rows: Vec<String> =
            sqlx::query_scalar("SELECT phase FROM journal WHERE job_id = ? ORDER BY id")
                .bind(job_id)
                .fetch_all(&pool)
                .await
                .expect("read journal");
        assert_eq!(
            rows,
            vec!["intent".to_string()],
            "exactly one intent, no terminal"
        );
    }

    /// AC-3 excluded-fields documentation: a dry-run and a Real apply produce
    /// identical journal entry sequences EXCEPT for two documented fields - the
    /// `at` phase-timing metadata, and the RealFs/MemFs (mode) marker. This test
    /// pins that the mode marker is NOT a `JournalEntry` field (it lives on the
    /// `jobs.mode` / `manifests.mode` row and the undo file), so the intent/done
    /// row sequences are otherwise byte-identical across modes. The excluded-fields
    /// list is therefore exactly: { `at`, mode marker (external to the row) }.
    #[test]
    fn ac3_journal_rows_carry_no_mode_marker() {
        let entry = JournalEntry {
            job_id: 1,
            seq: 0,
            op_id: 1,
            phase: JournalPhase::Done,
            at: "2026-07-18T00:00:00Z".to_string(),
            detail_json: None,
        };
        let value = serde_json::to_value(&entry).expect("serialize");
        let obj = value.as_object().expect("journal entry is an object");
        // The ONLY keys are the six JournalEntry fields; no mode / dry_run / backend
        // marker rides the row.
        for banned in [
            "mode", "dry_run", "dry-run", "backend", "real", "memfs", "realfs",
        ] {
            assert!(
                !obj.contains_key(banned),
                "a journal row must not carry the AC-3 mode marker (found {banned})"
            );
        }
        let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["at", "detail_json", "job_id", "op_id", "phase", "seq"],
            "the journal row shape is exactly the six JournalEntry fields"
        );
    }

    /// AC-13: a failed intent flush is a HARD STOP. The executor must not proceed
    /// to the filesystem call. Injecting a journal that fails the intent flush and
    /// a `PanicFs` that would panic the instant `dispatch` is reached: the walk
    /// returns `journal-write-failed` WITHOUT panicking, proving `dispatch` was
    /// never reached and no terminal row was produced.
    #[tokio::test]
    async fn a_failed_intent_flush_stops_before_the_filesystem_call() {
        let op = approved_op(1, 0, "E:\\Books\\Book.m4b", "E:\\Books\\Author\\Book.m4b");
        let executor = Executor::new(PanicFs, 5, vec![op]);
        let journal = MemJournal::failing_intent();

        let err = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect_err("a failed intent flush must stop the walk");
        assert_eq!(err.code(), "journal-write-failed");
        assert!(
            journal.entries().is_empty(),
            "no journal row is produced when the intent flush fails"
        );
    }
}
