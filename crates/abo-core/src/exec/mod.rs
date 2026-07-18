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
//! What is real this phase and what is deferred:
//! - Real: the seam ([`Vfs`]/[`RealFs`]/[`MemFs`]), the generic [`Executor`], the
//!   walk over the plan's APPROVED operations, and the journal row TYPES
//!   ([`JournalEntry`] / [`JournalPhase`]).
//! - Skeleton: per-operation dispatch ([`Executor::dispatch`]) performs no
//!   filesystem mutation - it only exercises the seam with a read-only probe. The
//!   rename-first / copy+verify+delete / TOCTOU / never-overwrite operation logic
//!   is a later phase, written test-first against [`MemFs`].
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

pub mod vfs;

use serde::{Deserialize, Serialize};

use crate::db::plans::PlanOpRow;

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

/// One journal row (the Phase 1 decision-gate shape): the durable record of one
/// operation's progress that also becomes the undo manifest (R-5). The next phase
/// persists these to the `journal` table and flushes the intent row before acting;
/// this phase defines the type and the skeleton walk produces the intent rows in
/// memory.
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
/// the journal rows produced. The skeleton produces only [`JournalPhase::Intent`]
/// rows (the terminal rows come with the operation logic in a later phase).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The number of approved operations the executor walked.
    pub ops_walked: usize,
    /// The journal rows produced this walk (intent rows only, this phase).
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

    /// Walk the plan's APPROVED operations in `seq` order, dispatching each, and
    /// return the walk outcome. `now` is the ISO-8601 UTC timestamp stamped on the
    /// journal rows (the core stays clock-free).
    ///
    /// Journal-before-act (R-5): each operation's [`JournalPhase::Intent`] row is
    /// produced BEFORE [`dispatch`](Self::dispatch) runs. This phase's dispatch is
    /// a no-mutation skeleton, so no terminal row is produced yet; the later phase
    /// persists-and-flushes the intent row, then appends the done/failed row.
    pub fn run(&self, now: &str) -> ExecOutcome {
        let mut journal = Vec::new();
        let mut ops_walked = 0usize;
        for op in self.ops.iter().filter(|o| o.approval == APPROVED) {
            journal.push(JournalEntry {
                job_id: self.job_id,
                seq: op.seq,
                op_id: op.id,
                phase: JournalPhase::Intent,
                at: now.to_string(),
                // The F-507 provenance captured at plan time rides the intent row
                // (FD-01, AC-12); the later phase formalizes the JSON shape.
                detail_json: op.provenance_json.clone(),
            });
            self.dispatch(op);
            ops_walked += 1;
        }
        ExecOutcome {
            ops_walked,
            journal,
        }
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
    #[test]
    fn dry_run_walk_over_memfs_touches_no_real_path() {
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
        let outcome = executor.run("2026-07-17T00:00:00Z");

        assert_eq!(outcome.ops_walked, 1);
        assert_eq!(outcome.journal.len(), 1);
        assert_eq!(outcome.journal[0].phase, JournalPhase::Intent);
        assert_eq!(outcome.journal[0].job_id, 7);
        assert_eq!(outcome.journal[0].op_id, 1);
        assert_eq!(outcome.journal[0].seq, 0);

        // The temp-dir watcher: a dry run must have created no real path.
        let created = base.read_dir().unwrap().count();
        assert_eq!(created, 0, "a dry run must not create any real path");
    }

    /// Only `approved` operations are walked: pending, rejected, and excluded ops
    /// stay put (the plan review's include/skip decision is the gate).
    #[test]
    fn only_approved_ops_are_walked() {
        let approved = approved_op(1, 0, "E:/lib/A.m4b", "E:/lib/Author/A.m4b");
        let mut pending = approved_op(2, 1, "E:/lib/B.m4b", "E:/lib/Author/B.m4b");
        pending.approval = "pending".to_string();
        let mut excluded = approved_op(3, 2, "E:/lib/C.m4b", "E:/lib/Author/C.m4b");
        excluded.approval = "excluded".to_string();

        let executor = Executor::new(MemFs::new(), 1, vec![approved, pending, excluded]);
        let outcome = executor.run("2026-07-17T00:00:00Z");

        assert_eq!(outcome.ops_walked, 1, "only the approved op is executable");
        assert_eq!(outcome.journal.len(), 1);
        assert_eq!(outcome.journal[0].op_id, 1);
    }

    /// The executor is generic over `RealFs` too (AC-1): it compiles and runs
    /// against the real filesystem backend. With no approved ops the walk does
    /// nothing and touches nothing - the phase never runs a Real op walk (D-09).
    #[test]
    fn executor_is_generic_over_realfs() {
        let executor = Executor::new(RealFs::new(), 1, vec![]);
        let outcome = executor.run("2026-07-17T00:00:00Z");
        assert_eq!(outcome.ops_walked, 0);
        assert!(outcome.journal.is_empty());
    }

    /// The intent row carries the op's plan-time provenance in `detail_json`
    /// (FD-01, AC-12): the shape the next phase persists and re-exports.
    #[test]
    fn intent_row_carries_provenance_detail() {
        let mut op = approved_op(9, 3, "E:/lib/Pack/x.m4b", "E:/lib/Author/x.m4b");
        op.provenance_json = Some(r#"{"pack_id":"hugo-winners"}"#.to_string());
        let executor = Executor::new(MemFs::new(), 4, vec![op]);
        let outcome = executor.run("2026-07-17T00:00:00Z");
        assert_eq!(
            outcome.journal[0].detail_json.as_deref(),
            Some(r#"{"pack_id":"hugo-winners"}"#)
        );
    }
}
