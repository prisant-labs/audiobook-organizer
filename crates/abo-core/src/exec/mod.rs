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
//! What is real (as of v0.5.0 Phase 3, executor core):
//! - The seam ([`Vfs`]/[`RealFs`]/[`MemFs`]), the generic [`Executor`], the
//!   journal-before-act walk over the plan's APPROVED operations, the journal
//!   seam ([`journal::Journal`]/[`SqliteJournal`]/[`MemJournal`]) that persists
//!   [`JournalEntry`] rows, and the self-contained undo [`manifest`].
//! - Per-operation dispatch ([`Executor::dispatch`]) now performs the REAL
//!   filesystem work through the seam: same-volume `move`/`rename`/`quarantine`
//!   via a metadata-only [`Vfs::rename`] (no byte copy, AC-4), cross-volume via
//!   copy + size verify + delete-source (AC-5), a TOCTOU re-check (source exists /
//!   target absent) before every op (AC-6), never-overwrite (AC-7), and
//!   access-denied retry-once-then-halt (AC-9). A failed op writes a `failed`
//!   journal row (so every `intent` has a terminal row) and HALTS the walk,
//!   surfacing which campaign group stopped ([`ExecOutcome::halt`]).
//! - The single-writer lock ([`lock`]) refuses a second apply while one runs
//!   (AC-8); Real mode is wired at the command boundary (`apply_start`).
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
pub mod lock;
pub mod manifest;
pub mod rollback;
pub mod vfs;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::plans::PlanOpRow;
use crate::error::AppError;
use crate::plan::builder::QUARANTINE_JOB_PLACEHOLDER;

pub use journal::{Journal, MemJournal, SqliteJournal};
pub use manifest::{
    build_manifest, get_manifest_row, Manifest, ManifestError, ManifestOp, ManifestRow, ReverseOp,
    MANIFEST_JSON_BASENAME, MANIFEST_SCHEMA_VERSION,
};
pub use rollback::{rollback_prepare, rollback_prepare_partial};
pub use vfs::{MemFs, RealFs, SeedEntry, Vfs, VfsError, VfsMetadata};

/// The `plan_ops.approval` value that marks an operation executable. Only
/// approved operations are walked (the plan review's include/skip decision is the
/// gate; everything else stays put).
const APPROVED: &str = "approved";

/// The operation kinds [`Executor::dispatch`] understands, which is EXACTLY the
/// set the plan builder emits (`mkdir`, `move`, `rename`, `quarantine`,
/// `rmdir-empty`, `no-op`; see `crate::plan::builder`). Frozen alongside the
/// manifest's reversible-kind list ([`manifest`]'s `is_reversible_kind`): a test
/// pins the two together so a new op kind cannot be dispatched without also being
/// classified reversible-or-not, and dispatch cannot silently ignore a kind the
/// builder emits.
pub const DISPATCH_OP_KINDS: &[&str] = &[
    "mkdir",
    "move",
    "rename",
    "quarantine",
    "rmdir-empty",
    "no-op",
];

/// Which filesystem an apply runs against - the `mode` argument of `apply_start`.
///
/// [`DryRun`](ApplyMode::DryRun) walks the plan against a [`MemFs`] seeded from
/// the snapshot (a first-class preview, D-04); [`Real`](ApplyMode::Real) walks it
/// against [`RealFs`], the actual disk. The SAME executor code path serves both
/// (the Vfs seam), which is what makes the dry run a faithful rehearsal of the
/// real apply (R-1). v0.5.0 Phase 3 flips Real mode on at the command boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ApplyMode {
    /// Walk against memory; touch no real path.
    DryRun,
    /// Walk against the real filesystem (the actual disk).
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

/// Why and where an executor walk halted (AC-5/6/7/9). Carries the stable
/// taxonomy code the journal `failed` row also used, the operation that failed,
/// the user-facing campaign group (slug) that stopped (FD-26), and the op's paths,
/// so the caller can mark the job failed with the right code and tell the user
/// which group halted. "Halt the group" halts the whole walk because this release
/// executes one approved plan as one job (the plan module does not model per-group
/// runs), so the first failing op stops the run with the journal left consistent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecHalt {
    /// The stable error code: `source-vanished`, `target-appeared`,
    /// `copy-verify-mismatch`, `access-denied`, or `apply-failed`.
    pub code: &'static str,
    /// The `plan_ops.id` of the operation that failed.
    pub op_id: i64,
    /// The failed operation's `plan_ops.seq`.
    pub seq: i64,
    /// The user-facing campaign group (slug) the failed op belongs to.
    pub group: String,
    /// The failed operation's source path.
    pub source_path: String,
    /// The failed operation's target path.
    pub target_path: String,
    /// Developer-facing detail, also stored in the journal `failed` row's
    /// `detail_json` (content only - no new row field, no mode marker, so AC-3
    /// dry-run == Real equality holds).
    pub detail: String,
}

/// The result of an executor walk: how many approved operations completed, the
/// journal rows produced (an in-memory echo of what was flushed through the
/// [`Journal`] seam), and an optional [`ExecHalt`] if an operation failed and
/// stopped the walk. On a clean walk `halt` is `None` and there is an
/// [`JournalPhase::Intent`] + [`JournalPhase::Done`] row per op; on a halt the
/// last op contributes an `intent` + `failed` pair, so EVERY intent still has a
/// terminal row (the journal-consistency invariant AC-6/AC-7 assert). The only
/// `Err` [`Executor::run`] returns is the AC-13 hard stop (a failed intent flush);
/// an operation failure is a normal, journaled outcome reported via `halt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The number of approved operations that completed (before any halt).
    pub ops_walked: usize,
    /// The journal rows produced this walk (intent + done per completed op, and an
    /// intent + failed pair for a halting op).
    pub journal: Vec<JournalEntry>,
    /// `Some` when an operation failed and halted the walk; `None` on a clean walk.
    pub halt: Option<ExecHalt>,
}

/// The apply-time scope (FD-34): the library root and the resolved set-aside
/// root, the ONLY two areas a walk may write into. Carried on the executor when
/// built with [`Executor::with_scope`] (the production apply path); it drives two
/// things: the [`QUARANTINE_JOB_PLACEHOLDER`] -> real-`job-id` substitution in
/// set-aside targets, and the structural scope guard that refuses any op whose
/// target lands outside both roots before a single filesystem call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyScope {
    /// The library root the plan was built over (every ordinary target sits
    /// inside it).
    pub library_root: String,
    /// The resolved set-aside root (a sibling of the library, OUTSIDE it), the
    /// ONE place a target may land outside the library (FD-34).
    pub set_aside_root: String,
}

impl ApplyScope {
    /// Whether `target` is at or under one of the two permitted roots.
    fn permits(&self, target: &str) -> bool {
        path_at_or_under(target, &self.library_root)
            || path_at_or_under(target, &self.set_aside_root)
    }

    /// FD-34 structural scope guard: an op that CREATES a target (`mkdir`, `move`,
    /// `rename`, `quarantine`) must land inside the library root or under the
    /// set-aside root. Anything else is refused BEFORE any filesystem call, as a
    /// journaled halt (the walk writes the failed row and stops). The builder
    /// never emits an out-of-scope target, so this fires only on a corrupted or
    /// tampered plan - the last structural backstop before the disk.
    fn check(&self, op: &PlanOpRow) -> Result<(), OpHalt> {
        let creates = matches!(op.kind.as_str(), "mkdir" | "move" | "rename" | "quarantine");
        if creates && !op.target_path.is_empty() && !self.permits(&op.target_path) {
            return Err(OpHalt {
                code: "out-of-scope-target",
                detail: format!(
                    "refused: the target lands outside the library and the set-aside area: {}",
                    op.target_path
                ),
            });
        }
        Ok(())
    }
}

/// Whether `target` equals `root` or sits under it, compared case- and
/// separator-insensitively (NTFS reality), matching the plan/validate scope
/// check so the two agree on what "in scope" means.
fn path_at_or_under(target: &str, root: &str) -> bool {
    let t = norm_scope_path(target);
    let r = norm_scope_path(root);
    t == r || t.starts_with(&format!("{r}/"))
}

/// Normalize a path for scope comparison: backslashes to forward slashes,
/// lowercased, no trailing separator.
fn norm_scope_path(p: &str) -> String {
    let mut s = p.replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// The executor: consumes an approved plan and a [`Vfs`], and walks the plan's
/// operations against that filesystem. Generic over `V` so the identical code
/// serves a dry run ([`MemFs`]) and a Real apply ([`RealFs`]) - the seam that
/// makes the dry run a first-class product (R-1).
pub struct Executor<V: Vfs> {
    vfs: V,
    job_id: i64,
    ops: Vec<PlanOpRow>,
    /// The FD-34 apply scope (set-aside root + library root). `None` for a
    /// scope-agnostic unit test built via [`Executor::new`]; `Some` on the
    /// production apply path ([`Executor::with_scope`]), which also substitutes
    /// the real job id into set-aside targets.
    scope: Option<ApplyScope>,
}

impl<V: Vfs> Executor<V> {
    /// Build an executor over `vfs` for apply `job_id`, holding the plan's `ops`
    /// (all of them; the walk selects the approved ones). No apply scope: the
    /// FD-34 substitution and scope guard are inert (used by scope-agnostic unit
    /// tests). The production apply path uses [`Executor::with_scope`].
    pub fn new(vfs: V, job_id: i64, ops: Vec<PlanOpRow>) -> Self {
        Self {
            vfs,
            job_id,
            ops,
            scope: None,
        }
    }

    /// Build an executor WITH its FD-34 apply scope (the production path). Two
    /// things happen here, both keyed off the real `job_id`:
    ///
    /// 1. **Job-id substitution.** Every set-aside target the builder stamped with
    ///    [`QUARANTINE_JOB_PLACEHOLDER`] gets the real `jobs.id` spliced into that
    ///    path segment, so this run's set-aside items land in their own collision-
    ///    free `<set-aside-root>\<job-id>\` folder (FD-34). This is done on the
    ///    executor's OWN in-memory copy of the ops; the frozen `plan_ops` row is
    ///    never rewritten (plan immutability). Because the job-id segment lives in
    ///    the TARGET path and a journal row carries no target, a dry run and a Real
    ///    apply still produce byte-identical journal sequences (AC-3) even though
    ///    each substitutes its own job id.
    /// 2. **Scope guard.** `scope` is retained so [`Self::dispatch`] refuses any op
    ///    whose target escapes both roots before touching the disk.
    pub fn with_scope(vfs: V, job_id: i64, ops: Vec<PlanOpRow>, scope: ApplyScope) -> Self {
        let job_seg = job_id.to_string();
        let ops = ops
            .into_iter()
            .map(|mut op| {
                if op.target_path.contains(QUARANTINE_JOB_PLACEHOLDER) {
                    op.target_path = op.target_path.replace(QUARANTINE_JOB_PLACEHOLDER, &job_seg);
                }
                op
            })
            .collect();
        Self {
            vfs,
            job_id,
            ops,
            scope: Some(scope),
        }
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
    /// On an operation failure, [`dispatch`](Self::dispatch) returns an [`OpHalt`];
    /// the walk writes a [`JournalPhase::Failed`] terminal row (so the intent still
    /// has a terminal) and STOPS, reporting the halt in [`ExecOutcome::halt`]
    /// (halting a group halts the run, since one approved plan is one job).
    pub async fn run<J: Journal>(&self, journal: &J, now: &str) -> Result<ExecOutcome, AppError> {
        let mut produced = Vec::new();
        let mut ops_walked = 0usize;
        let mut halt: Option<ExecHalt> = None;
        for op in self.ops.iter().filter(|o| o.approval == APPROVED) {
            let intent = JournalEntry {
                job_id: self.job_id,
                seq: op.seq,
                op_id: op.id,
                phase: JournalPhase::Intent,
                at: now.to_string(),
                // The intent row's detail is: for a set-aside (`quarantine`) op, the
                // AC-22 set-aside record (reason + original path + relative path); for
                // a flatten-packs member move, the F-507 pack/award provenance captured
                // at plan time (FD-01, AC-12). Both are plan-derived and mode-
                // independent, so a dry run and a Real apply produce identical detail
                // (AC-3), and neither carries a job id (that lives on the row's
                // `job_id` field, not the detail).
                detail_json: self.intent_detail_json(op),
            };
            // Flush the intent BEFORE acting. A failed flush is a hard stop: `?`
            // returns here, so `dispatch` is never reached (AC-13). The sequencing
            // is STRUCTURAL: `dispatch` cannot run except past this committed row.
            journal.write_intent(&intent).await?;
            produced.push(intent);

            match self.dispatch(op) {
                Ok(()) => {
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
                Err(op_halt) => {
                    // Terminal `failed` row so every intent has a terminal row (the
                    // journal-consistency invariant AC-6/AC-7 assert). The failure
                    // code + detail ride detail_json (content only - AC-3-safe: no
                    // new row field, no mode marker).
                    let failed = JournalEntry {
                        job_id: self.job_id,
                        seq: op.seq,
                        op_id: op.id,
                        phase: JournalPhase::Failed,
                        at: now.to_string(),
                        detail_json: Some(failure_detail_json(op_halt.code, &op_halt.detail)),
                    };
                    journal.write_failed(&failed).await?;
                    produced.push(failed);
                    halt = Some(ExecHalt {
                        code: op_halt.code,
                        op_id: op.id,
                        seq: op.seq,
                        group: group_slug_of(op),
                        source_path: op.source_path.clone(),
                        target_path: op.target_path.clone(),
                        detail: op_halt.detail,
                    });
                    break;
                }
            }
        }
        Ok(ExecOutcome {
            ops_walked,
            journal: produced,
            halt,
        })
    }

    /// The intent row's `detail_json` for `op`. A `quarantine` (set-aside) op with
    /// a configured scope carries the AC-22 set-aside record (reason + original
    /// path + original relative path); every other op carries the plan-time
    /// provenance the builder stamped (`op.provenance_json`, the F-507 pack/award
    /// data on flatten-packs member moves). The set-aside record is derived purely
    /// from the frozen op fields and the library root, so it is identical across a
    /// dry run and a Real apply (AC-3) and carries no job id.
    fn intent_detail_json(&self, op: &PlanOpRow) -> Option<String> {
        if op.kind == "quarantine" {
            if let Some(scope) = &self.scope {
                return Some(set_aside_record_json(op, &scope.library_root));
            }
        }
        op.provenance_json.clone()
    }

    /// Per-operation dispatch (AC-1: reaches the filesystem ONLY through
    /// `self.vfs`, never a direct standard-library call - a unit test greps this
    /// module's logic to keep that true). Routes each op by kind:
    /// - `no-op` - a documented no-op (staging / manual-review), does nothing;
    /// - `mkdir` - create the target directory and its ancestors;
    /// - `move` / `rename` / `quarantine` - a source-to-target move: same-volume
    ///   rename (AC-4) or cross-volume copy+verify+delete (AC-5), gated by the
    ///   TOCTOU re-check (AC-6) and never-overwrite (AC-7);
    /// - `rmdir-empty` - remove a now-empty folder.
    ///
    /// Returns `Ok(())` on success or an [`OpHalt`] carrying the stable code the
    /// walk journals and surfaces. Never overwrites: it only ever calls
    /// [`Vfs::rename`] / [`Vfs::copy_file`] (both refuse a present target) and
    /// [`Vfs::create_dir_all`] (idempotent), and the seam exposes NO
    /// open-for-write or truncate primitive, so "overwrite" is not expressible
    /// from here.
    fn dispatch(&self, op: &PlanOpRow) -> Result<(), OpHalt> {
        // FD-34 structural scope guard: refuse an out-of-scope target BEFORE any
        // filesystem call (the caller journals the halt). Inert when no scope was
        // configured (scope-agnostic unit tests via `Executor::new`).
        if let Some(scope) = &self.scope {
            scope.check(op)?;
        }
        match op.kind.as_str() {
            "no-op" => Ok(()),
            "mkdir" => self.op_mkdir(op),
            "rmdir-empty" => self.op_rmdir(op),
            "move" | "rename" | "quarantine" => self.op_move(op),
            other => Err(OpHalt {
                code: "apply-failed",
                detail: format!("unsupported operation kind '{other}'"),
            }),
        }
    }

    /// A source-to-target move (`move`/`rename`/`quarantine`): TOCTOU re-check,
    /// mkdir-first, then same-volume rename (AC-4) or cross-volume copy+verify+
    /// delete (AC-5).
    fn op_move(&self, op: &PlanOpRow) -> Result<(), OpHalt> {
        let source = Path::new(&op.source_path);
        let target = Path::new(&op.target_path);

        // TOCTOU backstop (AC-6), re-checked immediately before acting. NTFS-case-
        // insensitive matching comes from the Vfs (MemFs normalizes keys; RealFs's
        // exists() asks the case-insensitive filesystem).
        if !self.vfs.exists(source) {
            return Err(OpHalt::source_vanished());
        }
        if self.vfs.exists(target) {
            // Never-overwrite gate #1 (AC-6/AC-7): a present target halts before the
            // move is ever attempted, so nothing is clobbered.
            return Err(OpHalt::target_appeared());
        }

        // mkdir-first (P1 obligation): create the target's parent directories
        // before any rename/copy, so a missing parent never diverges MemFs
        // (auto-parents on rename) from RealFs (errors on a missing parent).
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                self.retrying(|| self.vfs.create_dir_all(parent))
                    .map_err(map_vfs)?;
            }
        }

        if crate::paths::same_volume(source, target) {
            // AC-4: same volume completes as a metadata-only rename, no byte copy.
            // Never-overwrite gate #2 (defense in depth): Vfs::rename itself refuses
            // a present target, so even a target that raced in AFTER the check above
            // cannot be overwritten - it surfaces as AlreadyExists -> target-appeared.
            self.retrying(|| self.vfs.rename(source, target))
                .map_err(map_vfs)?;
        } else {
            // AC-5: cross-volume copy + verify + delete-source, in that order.
            self.cross_volume_move(source, target)?;
        }
        Ok(())
    }

    /// Cross-volume move (AC-5): copy, then size-verify, then delete the source, in
    /// that order. A verify mismatch removes our own unverified copy and halts with
    /// `copy-verify-mismatch`, leaving the SOURCE intact.
    fn cross_volume_move(&self, source: &Path, target: &Path) -> Result<(), OpHalt> {
        // Capture the source size before the copy, for the post-copy verify.
        let src = self
            .retrying(|| self.vfs.metadata(source))
            .map_err(map_vfs)?;
        // Copy (never-overwrite: copy_file refuses an existing target).
        let copied = self
            .retrying(|| self.vfs.copy_file(source, target))
            .map_err(map_vfs)?;
        // Size verify (AC-5). Hash verify "where the plan marked it": plan_ops
        // carries NO hash-marker column (frozen schema), so size-verify is the whole
        // check this release; hash verify is deferred to F-702 (OQ-2), and the Vfs
        // seam grows no hash primitive here.
        let tgt = self
            .retrying(|| self.vfs.metadata(target))
            .map_err(map_vfs)?;
        if copied != src.size || tgt.size != src.size {
            // Roll back our own unverified copy so the op leaves no trace, and leave
            // the SOURCE intact (AC-5). Best-effort: a failed cleanup does not change
            // the verdict - the source is what safety turns on.
            let _ = self.vfs.remove_file(target);
            return Err(OpHalt::copy_verify_mismatch(src.size, tgt.size));
        }
        // Delete the source ONLY after the copy verified (AC-5 order).
        self.retrying(|| self.vfs.remove_file(source))
            .map_err(map_vfs)?;
        Ok(())
    }

    /// Create the target directory (and its ancestors); idempotent.
    fn op_mkdir(&self, op: &PlanOpRow) -> Result<(), OpHalt> {
        let target = Path::new(&op.target_path);
        self.retrying(|| self.vfs.create_dir_all(target))
            .map_err(map_vfs)
    }

    /// Remove a now-empty folder. Idempotent at the edges: an already-absent folder
    /// is the desired end state (success). Never removes a non-empty folder - if
    /// content raced in, it halts with the journal left consistent (P1 obligation:
    /// bound the check-then-remove window; the seam itself refuses the removal, so
    /// this is journaling, not clobbering).
    fn op_rmdir(&self, op: &PlanOpRow) -> Result<(), OpHalt> {
        let source = Path::new(&op.source_path);
        if !self.vfs.exists(source) {
            return Ok(());
        }
        match self.retrying(|| self.vfs.remove_dir(source)) {
            Ok(()) => Ok(()),
            // Raced away between the check and the remove: still the desired state.
            Err(VfsError::NotFound(_)) => Ok(()),
            // Content raced IN: never remove a non-empty folder.
            Err(e @ VfsError::DirectoryNotEmpty(_)) => Err(OpHalt {
                code: "target-appeared",
                detail: format!("the folder is no longer empty: {e}"),
            }),
            Err(e) => Err(map_vfs(e)),
        }
    }

    /// Run a single Vfs operation with the access-denied retry-once policy (AC-9):
    /// if the first attempt fails with an access-denied [`VfsError::Io`], retry
    /// EXACTLY once and return the second attempt's result (whatever it is). Any
    /// other error returns immediately with no retry. Two attempts is the hard cap;
    /// a second access-denied propagates and the caller halts the group.
    ///
    /// The retry is PER-Vfs-CALL, not per-operation: each seam call an op makes
    /// (`create_dir_all`, `rename`, or the `metadata`/`copy_file`/`remove_file` of a
    /// cross-volume move) is wrapped separately, so a multi-call op is bounded by two
    /// attempts PER call, not two attempts for the whole op. That is deliberate: each
    /// call is an independent chance for a transient access-denied to clear, and the
    /// total stays bounded by the small, fixed number of seam calls an op makes.
    fn retrying<T>(&self, mut op: impl FnMut() -> Result<T, VfsError>) -> Result<T, VfsError> {
        let first = op();
        match first {
            Err(ref e) if is_access_denied(e) => op(),
            _ => first,
        }
    }
}

/// A per-operation failure that halts the walk. Carries the stable taxonomy code
/// the journal `failed` row and the surfaced [`ExecHalt`] both use, plus a
/// developer-facing detail. Not public: the walk turns it into an [`ExecHalt`].
struct OpHalt {
    code: &'static str,
    detail: String,
}

impl OpHalt {
    fn source_vanished() -> Self {
        Self {
            code: "source-vanished",
            detail: "the source is no longer present".to_string(),
        }
    }

    fn target_appeared() -> Self {
        Self {
            code: "target-appeared",
            detail: "something already exists at the target".to_string(),
        }
    }

    fn copy_verify_mismatch(expected: u64, actual: u64) -> Self {
        Self {
            code: "copy-verify-mismatch",
            detail: format!("copied size {actual} did not match source size {expected}"),
        }
    }
}

/// The journal `failed` row's `detail_json`: the stable failure code plus its
/// developer-facing detail, as a small JSON object. Content only, so AC-3 dry-run
/// == Real equality is undisturbed (the row shape and key set are unchanged).
fn failure_detail_json(code: &str, detail: &str) -> String {
    serde_json::json!({ "code": code, "detail": detail }).to_string()
}

/// The AC-22 set-aside record for a `quarantine` op's intent row: the reason it
/// was set aside (the op's plain-language rationale, which already encodes
/// duplicate-of / non-preferred-format / clutter), the original absolute path,
/// and the original path RELATIVE to the library root - so the reason and the
/// original relative path are both recoverable from the quarantine record alone
/// (the job id itself is the intent row's `job_id` field). Deterministic key
/// order (serde_json's sorted map), and no job id inside, so it is byte-identical
/// across a dry run and a Real apply (AC-3).
fn set_aside_record_json(op: &PlanOpRow, library_root: &str) -> String {
    let rel = relative_to(&op.source_path, library_root);
    serde_json::json!({
        "set_aside_reason": op.rationale,
        "original_path": op.source_path,
        "original_relative_path": rel,
    })
    .to_string()
}

/// `path` relative to `root` (the root prefix and one leading separator removed);
/// unchanged if it does not sit under the root. Exact-prefix match: a set-aside
/// op's source and the library root come from the same stored snapshot, so they
/// share a spelling.
fn relative_to(path: &str, root: &str) -> String {
    match path.strip_prefix(root) {
        Some(rest) => rest.trim_start_matches(['/', '\\']).to_string(),
        None => path.to_string(),
    }
}

/// The user-facing campaign group (slug) an op belongs to (FD-26), for surfacing
/// which group halted. Falls back to the raw `op_group` if it is unrecognized
/// (never fabricates a group).
fn group_slug_of(op: &PlanOpRow) -> String {
    crate::plan::builder::group_for_op_group(&op.op_group)
        .map(|g| g.slug().to_string())
        .unwrap_or_else(|| op.op_group.clone())
}

/// Whether a [`VfsError`] is an access-denied platform error - the one error the
/// executor retries once (AC-9). Only [`VfsError::Io`] with
/// [`std::io::ErrorKind::PermissionDenied`] qualifies; the structured variants are
/// deterministic outcomes, never retried.
fn is_access_denied(err: &VfsError) -> bool {
    matches!(err, VfsError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied)
}

/// Map a residual [`VfsError`] from a real operation onto the halt taxonomy
/// (FD-19): a vanished source is `source-vanished`, a present target (never-
/// overwrite) is `target-appeared`, a still-denied access is `access-denied` (the
/// retry is already spent by the time this is reached), and any other platform
/// error halts under `apply-failed` with its detail.
fn map_vfs(err: VfsError) -> OpHalt {
    match err {
        VfsError::NotFound(p) => OpHalt {
            code: "source-vanished",
            detail: format!("path not found during the change: {}", p.display()),
        },
        VfsError::AlreadyExists(p) => OpHalt {
            code: "target-appeared",
            detail: format!("path already exists: {}", p.display()),
        },
        VfsError::Io(e) if e.kind() == std::io::ErrorKind::PermissionDenied => OpHalt {
            code: "access-denied",
            detail: format!("access denied: {e}"),
        },
        other => OpHalt {
            code: "apply-failed",
            detail: format!("filesystem error during the change: {other}"),
        },
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
        assert!(outcome.halt.is_none(), "a clean dry-run walk does not halt");
        // An intent then a done row per walked op.
        assert_eq!(outcome.journal.len(), 2);
        assert_eq!(outcome.journal[0].phase, JournalPhase::Intent);
        assert_eq!(outcome.journal[1].phase, JournalPhase::Done);
        assert_eq!(outcome.journal[0].job_id, 7);
        assert_eq!(outcome.journal[0].op_id, 1);
        assert_eq!(outcome.journal[0].seq, 0);
        // The seam saw the same rows the outcome echoes.
        assert_eq!(journal.entries(), outcome.journal);

        // STRENGTHENED (P1 obligation): the dispatch REALLY executed against the
        // MemFs (this is no longer a no-op skeleton), so the move happened in
        // memory - source gone, target present (its parent created mkdir-first) -
        // proving the op ran, not that it was skipped.
        let memfs = executor.vfs();
        assert!(!memfs.exists(&src), "the move emptied the source in memory");
        assert!(
            memfs.exists(&target),
            "the move created the target in memory"
        );
        assert!(
            memfs.is_dir(&lib.join("Author")),
            "mkdir-first created the target's parent in memory"
        );

        // The temp-dir watcher: even with REAL dispatch, a dry run created no real
        // path - MemFs is disk-inert by construction, so the temp dir is untouched.
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

        // Seed only the APPROVED op's source; the move then succeeds and the
        // pending/excluded sources need not exist because those ops are never walked.
        let memfs = MemFs::from_seed(&[
            SeedEntry {
                path: "E:/lib".to_string(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/A.m4b".to_string(),
                size: 10,
                is_dir: false,
            },
        ]);
        let executor = Executor::new(memfs, 1, vec![approved, pending, excluded]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-17T00:00:00Z")
            .await
            .expect("walk");

        assert_eq!(outcome.ops_walked, 1, "only the approved op is executable");
        assert!(outcome.halt.is_none(), "the approved op succeeds");
        assert_eq!(outcome.journal.len(), 2, "intent + done for the one op");
        assert_eq!(outcome.journal[0].op_id, 1);
        // The pending/excluded sources were never touched.
        assert!(executor.vfs().exists(Path::new("E:/lib/Author/A.m4b")));
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
        // Seed the source so the move succeeds and the terminal row is `done`.
        let memfs = MemFs::from_seed(&[
            SeedEntry {
                path: "E:/lib/Pack".to_string(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/Pack/x.m4b".to_string(),
                size: 7,
                is_dir: false,
            },
        ]);
        let executor = Executor::new(memfs, 4, vec![op]);
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

    // ---- Phase 3: executor-core operation logic (AC-4..AC-9) ----

    use std::cell::Cell;
    use std::path::PathBuf;

    /// An access-denied [`VfsError`] (the one error the executor retries once).
    fn denied() -> VfsError {
        VfsError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }

    /// One fault-injection `Vfs` wrapping a real [`MemFs`], with knobs for the
    /// Phase-3 adversarial tests. It attacks the REAL seam (delegating to the inner
    /// `MemFs` so never-overwrite, size verify, and the error contract all come from
    /// production code), only perturbing the specific behavior each test needs:
    /// - counts `rename`/`copy_file` calls (AC-4 rename-no-copy, AC-5 copy path);
    /// - denies the first N `rename` calls with access-denied (AC-9 retry-once);
    /// - over-reports a target file's size on `metadata` (AC-5 verify mismatch);
    /// - on the first target-absent `exists` probe, has a concurrent writer create a
    ///   distinct victim at the target, then answers "absent" so the executor's
    ///   TOCTOU check passes and the following rename races into a now-present target
    ///   (AC-7 never-overwrite).
    struct FaultFs {
        inner: MemFs,
        renames: Cell<usize>,
        copies: Cell<usize>,
        rename_calls: Cell<u32>,
        deny_renames: Cell<u32>,
        corrupt_target: Option<PathBuf>,
        race_target: Option<PathBuf>,
        race_victim: Option<PathBuf>,
        raced: Cell<bool>,
    }

    impl FaultFs {
        fn new(inner: MemFs) -> Self {
            Self {
                inner,
                renames: Cell::new(0),
                copies: Cell::new(0),
                rename_calls: Cell::new(0),
                deny_renames: Cell::new(0),
                corrupt_target: None,
                race_target: None,
                race_victim: None,
                raced: Cell::new(false),
            }
        }
        fn deny_renames(self, n: u32) -> Self {
            self.deny_renames.set(n);
            self
        }
        fn corrupt_target(mut self, p: &Path) -> Self {
            self.corrupt_target = Some(p.to_path_buf());
            self
        }
        fn race(mut self, target: &Path, victim: &Path) -> Self {
            self.race_target = Some(target.to_path_buf());
            self.race_victim = Some(victim.to_path_buf());
            self
        }
    }

    impl Vfs for FaultFs {
        fn exists(&self, path: &Path) -> bool {
            let present = self.inner.exists(path);
            if let Some(t) = &self.race_target {
                if path == t.as_path() && !present && !self.raced.get() {
                    self.raced.set(true);
                    // A concurrent writer creates a DISTINCT file at the target
                    // (a copy of the victim, a different size than our source), so a
                    // later assertion can prove the executor never overwrote it.
                    let victim = self.race_victim.as_ref().expect("race victim");
                    self.inner
                        .copy_file(victim, t)
                        .expect("the adversary creates the target");
                    // Answer the PRE-race value: the executor's absent-check passes,
                    // then its rename races into the now-present target.
                    return false;
                }
            }
            present
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.inner.is_dir(path)
        }
        fn metadata(&self, path: &Path) -> Result<VfsMetadata, VfsError> {
            let m = self.inner.metadata(path)?;
            if let Some(t) = &self.corrupt_target {
                if path == t.as_path() && !m.is_dir {
                    // The copy is corrupt: report a size that does not match the
                    // source, forcing the size-verify to fail (AC-5).
                    return Ok(VfsMetadata {
                        size: m.size + 7,
                        is_dir: false,
                    });
                }
            }
            Ok(m)
        }
        fn rename(&self, from: &Path, to: &Path) -> Result<(), VfsError> {
            self.rename_calls.set(self.rename_calls.get() + 1);
            let d = self.deny_renames.get();
            if d > 0 {
                self.deny_renames.set(d - 1);
                return Err(denied());
            }
            self.renames.set(self.renames.get() + 1);
            self.inner.rename(from, to)
        }
        fn copy_file(&self, from: &Path, to: &Path) -> Result<u64, VfsError> {
            self.copies.set(self.copies.get() + 1);
            self.inner.copy_file(from, to)
        }
        fn remove_file(&self, path: &Path) -> Result<(), VfsError> {
            self.inner.remove_file(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), VfsError> {
            self.inner.remove_dir(path)
        }
        fn create_dir_all(&self, path: &Path) -> Result<(), VfsError> {
            self.inner.create_dir_all(path)
        }
    }

    fn dir(path: &str) -> SeedEntry {
        SeedEntry {
            path: path.to_string(),
            size: 0,
            is_dir: true,
        }
    }
    fn file(path: &str, size: u64) -> SeedEntry {
        SeedEntry {
            path: path.to_string(),
            size,
            is_dir: false,
        }
    }

    /// Build a one-op executor over `fs` (job 1) walking a single approved `move`
    /// from `src` to `tgt`, plus its journal, and run it.
    async fn walk_one<V: Vfs>(
        fs: V,
        src: &str,
        tgt: &str,
    ) -> (Executor<V>, MemJournal, ExecOutcome) {
        let op = approved_op(1, 0, src, tgt);
        let executor = Executor::new(fs, 1, vec![op]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("walk");
        (executor, journal, outcome)
    }

    /// Assert the produced journal is internally consistent: every `intent` row has
    /// a matching terminal (`done` or `failed`) row for the same op_id (AC-6/AC-7).
    fn assert_journal_consistent(journal: &[JournalEntry]) {
        for row in journal.iter().filter(|r| r.phase == JournalPhase::Intent) {
            let has_terminal = journal.iter().any(|r| {
                r.op_id == row.op_id && matches!(r.phase, JournalPhase::Done | JournalPhase::Failed)
            });
            assert!(
                has_terminal,
                "intent for op {} has no terminal row",
                row.op_id
            );
        }
    }

    /// AC-4: a same-volume `move` completes via filesystem rename with NO byte copy
    /// (asserted by the MemFs-backed rename/copy counters).
    #[tokio::test]
    async fn ac4_same_volume_move_is_a_rename_with_no_copy() {
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
        ]));
        let (executor, _j, outcome) = walk_one(fs, "E:/lib/A.m4b", "E:/lib/Author/A.m4b").await;
        assert!(outcome.halt.is_none());
        assert_eq!(outcome.ops_walked, 1);
        let fs = executor.vfs();
        assert_eq!(
            fs.renames.get(),
            1,
            "a same-volume move renames exactly once"
        );
        assert_eq!(
            fs.copies.get(),
            0,
            "a same-volume move copies NO bytes (AC-4)"
        );
        assert!(!fs.exists(Path::new("E:/lib/A.m4b")), "source moved away");
        assert!(
            fs.exists(Path::new("E:/lib/Author/A.m4b")),
            "target present"
        );
    }

    /// AC-5 (happy path): a cross-volume move performs copy + verify + delete-source
    /// in that order (copy counter fires, no rename), leaving the source gone and the
    /// verified target present.
    #[tokio::test]
    async fn ac5_cross_volume_move_copies_verifies_and_deletes_source() {
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
        ]));
        // Distinct drive letters -> cross-volume.
        let (executor, _j, outcome) = walk_one(fs, "E:/lib/A.m4b", "F:/dest/A.m4b").await;
        assert!(outcome.halt.is_none());
        let fs = executor.vfs();
        assert_eq!(fs.copies.get(), 1, "a cross-volume move copies once (AC-5)");
        assert_eq!(fs.renames.get(), 0, "a cross-volume move never renames");
        assert!(
            !fs.exists(Path::new("E:/lib/A.m4b")),
            "source deleted after verify"
        );
        assert!(
            fs.exists(Path::new("F:/dest/A.m4b")),
            "verified target present"
        );
    }

    /// AC-5 (mismatch): a cross-volume copy whose verify fails halts with
    /// `copy-verify-mismatch` and leaves the SOURCE intact (the unverified copy is
    /// rolled back), with a consistent journal.
    #[tokio::test]
    async fn ac5_cross_volume_verify_mismatch_halts_and_leaves_source() {
        let target = Path::new("F:/dest/A.m4b");
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
        ]))
        .corrupt_target(target);
        let (executor, journal, outcome) = walk_one(fs, "E:/lib/A.m4b", "F:/dest/A.m4b").await;

        let halt = outcome.halt.expect("a verify mismatch halts");
        assert_eq!(halt.code, "copy-verify-mismatch");
        assert_eq!(outcome.ops_walked, 0, "the mismatched op did not complete");
        let fs = executor.vfs();
        assert!(
            fs.exists(Path::new("E:/lib/A.m4b")),
            "the source is left intact on a verify mismatch (AC-5)"
        );
        assert!(
            !fs.exists(target),
            "our own unverified copy is rolled back, leaving no partial target"
        );
        // Journal: intent + failed for the one op, and it is consistent.
        assert_eq!(journal.entries().len(), 2);
        assert_eq!(journal.entries()[1].phase, JournalPhase::Failed);
        assert_journal_consistent(&journal.entries());
    }

    /// AC-6 (source vanished): a source absent at the pre-op re-check halts with
    /// `source-vanished` and a consistent journal.
    #[tokio::test]
    async fn ac6_source_vanished_halts_the_group() {
        // Empty tree: the source does not exist when the executor re-checks.
        let fs = FaultFs::new(MemFs::new());
        let (_e, journal, outcome) =
            walk_one(fs, "E:/lib/Gone.m4b", "E:/lib/Author/Gone.m4b").await;
        let halt = outcome.halt.expect("a vanished source halts");
        assert_eq!(halt.code, "source-vanished");
        assert_eq!(halt.source_path, "E:/lib/Gone.m4b");
        assert_eq!(
            journal.entries().last().unwrap().phase,
            JournalPhase::Failed
        );
        assert_journal_consistent(&journal.entries());
    }

    /// AC-6 (target appeared): a target present at the pre-op re-check halts with
    /// `target-appeared` and never touches it.
    #[tokio::test]
    async fn ac6_target_appeared_halts_the_group() {
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
            // The target already exists at re-check time.
            file("E:/lib/Author/A.m4b", 55),
        ]));
        let (executor, journal, outcome) =
            walk_one(fs, "E:/lib/A.m4b", "E:/lib/Author/A.m4b").await;
        let halt = outcome.halt.expect("a present target halts");
        assert_eq!(halt.code, "target-appeared");
        let fs = executor.vfs();
        assert!(
            fs.exists(Path::new("E:/lib/A.m4b")),
            "the source is untouched"
        );
        assert_eq!(
            fs.metadata(Path::new("E:/lib/Author/A.m4b")).unwrap().size,
            55,
            "the pre-existing target is never overwritten"
        );
        assert_eq!(fs.renames.get(), 0, "no rename was attempted");
        assert_journal_consistent(&journal.entries());
    }

    /// AC-7 (adversarial never-overwrite): a concurrent writer creates a distinct
    /// file at the target AFTER the executor's absent-check passes; the executor's
    /// rename hits the real never-overwrite refusal (Vfs::rename AlreadyExists),
    /// halts with `target-appeared`, and never overwrites the victim. The journal is
    /// internally consistent.
    #[tokio::test]
    async fn ac7_concurrent_writer_creating_target_is_never_overwritten() {
        let target = Path::new("E:/lib/Author/A.m4b");
        let victim = Path::new("E:/lib/victim.m4b");
        // Source is 100 bytes; the victim (which the adversary drops at the target
        // mid-apply) is a DISTINCT 55 bytes, so the assertion can prove the target
        // still holds the victim, not our source.
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
            file("E:/lib/victim.m4b", 55),
        ]))
        .race(target, victim);

        let (executor, journal, outcome) =
            walk_one(fs, "E:/lib/A.m4b", "E:/lib/Author/A.m4b").await;

        let halt = outcome.halt.expect("the raced-in target halts");
        assert_eq!(halt.code, "target-appeared");
        let fs = executor.vfs();
        assert!(fs.raced.get(), "the adversary actually fired");
        assert!(
            fs.exists(Path::new("E:/lib/A.m4b")),
            "our source is untouched (the rename was refused)"
        );
        assert_eq!(
            fs.metadata(target).unwrap().size,
            55,
            "the target still holds the concurrent writer's file, never overwritten"
        );
        // Every intent has a terminal row despite the halt (AC-7).
        assert_journal_consistent(&journal.entries());
        assert_eq!(
            journal.entries().last().unwrap().phase,
            JournalPhase::Failed
        );
    }

    /// AC-9 (retry-once succeeds): a single access-denied on the rename is retried
    /// exactly once, the retry succeeds, and the op completes.
    #[tokio::test]
    async fn ac9_access_denied_retried_once_then_succeeds() {
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
        ]))
        .deny_renames(1);
        let (executor, _j, outcome) = walk_one(fs, "E:/lib/A.m4b", "E:/lib/Author/A.m4b").await;
        assert!(outcome.halt.is_none(), "the retry succeeds");
        assert_eq!(outcome.ops_walked, 1);
        let fs = executor.vfs();
        assert_eq!(fs.rename_calls.get(), 2, "exactly one retry (two attempts)");
        assert!(
            fs.exists(Path::new("E:/lib/Author/A.m4b")),
            "the move completed"
        );
    }

    /// AC-9 (retry-once then halt): a second access-denied halts the group with
    /// `access-denied` after exactly one retry; the source is left in place and the
    /// journal is consistent.
    #[tokio::test]
    async fn ac9_access_denied_twice_halts_after_exactly_one_retry() {
        let fs = FaultFs::new(MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 100),
        ]))
        .deny_renames(2);
        let (executor, journal, outcome) =
            walk_one(fs, "E:/lib/A.m4b", "E:/lib/Author/A.m4b").await;
        let halt = outcome.halt.expect("a second access-denied halts");
        assert_eq!(halt.code, "access-denied");
        let fs = executor.vfs();
        assert_eq!(
            fs.rename_calls.get(),
            2,
            "retries EXACTLY once - two attempts, never a third"
        );
        assert!(
            fs.exists(Path::new("E:/lib/A.m4b")),
            "the source is untouched when access stays denied"
        );
        assert_eq!(
            journal.entries().last().unwrap().phase,
            JournalPhase::Failed
        );
        assert_journal_consistent(&journal.entries());
    }

    /// P1 obligation (mkdir-first equivalence): an op whose target parent does not
    /// exist succeeds IDENTICALLY under MemFs and RealFs, because the executor
    /// creates the parent first. The RealFs leg runs entirely inside a temp dir.
    #[tokio::test]
    async fn mkdir_first_makes_a_missing_parent_succeed_on_both_backends() {
        // MemFs: seed only the source; the target parents (Author/Sub) are missing.
        let mem = MemFs::from_seed(&[dir("E:/lib"), file("E:/lib/A.m4b", 5)]);
        let (mem_exec, _mj, mem_out) =
            walk_one(mem, "E:/lib/A.m4b", "E:/lib/Author/Sub/A.m4b").await;
        assert!(
            mem_out.halt.is_none(),
            "MemFs: mkdir-first lets the move succeed"
        );
        let mem = mem_exec.vfs();
        assert!(mem.is_dir(Path::new("E:/lib/Author/Sub")));
        assert!(mem.exists(Path::new("E:/lib/Author/Sub/A.m4b")));

        // RealFs: the same shape inside a temp dir; the parents do not exist on disk.
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        let src = base.join("A.m4b");
        std::fs::write(&src, b"hello").unwrap();
        let target = base.join("Author").join("Sub").join("A.m4b");
        let (real_exec, _rj, real_out) = walk_one(
            RealFs::new(),
            &src.to_string_lossy(),
            &target.to_string_lossy(),
        )
        .await;
        assert!(
            real_out.halt.is_none(),
            "RealFs: mkdir-first lets the move succeed identically"
        );
        let real = real_exec.vfs();
        assert!(real.is_dir(&base.join("Author").join("Sub")));
        assert!(real.exists(&target));
        assert!(!real.exists(&src), "the source moved on RealFs too");
    }

    /// AC-3 (dry-run == Real journal equality, now provable with real dispatch): a
    /// dry-run over MemFs and a Real apply over RealFs, given identical inputs,
    /// produce identical journal entry sequences EXCEPT the `at` phase-timing field
    /// (documented excluded) and the RealFs/MemFs mode marker (which is NOT a
    /// journal-row field at all - it rides jobs.mode / the undo file). Both legs move
    /// the same shape; the journals match byte-for-byte after excluding `at`.
    #[tokio::test]
    async fn ac3_dry_run_and_real_produce_identical_journal_sequences() {
        // RealFs leg (a real temp dir): the "Real apply" input.
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();
        let src = base.join("src.m4b");
        std::fs::write(&src, b"hello").unwrap();
        let tgt = base.join("out").join("dest.m4b");
        let real_op = approved_op(1, 0, &src.to_string_lossy(), &tgt.to_string_lossy());
        let real_exec = Executor::new(RealFs::new(), 1, vec![real_op]);
        let real_journal = MemJournal::new();
        // Real runs at a DIFFERENT timestamp, to prove `at` is the only difference.
        real_exec
            .run(&real_journal, "2026-07-18T00:00:01Z")
            .await
            .expect("real walk");

        // MemFs leg (dry run): the identical shape seeded from the same paths.
        let mem = MemFs::from_seed(&[
            dir(&base.to_string_lossy()),
            file(&src.to_string_lossy(), 5),
        ]);
        let mem_op = approved_op(1, 0, &src.to_string_lossy(), &tgt.to_string_lossy());
        let mem_exec = Executor::new(mem, 1, vec![mem_op]);
        let mem_journal = MemJournal::new();
        mem_exec
            .run(&mem_journal, "2026-07-18T00:00:00Z")
            .await
            .expect("dry-run walk");

        // Exclude ONLY `at` (the documented phase-timing field); everything else must
        // be identical across the two modes.
        let blank_at = |rows: Vec<JournalEntry>| -> Vec<JournalEntry> {
            rows.into_iter()
                .map(|mut r| {
                    r.at = String::new();
                    r
                })
                .collect()
        };
        assert_eq!(
            blank_at(mem_journal.entries()),
            blank_at(real_journal.entries()),
            "dry-run and Real journal sequences are identical except `at` (AC-3)"
        );
        // And neither sequence carries any mode marker on the row (the marker lives
        // on jobs.mode / the undo file, not here).
        for entry in mem_journal.entries() {
            let obj = serde_json::to_value(&entry).unwrap();
            assert!(obj.get("mode").is_none() && obj.get("dry_run").is_none());
        }
    }

    /// A multi-op plan that fails MID-WALK: op 2 of 3 halts, so op 1 completed
    /// (intent + done), op 2 has intent + failed, and op 3 - after the halt - has NO
    /// rows at all. ops_walked counts only the completed op, and the halt names op 2.
    #[tokio::test]
    async fn a_mid_walk_halt_stops_and_leaves_later_ops_unwalked() {
        // Seed so op1's source exists and op3's would too, but op2's does not.
        let fs = MemFs::from_seed(&[
            dir("E:/lib"),
            file("E:/lib/A.m4b", 10),
            file("E:/lib/C.m4b", 30),
        ]);
        let op1 = approved_op(10, 0, "E:/lib/A.m4b", "E:/lib/Author/A.m4b");
        let op2 = approved_op(20, 1, "E:/lib/GONE.m4b", "E:/lib/Author/GONE.m4b");
        let op3 = approved_op(30, 2, "E:/lib/C.m4b", "E:/lib/Author/C.m4b");
        let executor = Executor::new(fs, 1, vec![op1, op2, op3]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("walk");

        let halt = outcome.halt.expect("op 2 halts the walk");
        assert_eq!(halt.op_id, 20);
        assert_eq!(halt.code, "source-vanished");
        assert_eq!(outcome.ops_walked, 1, "only op 1 completed");

        let rows = journal.entries();
        let phases = |op_id: i64| -> Vec<JournalPhase> {
            rows.iter()
                .filter(|r| r.op_id == op_id)
                .map(|r| r.phase)
                .collect()
        };
        assert_eq!(
            phases(10),
            vec![JournalPhase::Intent, JournalPhase::Done],
            "op 1 completed"
        );
        assert_eq!(
            phases(20),
            vec![JournalPhase::Intent, JournalPhase::Failed],
            "op 2 has intent + failed"
        );
        assert!(phases(30).is_empty(), "op 3, after the halt, has no rows");
        assert_journal_consistent(&rows);
        // op 3's source was never touched (the walk stopped at op 2).
        assert!(executor.vfs().exists(Path::new("E:/lib/C.m4b")));
    }

    /// The mkdir and rmdir-empty op kinds dispatch end to end. Field mapping matches
    /// the builder: a `mkdir` carries its folder in `target_path` (empty source); an
    /// `rmdir-empty` carries the folder in `source_path` (and target_path).
    #[tokio::test]
    async fn mkdir_and_rmdir_empty_dispatch_end_to_end() {
        let fs = MemFs::from_seed(&[dir("E:/lib"), dir("E:/lib/EmptyDir")]);
        let mut mkdir = approved_op(1, 0, "", "E:/lib/NewDir");
        mkdir.kind = "mkdir".to_string();
        let mut rmdir = approved_op(2, 1, "E:/lib/EmptyDir", "E:/lib/EmptyDir");
        rmdir.kind = "rmdir-empty".to_string();

        let executor = Executor::new(fs, 1, vec![mkdir, rmdir]);
        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("walk");

        assert!(outcome.halt.is_none());
        assert_eq!(outcome.ops_walked, 2);
        let fs = executor.vfs();
        assert!(
            fs.is_dir(Path::new("E:/lib/NewDir")),
            "mkdir created the folder"
        );
        assert!(
            !fs.exists(Path::new("E:/lib/EmptyDir")),
            "rmdir-empty removed the empty folder"
        );
        // Each op journaled intent + done.
        assert_eq!(journal.entries().len(), 4);
    }

    /// DISPATCH_OP_KINDS lists exactly the kinds dispatch handles, and each is a
    /// real op kind the builder emits; the pinning test that also ties this to the
    /// manifest's reversible-kind list lives in `exec::manifest`.
    #[test]
    fn dispatch_op_kinds_are_the_six_builder_kinds() {
        let mut kinds: Vec<&str> = DISPATCH_OP_KINDS.to_vec();
        kinds.sort_unstable();
        assert_eq!(
            kinds,
            [
                "mkdir",
                "move",
                "no-op",
                "quarantine",
                "rename",
                "rmdir-empty"
            ]
        );
    }

    // ---- Phase 4: FD-34 set-aside (quarantine) placement and safety ----

    /// An approved `quarantine` (set-aside) op from `source` to `target`.
    fn quarantine_op(id: i64, seq: i64, source: &str, target: &str) -> PlanOpRow {
        let mut op = approved_op(id, seq, source, target);
        op.kind = "quarantine".to_string();
        op.op_group = "dedupe-quarantine".to_string();
        op.rule_id = "parallel-format-quarantine".to_string();
        op.rationale =
            "This book keeps a preferred m4b copy, so the extra \"track01.mp3\" copy is set aside (never deleted)."
                .to_string();
        op
    }

    fn scope(library_root: &str, set_aside_root: &str) -> ApplyScope {
        ApplyScope {
            library_root: library_root.to_string(),
            set_aside_root: set_aside_root.to_string(),
        }
    }

    /// FD-34 scope guard (the brief-mandated structural check): an op whose target
    /// lands outside BOTH the library root and the set-aside root is refused BEFORE
    /// any filesystem call, as a journaled halt. Proven with a `PanicFs` that would
    /// panic the instant the seam is touched: the walk returns a clean halt without
    /// panicking, so no fs call was made.
    #[tokio::test]
    async fn scope_guard_refuses_an_out_of_scope_target_before_any_fs_call() {
        // Target escapes both roots (not under E:/lib, not under E:/Set Aside).
        let op = approved_op(1, 0, "E:/lib/Book.m4b", "E:/Elsewhere/Book.m4b");
        let executor = Executor::with_scope(PanicFs, 7, vec![op], scope("E:/lib", "E:/Set Aside"));
        let journal = MemJournal::new();
        // If the guard did NOT short-circuit before the fs, PanicFs would panic.
        let outcome = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("the scope guard halts cleanly, it does not reach the fs");
        let halt = outcome.halt.expect("an out-of-scope target halts");
        assert_eq!(halt.code, "out-of-scope-target");
        assert_eq!(outcome.ops_walked, 0);
        // Journal-consistent: the one intent has a terminal (failed) row.
        assert_journal_consistent(&journal.entries());
        assert_eq!(
            journal.entries().last().unwrap().phase,
            JournalPhase::Failed
        );
    }

    /// A set-aside target UNDER the set-aside root (outside the library) is
    /// PERMITTED by the guard (the FD-34 carve-out), and its `{job-id}` placeholder
    /// is substituted with the real job id at apply time.
    #[tokio::test]
    async fn set_aside_target_is_permitted_and_job_id_is_substituted() {
        let src = "E:/lib/Some Book/track01.mp3";
        // The builder stamps the {job-id} placeholder; the executor substitutes it.
        let op = quarantine_op(1, 0, src, "E:/Set Aside/{job-id}/Some Book/track01.mp3");
        let memfs = MemFs::from_seed(&[dir("E:/lib"), dir("E:/lib/Some Book"), file(src, 40_000)]);
        let executor = Executor::with_scope(memfs, 41, vec![op], scope("E:/lib", "E:/Set Aside"));

        // The executor's own op copy has the real job id spliced in (this is what
        // the undo manifest is built from), while plan_ops stays frozen upstream.
        assert_eq!(
            executor.ops()[0].target_path,
            "E:/Set Aside/41/Some Book/track01.mp3",
            "the {{job-id}} placeholder is replaced by the real job id"
        );

        let journal = MemJournal::new();
        let outcome = executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("the set-aside move is in scope");
        assert!(
            outcome.halt.is_none(),
            "a set-aside under the set-aside root is permitted"
        );
        assert_eq!(outcome.ops_walked, 1);

        let fs = executor.vfs();
        assert!(
            !fs.exists(Path::new(src)),
            "the item left its original location"
        );
        assert!(
            fs.exists(Path::new("E:/Set Aside/41/Some Book/track01.mp3")),
            "the item was set aside under the per-job folder (never deleted)"
        );
    }

    /// AC-3 under substitution: two apply jobs of the SAME plan substitute their
    /// own job ids into the set-aside target, yet their journal sequences are
    /// byte-identical (the job-id segment lives in the target, and a journal row
    /// carries no target).
    #[tokio::test]
    async fn different_jobs_substitute_own_job_id_but_journals_stay_identical() {
        let src = "E:/lib/B/x.mp3";
        let make = || MemFs::from_seed(&[dir("E:/lib"), dir("E:/lib/B"), file(src, 10)]);
        let target = "E:/Set Aside/{job-id}/B/x.mp3";

        let e41 = Executor::with_scope(
            make(),
            41,
            vec![quarantine_op(1, 0, src, target)],
            scope("E:/lib", "E:/Set Aside"),
        );
        let j41 = MemJournal::new();
        e41.run(&j41, "2026-07-18T00:00:00Z").await.unwrap();

        let e99 = Executor::with_scope(
            make(),
            99,
            vec![quarantine_op(1, 0, src, target)],
            scope("E:/lib", "E:/Set Aside"),
        );
        let j99 = MemJournal::new();
        e99.run(&j99, "2026-07-18T00:00:00Z").await.unwrap();

        // Different real destinations...
        assert!(e41.vfs().exists(Path::new("E:/Set Aside/41/B/x.mp3")));
        assert!(e99.vfs().exists(Path::new("E:/Set Aside/99/B/x.mp3")));
        // ...but the job's own id is the ONLY difference the rows carry (job_id
        // field); blank it and the sequences are identical (AC-3).
        let blank_job = |rows: Vec<JournalEntry>| -> Vec<JournalEntry> {
            rows.into_iter()
                .map(|mut r| {
                    r.job_id = 0;
                    r
                })
                .collect()
        };
        assert_eq!(blank_job(j41.entries()), blank_job(j99.entries()));
    }

    /// AC-22: a set-aside op's intent row records the reason and the original
    /// relative path, both recoverable from the quarantine record (the journal
    /// detail) alone. No job id lives in the detail (it is the row's job_id field),
    /// so the detail is identical across a dry run and a Real apply.
    #[tokio::test]
    async fn ac22_quarantine_intent_records_reason_and_original_relative_path() {
        let src = "E:/lib/Some Book/track01.mp3";
        let op = quarantine_op(1, 0, src, "E:/Set Aside/{job-id}/Some Book/track01.mp3");
        let memfs = MemFs::from_seed(&[dir("E:/lib"), dir("E:/lib/Some Book"), file(src, 40_000)]);
        let executor = Executor::with_scope(memfs, 7, vec![op], scope("E:/lib", "E:/Set Aside"));
        let journal = MemJournal::new();
        executor
            .run(&journal, "2026-07-18T00:00:00Z")
            .await
            .expect("walk");

        let intent = &journal.entries()[0];
        assert_eq!(intent.phase, JournalPhase::Intent);
        let detail = intent
            .detail_json
            .as_deref()
            .expect("a set-aside intent carries the AC-22 record");
        let record: serde_json::Value = serde_json::from_str(detail).expect("valid json");
        // The reason (encodes non-preferred format) and the original relative path
        // are both recoverable from the record alone.
        assert!(
            record["set_aside_reason"]
                .as_str()
                .unwrap()
                .contains("set aside"),
            "the reason is recorded: {record}"
        );
        assert_eq!(
            record["original_relative_path"].as_str().unwrap(),
            "Some Book/track01.mp3",
            "the original relative path is recoverable"
        );
        assert_eq!(record["original_path"].as_str().unwrap(), src);
        // No job id inside the detail (AC-3): it lives on the row's job_id field.
        assert!(!detail.contains("\"job_id\""));
        assert_eq!(intent.job_id, 7);
    }

    /// AC-23 (behavioral): across an apply that MOVES an audiobook, SETS ASIDE
    /// another, and removes an empty folder, NOT ONE audio file is deleted - every
    /// audio file that existed still exists (relocated), the set-aside item lives at
    /// its new location, and the only removal is the empty directory.
    #[tokio::test]
    async fn ac23_apply_deletes_no_audio_only_moves_and_empty_dir_removal() {
        let loose = "E:/lib/Loose Book.m4b";
        let loser = "E:/lib/Some Book/track01.mp3";
        let memfs = MemFs::from_seed(&[
            dir("E:/lib"),
            file(loose, 100),
            dir("E:/lib/Some Book"),
            file(loser, 40),
            dir("E:/lib/Empty"),
        ]);
        // A move (audio into its folder), a set-aside (audio, placeholder target),
        // and an empty-folder removal. The audio total must be preserved.
        let mut move_op = approved_op(1, 0, loose, "E:/lib/Loose Author/Loose Book.m4b");
        move_op.kind = "move".to_string();
        let set_aside = quarantine_op(2, 1, loser, "E:/Set Aside/{job-id}/Some Book/track01.mp3");
        let mut rmdir = approved_op(3, 2, "E:/lib/Empty", "E:/lib/Empty");
        rmdir.kind = "rmdir-empty".to_string();

        let executor = Executor::with_scope(
            memfs,
            5,
            vec![move_op, set_aside, rmdir],
            scope("E:/lib", "E:/Set Aside"),
        );
        let outcome = executor
            .run(&MemJournal::new(), "2026-07-18T00:00:00Z")
            .await
            .expect("walk");
        assert!(outcome.halt.is_none());
        assert_eq!(outcome.ops_walked, 3);

        let fs = executor.vfs();
        // The moved audiobook exists at its destination (not deleted).
        assert!(fs.exists(Path::new("E:/lib/Loose Author/Loose Book.m4b")));
        // The set-aside audio exists at the set-aside location (set aside, never
        // deleted) - this is the whole point of quarantine-only removal (D-09).
        assert!(fs.exists(Path::new("E:/Set Aside/5/Some Book/track01.mp3")));
        // Neither audio file is gone from the filesystem: both survive, relocated.
        assert!(
            !fs.exists(Path::new(loose)),
            "the loose book moved (not deleted)"
        );
        assert!(
            !fs.exists(Path::new(loser)),
            "the loser moved to set-aside (not deleted)"
        );
        // The only removal is the empty directory.
        assert!(
            !fs.exists(Path::new("E:/lib/Empty")),
            "the empty folder was swept out"
        );
    }

    /// AC-23 (source scan, in the spirit of the no-std::fs test): the executor's
    /// operation logic contains NO `remove_file` call reachable for a move/rename/
    /// set-aside - the ONLY `remove_file`s are inside `cross_volume_move` (the AC-5
    /// verified delete-source and the own-unverified-copy rollback). A same-volume
    /// set-aside is a metadata rename that never deletes, so no audio can be removed
    /// outside the one verified cross-volume path.
    #[test]
    fn ac23_remove_file_is_confined_to_the_cross_volume_delete_path() {
        const SRC: &str = include_str!("mod.rs");
        let logic = SRC.split("#[cfg(test)]").next().unwrap_or(SRC);
        // The seam CALL form, so a prose mention of remove_file in a doc comment
        // (e.g. the `retrying` doc) is not counted - only actual delete calls are.
        let needle = concat!("self.vfs.remove", "_file");

        // Extract a 4-space-indented method body by name (from `fn NAME` to the
        // next `\n    fn ` boundary, or end of logic).
        let fn_body = |name: &str| -> &str {
            let start = logic
                .find(&format!("fn {name}"))
                .unwrap_or_else(|| panic!("method {name} not found"));
            let rest = &logic[start..];
            let end = rest[1..]
                .find("\n    fn ")
                .map(|i| start + 1 + i)
                .unwrap_or(logic.len());
            &logic[start..end]
        };

        // The same-volume/quarantine dispatch path (op_move) never deletes a file:
        // it renames, or delegates to cross_volume_move.
        assert!(
            !fn_body("op_move").contains(needle),
            "op_move (the move/rename/set-aside path) must never call remove_file (AC-23)"
        );

        // Every remove_file in the operation logic lives in cross_volume_move.
        let total = logic.matches(needle).count();
        let in_cross_volume = fn_body("cross_volume_move").matches(needle).count();
        assert!(
            in_cross_volume >= 1,
            "the cross-volume path deletes the source after verify"
        );
        assert_eq!(
            total, in_cross_volume,
            "every remove_file is confined to cross_volume_move (the verified delete-source path, AC-23)"
        );
    }
}
