//! `AppError` taxonomy (thiserror; deriving `serde` and `specta::Type`).
//!
//! One enum, grouped into families as doc-sections so later phases extend it in
//! place. v0.1.0 spine, Phase 2 seeds the **Storage** family (breakdown Section
//! 8). Phase 4 adds the **Scan** family (`root-not-found`, `permission-denied`,
//! `junction-skipped`, ...) as another doc-section below, without disturbing the
//! variants defined here.
//!
//! Wire codes are the serde variant tags, kebab-case via `rename_all`, so the
//! machine code a caller keys off is exactly what serde emits (see
//! [`AppError::code`], which is contract-tested to agree with serialization).
//! Every variant carries a stable code plus a family-safe remediation sentence
//! (never a raw OS error on its own); this pairs one-to-one with the FD-04 UI
//! surfaces.

use serde::{Deserialize, Serialize};

/// The Audiobook Organizer error taxonomy.
///
/// Serialized across IPC as an externally-tagged enum whose tag is the stable
/// kebab-case machine code (e.g. `db-migration-failed`). Deriving `serde` both
/// ways and `specta::Type` makes `Result<T, AppError>` a valid tauri-specta
/// command return type once the seam is wired (Phase 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, thiserror::Error)]
#[serde(rename_all = "kebab-case")]
pub enum AppError {
    // ---- Storage family (Phase 2: database open, migration, recovery) ----
    /// The database could not be created, opened, or migrated (and recovery to a
    /// fresh database also failed). This is the hard-failure end of the startup
    /// path; the recoverable case surfaces as [`DbCorruptRecovered`].
    ///
    /// [`DbCorruptRecovered`]: AppError::DbCorruptRecovered
    #[error("database migration failed: {detail}")]
    DbMigrationFailed { detail: String },

    /// The existing database was unreadable and was reset. The corrupt file was
    /// preserved (moved aside, never deleted) at `backup_path`, and a fresh,
    /// migrated database is in use. Non-fatal: surfaced by the shell as a
    /// one-time family-safe notice, not a blocking error.
    #[error("database was corrupt and has been recovered; prior data preserved at {backup_path}")]
    DbCorruptRecovered { backup_path: String },

    /// The singleton application settings row (F-803) could not be read or
    /// written. A rare SQLite error on the settings CRUD path (`settings_get` /
    /// `settings_set`); `detail` is the developer-facing cause. Distinct from
    /// [`DbMigrationFailed`](AppError::DbMigrationFailed), which is the
    /// startup-time open/migrate failure, and from
    /// [`DbCorruptRecovered`](AppError::DbCorruptRecovered), the recovered
    /// startup case: this is a runtime read/write failure against an
    /// already-open, already-migrated database.
    #[error("settings could not be read or saved: {detail}")]
    SettingsFailed { detail: String },

    // ---- Scan family (Phase 3: scanner, file typing, snapshot persistence) ----
    //
    // Phase 3 seeds the two root-validation codes plus one internal write-path
    // code. Phase 4 formalizes the rest of the Scan taxonomy (`permission-denied`,
    // `junction-skipped`, `csv-parse`; architecture Section 7). Note: per-entry
    // permission-denied and junction events are NOT hard errors here - the scan
    // records the entry, counts it as skipped on the summary, and continues to
    // completion (AC-11); only a bad root or a failed DB write aborts a scan.
    /// The scan root does not exist. Return before any DB row is written.
    #[error("scan root not found: {path}")]
    RootNotFound { path: String },

    /// The scan root exists but is not a directory (e.g. a file was chosen).
    #[error("scan root is not a directory: {path}")]
    RootNotDirectory { path: String },

    /// A single entry could not be read because the OS denied access. Defined
    /// and ready for v0.2.0: the v0.1.0 walk records such entries and counts
    /// them in [`ScanSummary::skipped_count`](crate::ipc::ScanSummary) rather
    /// than surfacing this per-entry error (AC-11), so a locked subtree never
    /// aborts a scan. Wiring the walk to emit this per entry is a v0.2.0
    /// concern; the code, message, and remediation are pinned here so the IPC
    /// contract is stable before then. Not produced by `run_scan` yet.
    #[error("permission denied for entry: {path}")]
    PermissionDenied { path: String },

    /// A junction or reparse point was recorded but deliberately not followed
    /// (D-09), so the walk cannot loop through a link back into the tree.
    /// Defined and ready for v0.2.0 on the same footing as
    /// [`PermissionDenied`](AppError::PermissionDenied): the v0.1.0 walk counts
    /// the skip on the summary rather than emitting this per-entry error, and
    /// wiring it per entry is a v0.2.0 concern. Not produced by `run_scan` yet.
    #[error("junction/reparse point skipped (not followed): {path}")]
    JunctionSkipped { path: String },

    /// An internal failure while writing the snapshot (a SQLite/transaction
    /// error during the scans/entries write path). The walk itself never fails
    /// this way; this is the DB-side hard-failure end of `run_scan`.
    #[error("scan failed: {detail}")]
    ScanFailed { detail: String },

    /// A WizTree CSV import (F-102) could not fully parse. `row` is the 1-based
    /// index of the offending data row (the row after the header, preamble
    /// lines not counted), OR `0` reserved for a file-level/structural failure
    /// (the file could not be opened, or no WizTree header line was found at
    /// all) rather than a single bad data row.
    ///
    /// Isolated bad data rows (`row >= 1`) are recovered from: the row is
    /// skipped and the import continues with the rest of the file, so this
    /// variant surfaces as a per-row record the caller collects rather than as
    /// a hard `Err` in that case. A wholly invalid file (`row == 0`) IS a hard
    /// `Err` returned before any `scans` row is written, mirroring
    /// [`AppError::RootNotFound`]'s before-any-write posture.
    #[error("CSV row {row} could not be parsed")]
    CsvParse { row: usize },

    // ---- Ruleset family (v0.3.0 Phase 3: F-801 ruleset model) ----
    /// A ruleset's JSON body could not be accepted: it is not valid JSON, is
    /// missing a required field, has a field of the wrong type, carries a
    /// `schema_version` this build does not support, or holds an out-of-range
    /// value (for example a series-index width below 1). Returned by
    /// [`crate::ruleset::parse_and_validate`] BEFORE any row is written, so a
    /// bad body is never persisted half-valid (AC-29). `detail` is a
    /// developer-facing explanation; the remediation is the user-facing one.
    #[error("ruleset body is invalid: {detail}")]
    RulesetInvalid { detail: String },

    // ---- Ruleset editor family (v0.4.0 Phase 6: F-906 ruleset editor) ----
    /// `ruleset_get`/`ruleset_delete` named a `ruleset_id` that does not exist
    /// (never created, or already deleted).
    #[error("ruleset not found: {ruleset_id}")]
    RulesetNotFound { ruleset_id: i64 },

    /// `ruleset_delete` refused to delete `ruleset_id` because it is
    /// currently the ACTIVE ruleset (the one `plan_generate` builds against).
    /// Deleting it would leave nothing active without the caller explicitly
    /// choosing a replacement first, so this is a policy refusal, not a
    /// database error.
    #[error("ruleset {ruleset_id} is in use and cannot be deleted")]
    RulesetInUse { ruleset_id: i64 },

    /// A ruleset database operation (list/get/save/delete/activate) failed at
    /// the SQLite layer. `detail` is developer-facing; distinct from
    /// [`RulesetInvalid`](AppError::RulesetInvalid), which is a rejected body,
    /// not a database failure.
    #[error("ruleset could not be read or saved: {detail}")]
    RulesetOperationFailed { detail: String },

    // ---- Plan family (v0.3.0 Phase 5: F-404 plan validation) ----
    //
    // The machine codes below are the breakdown Section 8 "Plan" family. They
    // are the SAME strings the per-operation validation verdicts carry in
    // `plan_ops.validation_reason` (see [`crate::plan::validate::ValidationReason`],
    // whose `code()` is contract-tested to agree with these variants), so a
    // blocked op's stored reason and the AppError a caller might raise for that
    // same hazard speak one vocabulary. Validation itself does not RETURN an
    // `AppError` (its output is a per-op verdict, not a hard failure); these
    // variants exist so the codes are part of the stable IPC taxonomy and so an
    // apply-time (v0.5.0) refusal can surface the matching code.
    /// A source path recorded at plan time no longer exists at validation time
    /// (the snapshot went stale). The operation cannot run against a vanished
    /// source.
    #[error("source no longer exists (snapshot is stale): {path}")]
    SnapshotStale { path: String },

    /// Two operations in the same plan produce the same target path (compared
    /// case-insensitively for NTFS), so one would clobber the other.
    #[error("two operations target the same path within the plan: {path}")]
    CollisionInPlan { path: String },

    /// An operation's target path already exists on disk (compared
    /// case-insensitively for NTFS) and is not being vacated by the plan.
    #[error("target already exists on disk: {path}")]
    CollisionOnDisk { path: String },

    /// A target path exceeds the maximum length even with the Windows
    /// extended-length (`\\?\`) allowance. `length` is the measured character
    /// count.
    #[error("target path is too long ({length} chars): {path}")]
    PathTooLong { path: String, length: usize },

    /// A path component is not a legal filesystem name (an illegal character, or
    /// a trailing dot/space). This is the backstop to the F-304 name normalizer.
    #[error("path component is not a legal name ({component}): {path}")]
    IllegalComponent { path: String, component: String },

    /// A path component is (or begins with) a reserved Windows device name
    /// (CON, PRN, AUX, NUL, COM1-9, LPT1-9). Backstop to F-304.
    #[error("path component is a reserved device name ({component}): {path}")]
    ReservedName { path: String, component: String },

    /// The cross-volume operations targeting `volume` sum to more bytes
    /// (`needed`) than the volume has free (`available`), so the
    /// copy+verify+delete moves cannot all complete.
    #[error("not enough free space on {volume}: need {needed} bytes, {available} available")]
    CrossVolumeSpaceInsufficient {
        volume: String,
        needed: u64,
        available: u64,
    },

    /// An operation would move a source into its own subtree (target lies inside
    /// source), which is a cycle no filesystem can perform.
    #[error("operation would move a folder into itself: {source_path} -> {target_path}")]
    CycleDetected {
        source_path: String,
        target_path: String,
    },

    /// A proceed/apply was requested but no operation is in the `approved`
    /// state, so there is nothing to do. Defined for the v0.5.0 apply path;
    /// v0.3.0 only plans and validates.
    #[error("no operations are approved")]
    NothingApproved,

    // ---- Plan review family (v0.4.0 Phase 5: F-903 review surface) ----
    /// A plan generation run (build -> validate -> persist) could not complete.
    /// Wraps a database, filesystem-read, or not-found failure from
    /// [`crate::plan::report::build_and_persist_plan`] into one family-safe
    /// code; `detail` is developer-facing.
    #[error("the plan could not be built: {detail}")]
    PlanGenerationFailed { detail: String },

    /// A plan query or approval command named a `plan_id` that does not exist
    /// (never generated, or its scan/ruleset predecessor is gone).
    #[error("plan not found: {plan_id}")]
    PlanNotFound { plan_id: i64 },

    // ---- Apply family (v0.5.0 Phase 1: F-607 executor seam) ----
    /// A `Real` (non-dry-run) apply was requested, but this build implements only
    /// the dry-run walk (v0.5.0 Phase 1 Vfs seam); the executor's operation logic
    /// lands in a later phase. Returned BEFORE any filesystem work, so an
    /// intermediate build can never half-apply (D-09 safety invariant).
    #[error("real apply is not available in this build yet")]
    ApplyNotSupported,

    /// An apply job could not be recorded or closed (a SQLite error on the apply
    /// job's own bookkeeping row). This is the app-database side of starting or
    /// finishing an apply run; a filesystem failure during an actual operation is
    /// surfaced by the executor's later phases with their own codes.
    #[error("apply job could not be recorded: {detail}")]
    ApplyFailed { detail: String },

    /// The journal's `intent` row could not be flushed before the filesystem call
    /// (v0.5.0 Phase 2, journal-before-act, R-5). This is a HARD STOP: the executor
    /// does not proceed to the filesystem call if the intent flush fails (AC-13),
    /// so nothing is ever moved without a durable intent record to reconcile from.
    /// `detail` is the developer-facing SQLite cause.
    #[error("could not record what was about to happen before making a change: {detail}")]
    JournalWriteFailed { detail: String },

    // ---- Apply execution family (v0.5.0 Phase 3: F-601 executor core) ----
    //
    // These are the apply-TIME hazards the executor surfaces while it is moving
    // files: the single-writer lock, the TOCTOU re-checks, the never-overwrite
    // guard, the cross-volume verify, and the access-denied retry. Distinct from
    // the plan-family codes (which are plan-BUILD-time verdicts): `snapshot-stale`
    // is a source missing at validation time, `collision-on-disk` a target present
    // at validation time, whereas `source-vanished` / `target-appeared` are the
    // SAME hazards re-checked immediately before each real operation (AC-6).
    /// A second apply was started while one is already running. The single-writer
    /// lock (a running apply `jobs` row plus an in-process guard, AC-8) refuses the
    /// second start immediately so two apply runs never touch the library at once.
    #[error("a run is already in progress")]
    JobAlreadyRunning,

    /// A cross-volume move copied the file, then the copy's size did not match the
    /// original, so the change was stopped and the original left untouched (AC-5).
    /// The copy+verify+delete order means the source is always still there when
    /// this fires; the (unverified) copy is removed so nothing partial is left.
    #[error("a copied file did not match the original, so the change was stopped: {path}")]
    CopyVerifyMismatch { path: String },

    /// A source recorded in the plan was gone when the executor re-checked it just
    /// before acting (AC-6): the library changed under the plan. The group is
    /// halted with the journal left consistent (every started op has a terminal
    /// row). Distinct from [`SnapshotStale`](AppError::SnapshotStale), the
    /// plan-build-time counterpart.
    #[error("something to change is no longer where it was: {path}")]
    SourceVanished { path: String },

    /// Something already existed where a change would land when the executor
    /// re-checked just before acting (AC-6), or appeared mid-apply from another
    /// program (AC-7). Never-overwrite: the item is left untouched and the group is
    /// halted. Distinct from [`CollisionOnDisk`](AppError::CollisionOnDisk), the
    /// plan-build-time counterpart.
    #[error("something already exists where a change would go, so it was left alone: {path}")]
    TargetAppeared { path: String },

    /// Windows denied access to an item during an apply. The executor retried once,
    /// then stopped the current group rather than looping (AC-9). Nothing was
    /// forced; the run can be tried again once access is granted.
    #[error("Windows denied access to an item while making changes: {path}")]
    AccessDenied { path: String },

    // ---- Undo family (v0.5.0 Phase 5: F-604 rollback as an inverse plan) ----
    //
    // Preparing an undo builds, validates, and persists the INVERSE of an applied
    // tidy-up as an ordinary plan (D-09: rollback is not a special code path). These
    // are the ways preparing that inverse plan can be refused. User-facing copy says
    // "undo" (never "rollback", "undo file", or "journal"); the machine codes keep
    // the engineering term, and the copy-map (errorCopy.ts) speaks the family register.
    /// The tidy-up being undone was a REHEARSAL (a dry run), which moved nothing, so
    /// there is nothing to put back. Surfacing this (rather than panicking or failing
    /// generically) is the plain-language form of the P2 safety semantic that a
    /// dry-run manifest refuses to reverse. Also raised when a tidy-up recorded a
    /// change kind that cannot be reversed (honest rather than a false undo offer).
    #[error("this was a rehearsal, so there is nothing to undo")]
    RollbackNotReversible,

    /// A partial undo selected changes that are not a single unbroken run of the most
    /// recent ones (AC-16). An undo can only peel changes off the end in order: a gap
    /// in the middle would leave the library in a state no forward plan describes, so
    /// a non-contiguous selection is refused rather than applied.
    #[error("an undo must cover the most recent changes in one unbroken run")]
    RollbackSelectionNotContiguous,

    /// The undo could not be prepared: the undo file could not be read, the tidy-up
    /// it refers to is gone, or a change's original location could no longer be found
    /// to reverse. `detail` is the developer-facing cause. Distinct from
    /// [`RollbackNotReversible`](AppError::RollbackNotReversible) (a rehearsal or an
    /// unreversible kind) and [`RollbackSelectionNotContiguous`](AppError::RollbackSelectionNotContiguous)
    /// (a bad partial selection): this is the catch-all for a read/reconstruction failure.
    #[error("the undo could not be prepared: {detail}")]
    RollbackPrepareFailed { detail: String },

    // ---- Duplicate verification family (v0.6.0 P2: F-702 hash verification) ----
    /// Checking whether two copies really are the same book could not finish.
    /// `detail` is the developer-facing cause (a database failure, or a group
    /// that no longer exists).
    ///
    /// This is the JOB-level failure only. A single file that could not be READ
    /// is not this error: that outcome is recorded against the member and the
    /// job carries on with the rest of the group, because one unreadable file
    /// must not abandon the others (F-702, AC-12).
    #[error("the copies could not be checked: {detail}")]
    DuplicateVerifyFailed { detail: String },

    /// Writing the duplicates export into the Reports folder failed. `detail` is
    /// the developer-facing cause (usually a filesystem error: no space, or the
    /// Reports folder not writable).
    ///
    /// Its own variant rather than folding into
    /// [`DuplicateVerifyFailed`](AppError::DuplicateVerifyFailed), because the
    /// two need opposite reassurances: a failed check means nothing was decided,
    /// while a failed export means everything on screen is still correct and only
    /// the file did not get written. One message cannot say both.
    #[error("the duplicates export could not be written: {detail}")]
    DuplicateExportFailed { detail: String },

    /// A decision about a duplicate group could not be recorded or withdrawn.
    /// `detail` is the developer-facing cause (a database failure).
    ///
    /// Separate from [`DuplicateVerifyFailed`](AppError::DuplicateVerifyFailed)
    /// because the reassurance differs: a failed CHECK means nothing is known
    /// about the copies, while a failed DECISION means the copies are fine and
    /// the answer simply did not stick. Telling someone to "run the check again"
    /// after a failed write would send them to fix the wrong thing.
    #[error("the decision could not be recorded: {detail}")]
    DuplicateConfirmFailed { detail: String },

    /// A resolution was confirmed for a group whose copies have NOT been proven
    /// identical, without the explicit override (`AC-12`).
    ///
    /// THIS IS THE GATE, AND IT LIVES HERE ON PURPOSE. `AC-12` permits archiving
    /// only when every copy carries a matching hash, or when the user supplies an
    /// explicit override. A gate enforced only by the screen that happens to call
    /// this is a convention, not a mechanism: any other caller, or a later
    /// screen, would silently not have it. The refusal is the backend's, so the
    /// guarantee holds regardless of who calls.
    #[error("the copies in {group_key} have not been checked, and no override was given")]
    DuplicateNotVerified { group_key: String },

    // ---- Post-apply check family (v0.5.0 Phase 6: F-604 after-the-fact check) ----
    /// A previous tidy-up's after-the-fact check found a difference between what
    /// was planned and what is on disk, and that difference has not been
    /// acknowledged yet. Forward tidying is paused until a human acknowledges it
    /// (AC-20). This gate is FORWARD-only: preparing or running an UNDO is never
    /// refused this way, because undo is the remedy for such a difference.
    #[error("further runs are paused until the after-the-fact check is acknowledged")]
    TidyingBlocked,

    /// A previous run was cut short and the startup reconciler could not
    /// establish what it actually did, so the library's true state is unknown.
    /// Starting a fresh forward run from an unknown state is the one thing that
    /// could turn a recoverable interruption into an unrecoverable one, so the
    /// forward path is refused until that run is settled.
    ///
    /// Distinct from [`Self::TidyingBlocked`], which means a check FOUND a
    /// difference. This means no check could be completed at all. The two need
    /// different copy because they ask the reader for different things.
    ///
    /// FORWARD-only, exactly like `TidyingBlocked`: preparing or running an UNDO
    /// is never refused this way, because undo is the remedy.
    #[error("a previous run could not be checked, so further runs are paused until it is settled")]
    InterruptionUnresolved,

    // ---- Apply control family (v0.5.0 Phase 7: F-608 pause/resume) ----
    //
    // The plain refusals for the pause/resume controls (AC-24). Pausing needs a
    // tidy-up actually in progress; resuming needs one that is currently paused.
    // A Stop of a not-running tidy-up is a harmless no-op (the `job_stop` command
    // returns a boolean, like the scan Stop), so it has no error variant here.
    /// `job_pause` was asked to pause, but no tidy-up is in progress to pause
    /// (it already finished, was never started, or the id is unknown).
    #[error("there is nothing in progress to pause")]
    NothingToPause,

    /// `job_resume` was asked to resume, but the tidy-up is not paused (it is
    /// running normally, already finished, or the id is unknown), so there is
    /// nothing to resume.
    #[error("there is no paused run to resume")]
    NothingToResume,

    // ---- Interruption safety family (v0.6.0 Phase 1: F-606 reconcile) ----
    /// The startup reconciliation pass (F-606) could not read the journal to
    /// check whether the last tidy-up was interrupted mid-change (a SQLite error
    /// on the journal read). `detail` is the developer-facing cause. Distinct
    /// from [`JournalWriteFailed`](AppError::JournalWriteFailed), the write-side
    /// journal-before-act hard stop: this is a read failure while recovering from
    /// an interruption at startup.
    #[error("could not check whether the last run was interrupted: {detail}")]
    ReconcileFailed { detail: String },

    // ---- History family (v0.6.0: the record of past tidy-ups) ----
    /// The History screen's read of past tidy-ups failed (a SQLite error reading
    /// `jobs`, `journal`, or `manifests`). `detail` is the developer-facing cause.
    /// A read-only failure: it means the record could not be SHOWN, never that a
    /// past tidy-up or its undo file was lost - both live outside this read, and
    /// the undo file is self-contained by design (AC-11).
    #[error("could not read the record of past runs: {detail}")]
    HistoryUnavailable { detail: String },

    // ---- Open-a-folder family (F-610, v0.6.0 P10) ----
    /// The path handed to the open-a-folder command could not be PROVEN to sit
    /// inside the library root or the Archive root, so it was refused (`AC-48`).
    ///
    /// One code covers two causes on purpose, because the gate gives one answer.
    /// A path outside the sanctioned roots and a path that no longer resolves are
    /// the same result from the gate's point of view: it cannot show that opening
    /// this is allowed, so it does not. Failing closed is the point. Without this
    /// refusal the command is a general "open any path on this machine"
    /// primitive reachable from the web layer, which is exactly what `FD-29`'s
    /// minimal-capability posture exists to prevent.
    #[error("refused to open a path outside the library or the Archive: {path}")]
    RevealRefused { path: String },

    /// The path was allowed, and the OS file manager could not be started.
    /// Distinct from [`RevealRefused`](AppError::RevealRefused) because the two
    /// mean opposite things to a reader: one is the app declining, the other is
    /// the app trying and the machine not cooperating.
    #[error("could not open the file manager: {detail}")]
    RevealFailed { detail: String },
}

impl AppError {
    /// Stable machine-readable code. These strings are part of the IPC contract
    /// and must not change; they equal the serde variant tag (contract-tested).
    pub fn code(&self) -> &'static str {
        match self {
            // Storage family
            AppError::DbMigrationFailed { .. } => "db-migration-failed",
            AppError::DbCorruptRecovered { .. } => "db-corrupt-recovered",
            AppError::SettingsFailed { .. } => "settings-failed",
            // Scan family
            AppError::RootNotFound { .. } => "root-not-found",
            AppError::RootNotDirectory { .. } => "root-not-directory",
            AppError::PermissionDenied { .. } => "permission-denied",
            AppError::JunctionSkipped { .. } => "junction-skipped",
            AppError::ScanFailed { .. } => "scan-failed",
            AppError::CsvParse { .. } => "csv-parse",
            // Ruleset family
            AppError::RulesetInvalid { .. } => "ruleset-invalid",
            // Ruleset editor family
            AppError::RulesetNotFound { .. } => "ruleset-not-found",
            AppError::RulesetInUse { .. } => "ruleset-in-use",
            AppError::RulesetOperationFailed { .. } => "ruleset-operation-failed",
            // Plan family
            AppError::SnapshotStale { .. } => "snapshot-stale",
            AppError::CollisionInPlan { .. } => "collision-in-plan",
            AppError::CollisionOnDisk { .. } => "collision-on-disk",
            AppError::PathTooLong { .. } => "path-too-long",
            AppError::IllegalComponent { .. } => "illegal-component",
            AppError::ReservedName { .. } => "reserved-name",
            AppError::CrossVolumeSpaceInsufficient { .. } => "cross-volume-space-insufficient",
            AppError::CycleDetected { .. } => "cycle-detected",
            AppError::NothingApproved => "nothing-approved",
            // Plan review family
            AppError::PlanGenerationFailed { .. } => "plan-generation-failed",
            AppError::PlanNotFound { .. } => "plan-not-found",
            // Apply family
            AppError::ApplyNotSupported => "apply-not-supported",
            AppError::ApplyFailed { .. } => "apply-failed",
            AppError::JournalWriteFailed { .. } => "journal-write-failed",
            // Apply execution family
            AppError::JobAlreadyRunning => "job-already-running",
            AppError::CopyVerifyMismatch { .. } => "copy-verify-mismatch",
            AppError::SourceVanished { .. } => "source-vanished",
            AppError::TargetAppeared { .. } => "target-appeared",
            AppError::AccessDenied { .. } => "access-denied",
            // Undo family
            AppError::RollbackNotReversible => "rollback-not-reversible",
            AppError::RollbackSelectionNotContiguous => "rollback-selection-not-contiguous",
            AppError::RollbackPrepareFailed { .. } => "rollback-prepare-failed",
            AppError::DuplicateVerifyFailed { .. } => "duplicate-verify-failed",
            AppError::DuplicateExportFailed { .. } => "duplicate-export-failed",
            AppError::DuplicateConfirmFailed { .. } => "duplicate-confirm-failed",
            AppError::DuplicateNotVerified { .. } => "duplicate-not-verified",
            // Post-apply check family
            AppError::TidyingBlocked => "tidying-blocked",
            AppError::InterruptionUnresolved => "interruption-unresolved",
            // Apply control family
            AppError::NothingToPause => "nothing-to-pause",
            AppError::NothingToResume => "nothing-to-resume",
            // Interruption safety family
            AppError::ReconcileFailed { .. } => "reconcile-failed",
            // History family
            AppError::HistoryUnavailable { .. } => "history-unavailable",
            AppError::RevealRefused { .. } => "reveal-refused",
            AppError::RevealFailed { .. } => "reveal-failed",
        }
    }

    /// Family-safe, actionable guidance shown alongside the message. Never empty.
    pub fn remediation(&self) -> &'static str {
        match self {
            // Storage family
            AppError::DbMigrationFailed { .. } => {
                "The app's database could not be prepared. Restart the app. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
            }
            AppError::DbCorruptRecovered { .. } => {
                "The app's database was unreadable and has been reset so the app can run. Your \
                 previous data was preserved as a backup in the corrupt-backups folder and can \
                 be recovered manually if needed."
            }
            AppError::SettingsFailed { .. } => {
                "Your settings could not be saved. Restart the app and try again. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
            }
            // Scan family
            AppError::RootNotFound { .. } => {
                "The folder to scan could not be found. Check that the drive is connected and \
                 the folder still exists, then choose it again."
            }
            AppError::RootNotDirectory { .. } => {
                "The item chosen to scan is a file, not a folder. Choose a folder to scan."
            }
            AppError::PermissionDenied { .. } => {
                "This item could not be read because Windows denied access. It was skipped and \
                 the rest of the scan continued. If you need it included, run the app as a user \
                 who can read it, or adjust the folder's permissions, then scan again."
            }
            AppError::JunctionSkipped { .. } => {
                "This item is a junction or reparse point (a link to another location). It was \
                 recorded but not opened, so the scan cannot loop back on itself. This is \
                 expected behavior and needs no action."
            }
            AppError::ScanFailed { .. } => {
                "The scan could not be saved. Restart the app and try again. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
            }
            AppError::CsvParse { .. } => {
                "One or more rows in the WizTree CSV file could not be read. Rows that could be \
                 read were imported; check that the file is an unmodified WizTree export and try \
                 again if entries are missing."
            }
            // Ruleset family
            AppError::RulesetInvalid { .. } => {
                "This ruleset could not be saved because its settings were not valid. This is \
                 usually an out-of-date or hand-edited ruleset file; reset it to the defaults or \
                 re-create it, then save again."
            }
            // Ruleset editor family
            AppError::RulesetNotFound { .. } => {
                "This ruleset could not be found; it may already have been deleted. Choose \
                 another ruleset, or create a new one."
            }
            AppError::RulesetInUse { .. } => {
                "This is the ruleset you're using right now, so it can't be deleted. Choose a \
                 different one first, then delete this one."
            }
            AppError::RulesetOperationFailed { .. } => {
                "Your library-organizing settings could not be read or saved. Restart the app and \
                 try again. If this keeps happening, the disk may be full or the app data folder \
                 may be on a synced location (OneDrive); free space or move the app data out of \
                 the synced folder."
            }
            // Plan family
            AppError::SnapshotStale { .. } => {
                "A file or folder in the plan has moved or been deleted since the library was \
                 last scanned. Scan again to refresh the plan, then review it."
            }
            AppError::CollisionInPlan { .. } => {
                "Two changes in this plan would end up with the same name and location, so one \
                 would overwrite the other. Adjust the naming rules or exclude one of them, then \
                 review the plan again."
            }
            AppError::CollisionOnDisk { .. } => {
                "A change would land on a file or folder that already exists. Rename or move the \
                 existing item, or exclude this change, then review the plan again."
            }
            AppError::PathTooLong { .. } => {
                "The new location's full path is too long for Windows to store. Choose a shorter \
                 library location or shorter naming rules, then review the plan again."
            }
            AppError::IllegalComponent { .. } => {
                "The new name contains a character Windows does not allow in file names, or ends \
                 in a dot or space. This should have been cleaned up automatically; re-create the \
                 plan, and if it recurs report it."
            }
            AppError::ReservedName { .. } => {
                "The new name matches a name Windows reserves for hardware devices (such as CON \
                 or COM1). This should have been cleaned up automatically; re-create the plan, \
                 and if it recurs report it."
            }
            AppError::CrossVolumeSpaceInsufficient { .. } => {
                "Some changes move files to a different drive, which copies them, and that drive \
                 does not have enough free space for all of them. Free space on the target drive, \
                 or exclude some of those changes, then review the plan again."
            }
            AppError::CycleDetected { .. } => {
                "A change would move a folder inside itself, which is not possible. Adjust the \
                 naming rules or exclude that change, then review the plan again."
            }
            AppError::NothingApproved => {
                "No changes have been approved yet, so there is nothing to do. Approve at least \
                 one group or operation first."
            }
            // Plan review family
            AppError::PlanGenerationFailed { .. } => {
                "The plan could not be built. Scan the library again and try building \
                 the plan once more. If this keeps happening, the disk may be full or the app \
                 data folder may be on a synced location (OneDrive); free space or move the app \
                 data out of the synced folder."
            }
            AppError::PlanNotFound { .. } => {
                "This plan could not be found; it may have been built in an earlier \
                 session. Build a new plan from the current scan and review that instead."
            }
            // Apply family
            AppError::ApplyNotSupported => {
                "Making changes for real is not available in this version yet. You can preview \
                 what organizing would change; making changes for real arrives in a later version."
            }
            AppError::ApplyFailed { .. } => {
                "The app could not record this run. Try again. If this keeps happening, \
                 the disk may be full or the app data folder may be on a synced location \
                 (OneDrive); free space or move the app data out of the synced folder."
            }
            AppError::JournalWriteFailed { .. } => {
                "The app stopped before making any change because it could not first record what \
                 it was about to do. Nothing was moved. Try again. If this keeps happening, the \
                 disk may be full or the app data folder may be on a synced location (OneDrive); \
                 free space or move the app data out of the synced folder."
            }
            // Apply execution family
            AppError::JobAlreadyRunning => {
                "Organizing is already in progress. Wait for it to finish, then start the next \
                 one. Only one run happens at a time so your library is never changed twice at \
                 once."
            }
            AppError::CopyVerifyMismatch { .. } => {
                "A file had to be copied to another drive, but the copy did not match the \
                 original, so the change was stopped and your original file was left exactly where \
                 it was. Check the target drive for errors, then try again."
            }
            AppError::SourceVanished { .. } => {
                "A file or folder this run was going to change is no longer where it was, so \
                 the run stopped safely. Scan your library again to refresh it, then review \
                 and organize once more."
            }
            AppError::TargetAppeared { .. } => {
                "Something already exists where a change was going to land, so it was left alone \
                 and the run stopped without overwriting anything. Scan your library again to \
                 refresh it, then review and organize once more."
            }
            AppError::AccessDenied { .. } => {
                "Windows would not let the app change an item, even after a second try, so the \
                 run stopped there. Close any program that may be using that file or folder, \
                 or grant the app permission to it, then try again."
            }
            // Undo family
            AppError::RollbackNotReversible => {
                "That was a rehearsal, so nothing was actually moved and there is nothing \
                 to undo. Do a real run first if you want changes you can undo."
            }
            AppError::RollbackSelectionNotContiguous => {
                "An undo can only take back the most recent changes, in order. Choose an unbroken \
                 run of the latest changes to undo, without skipping any in the middle."
            }
            AppError::RollbackPrepareFailed { .. } => {
                "The undo could not be prepared. The undo file may be missing or the run it \
                 refers to may be gone. Scan your library again, then build and review a fresh \
                 plan instead."
            }
            AppError::DuplicateVerifyFailed { .. } => {
                "The check on your duplicate copies could not finish, so nothing was decided \
                 about them. Your books are untouched. You can run the check again."
            }
            AppError::DuplicateExportFailed { .. } => {
                "The list of duplicate copies could not be saved to a file. What you see on \
                 screen is still correct and your books are untouched. Check there is room on \
                 the drive, then try saving it again."
            }
            AppError::DuplicateConfirmFailed { .. } => {
                "Your choice about these copies could not be saved. The copies themselves are \
                 fine and nothing has moved. Make the choice again."
            }
            AppError::DuplicateNotVerified { .. } => {
                "These copies have not been checked yet, so the app cannot tell whether they \
                 really are the same book. Check them first, or say plainly that you want to \
                 archive them without checking."
            }
            // Post-apply check family
            AppError::TidyingBlocked => {
                "The last run's after-the-fact check found a difference that needs a look \
                 before more changes are made. Review the after-the-fact check and acknowledge it; \
                 undoing the last run is still available. Once acknowledged, runs resume."
            }
            AppError::InterruptionUnresolved => {
                "The last run stopped early and the app could not work out what it had already \
                 done, so it will not start another one from a library it cannot read. Open \
                 History and settle that run first; putting it back is still available. Nothing \
                 has been changed by this refusal."
            }
            // Apply control family
            AppError::NothingToPause => {
                "There is nothing in progress to pause. Start organizing first; you can pause it \
                 between books while it runs."
            }
            AppError::NothingToResume => {
                "This run is not paused, so there is nothing to resume. If a run is \
                 paused, use Resume to continue it between books."
            }
            // Interruption safety family
            AppError::ReconcileFailed { .. } => {
                "The app could not check whether the last run was interrupted. Restart the \
                 app and try again. If this keeps happening, the disk may be full or the app \
                 data folder may be on a synced location (OneDrive); free space or move the app \
                 data out of the synced folder."
            }
            // History family
            AppError::HistoryUnavailable { .. } => {
                "The app could not read the record of your past runs. Your books and your                  undo files are untouched - only the app's own notes could not be read. Restart                  the app and try again."
            }
            AppError::RevealRefused { .. } => {
                "This app only opens folders inside your library or your Archive, and it                  could not confirm that this one is. It may also have been moved or renamed                  since the last scan. Scan again, then try opening it from the fresh result."
            }
            AppError::RevealFailed { .. } => {
                "Windows did not open a folder window. Nothing in your library was changed.                  Try again, and if it keeps happening you can still reach the folder yourself                  in File Explorer."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One instance of every variant. Keep in sync with the enum; the code and
    /// remediation coverage tests iterate this list.
    fn one_of_each() -> Vec<AppError> {
        vec![
            // An adversarial review found DuplicateVerifyFailed missing from this
            // list, which silently narrowed EVERY test that iterates it, including
            // the remediation vocabulary sweep. A helper named "one of each" that
            // is not one of each is worse than no helper: callers trust the name.
            AppError::DuplicateVerifyFailed {
                detail: "boom".into(),
            },
            AppError::DuplicateExportFailed {
                detail: "boom".into(),
            },
            AppError::DuplicateConfirmFailed {
                detail: "boom".into(),
            },
            AppError::DuplicateNotVerified {
                group_key: "Dune.m4b|900".into(),
            },
            AppError::HistoryUnavailable {
                detail: "boom".into(),
            },
            AppError::DbMigrationFailed {
                detail: "boom".into(),
            },
            AppError::DbCorruptRecovered {
                backup_path: "C:/x/corrupt-backups/abo-1.db".into(),
            },
            AppError::SettingsFailed {
                detail: "database is locked".into(),
            },
            AppError::RootNotFound {
                path: r"C:\Users\x\missing".into(),
            },
            AppError::RootNotDirectory {
                path: r"C:\Users\x\a-file.txt".into(),
            },
            AppError::PermissionDenied {
                path: r"C:\Users\x\locked\secret.m4b".into(),
            },
            AppError::JunctionSkipped {
                path: r"C:\Users\x\library\link-to-elsewhere".into(),
            },
            AppError::ScanFailed {
                detail: "database is locked".into(),
            },
            AppError::CsvParse { row: 42 },
            AppError::RulesetInvalid {
                detail: "missing field `naming`".into(),
            },
            AppError::RulesetNotFound { ruleset_id: 7 },
            AppError::RulesetInUse { ruleset_id: 7 },
            AppError::RulesetOperationFailed {
                detail: "database is locked".into(),
            },
            AppError::SnapshotStale {
                path: r"E:\Books\Gone.m4b".into(),
            },
            AppError::CollisionInPlan {
                path: r"E:\Books\Author\Title".into(),
            },
            AppError::CollisionOnDisk {
                path: r"E:\Books\Author\Title".into(),
            },
            AppError::PathTooLong {
                path: r"E:\Books\very\long\path".into(),
                length: 33_000,
            },
            AppError::IllegalComponent {
                path: r"E:\Books\bad?name".into(),
                component: "bad?name".into(),
            },
            AppError::ReservedName {
                path: r"E:\Books\CON".into(),
                component: "CON".into(),
            },
            AppError::CrossVolumeSpaceInsufficient {
                volume: "D:".into(),
                needed: 2_000,
                available: 1_000,
            },
            AppError::CycleDetected {
                source_path: r"E:\Books\A".into(),
                target_path: r"E:\Books\A\B".into(),
            },
            AppError::NothingApproved,
            AppError::PlanGenerationFailed {
                detail: "database is locked".into(),
            },
            AppError::PlanNotFound { plan_id: 42 },
            AppError::ApplyNotSupported,
            AppError::ApplyFailed {
                detail: "database is locked".into(),
            },
            AppError::JournalWriteFailed {
                detail: "database is locked".into(),
            },
            AppError::JobAlreadyRunning,
            AppError::CopyVerifyMismatch {
                path: r"F:\Books\Author\Title\book.m4b".into(),
            },
            AppError::SourceVanished {
                path: r"E:\Books\Gone.m4b".into(),
            },
            AppError::TargetAppeared {
                path: r"E:\Books\Author\Title".into(),
            },
            AppError::AccessDenied {
                path: r"E:\Books\locked\book.m4b".into(),
            },
            AppError::RollbackNotReversible,
            AppError::RollbackSelectionNotContiguous,
            AppError::RollbackPrepareFailed {
                detail: "undo file could not be read".into(),
            },
            AppError::TidyingBlocked,
            AppError::NothingToPause,
            AppError::NothingToResume,
            AppError::ReconcileFailed {
                detail: "database is locked".into(),
            },
            // Added 2026-08-21. It had been missing since the variant was
            // introduced, which is the SECOND time this list has silently
            // narrowed every test that iterates it. See
            // `one_of_each_really_is_one_of_each` below, which now makes a third
            // time impossible.
            AppError::InterruptionUnresolved,
            // F-610 (P10). Added because `one_of_each_really_is_one_of_each`
            // refused the build until they were, which is the mechanism working
            // on its first genuinely new variants rather than on a rehearsal.
            AppError::RevealRefused {
                path: "E:/Elsewhere".into(),
            },
            AppError::RevealFailed {
                detail: "no such binary".into(),
            },
        ]
    }

    /// Every `AppError` variant appears in [`one_of_each`], proved from the
    /// source rather than from memory.
    ///
    /// # Why this test exists rather than another comment asking for care
    ///
    /// `one_of_each` is a hand-maintained list, and five tests iterate it: the
    /// code-shape check, the serde tag lock, the serde round trip, the
    /// non-empty-remediation check, and the retired-vocabulary sweep over text a
    /// user reads. A variant missing from the list is therefore not "one test
    /// slightly weaker", it is five guarantees quietly not applying to that
    /// variant, with every test still green.
    ///
    /// This has now happened twice. An adversarial review caught
    /// `DuplicateVerifyFailed` missing; the response was to add it and write a
    /// comment saying to keep the list in sync. `InterruptionUnresolved` was
    /// missing anyway, found on 2026-08-21. **The fix was the instance, not the
    /// class**, and a comment is not a mechanism: it asks the next author to
    /// remember something the compiler is perfectly capable of noticing.
    ///
    /// So this reads the file's own source. `code()` is an exhaustive match with
    /// no wildcard arm, which means the COMPILER already forces it to name every
    /// variant: adding one without touching `code()` will not build. That makes
    /// `code()` a trustworthy census of the enum, and comparing it against
    /// `one_of_each` closes the gap for good. A source-reading test is an unusual
    /// shape and is justified here by there being no reflection over enum
    /// variants in Rust, and by the alternative having already failed twice.
    #[test]
    fn one_of_each_really_is_one_of_each() {
        /// Variant names mentioned inside `src`, between the first occurrence of
        /// `start` and the block terminator that follows it.
        fn variants_in(src: &str, start: &str) -> std::collections::BTreeSet<String> {
            let from = src.find(start).expect("anchor present in source");
            let body = &src[from..];
            let to = body
                .find(
                    "
    }",
                )
                .expect("block terminator present");
            let mut out = std::collections::BTreeSet::new();
            let mut rest = &body[..to];
            while let Some(i) = rest.find("AppError::") {
                rest = &rest[i + "AppError::".len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    out.insert(name);
                }
            }
            out
        }

        let src = include_str!("error.rs");
        let declared = variants_in(src, "pub fn code(&self) -> &'static str {");
        let listed = variants_in(src, "fn one_of_each() -> Vec<AppError> {");

        assert!(
            declared.len() > 40,
            "sanity: the parse found only {} variants in code(), so the anchors moved              and this test is measuring nothing",
            declared.len()
        );

        let missing: Vec<&String> = declared.difference(&listed).collect();
        assert!(
            missing.is_empty(),
            "one_of_each() is missing {missing:?}. Five tests iterate that list, so each              missing variant is five guarantees silently not applying to it: the code shape              check, the serde tag lock, the serde round trip, non-empty remediation, and the              retired-vocabulary sweep over text a user reads. Add it to one_of_each()."
        );

        let unknown: Vec<&String> = listed.difference(&declared).collect();
        assert!(
            unknown.is_empty(),
            "one_of_each() names {unknown:?}, which code() does not. Either the variant was              removed and this list was not, or the anchors this test parses have moved."
        );
    }

    #[test]
    fn every_code_is_stable_kebab_and_unique() {
        let codes: Vec<&str> = one_of_each().iter().map(AppError::code).collect();
        for c in &codes {
            assert!(!c.is_empty(), "code must be non-empty");
            assert!(
                c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "code must be kebab-case: {c}"
            );
        }
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "all codes must be unique");
    }

    /// Remediation copy is USER-FACING: it crosses IPC and renders in the app's
    /// error surfaces. It passed through NEITHER vocabulary gate - the TypeScript
    /// sweep covers `strings.ts` and `errorCopy.ts`, the Rust report gate covers
    /// only the generated HTML - which is how "Your shelf-organizing settings..."
    /// survived FD-47's retirement of "shelf" and was found by an adversarial
    /// review rather than by a test.
    ///
    /// The general lesson, and the reason this test exists here rather than being
    /// folded into one of the others: the gates were keyed on WHICH FILE text lives
    /// in, when the property that matters is whether the text REACHES A USER.
    #[test]
    fn no_remediation_carries_retired_vocabulary() {
        for err in one_of_each() {
            // "Audiobookshelf" is the product this app complements and may legitimately
            // appear. Neutralize it rather than weakening the match, so the exception
            // stays visible (same treatment as the report gate).
            let text = err
                .remediation()
                .to_lowercase()
                .replace("audiobookshelf", "<the-product>");
            for (word, decision, successor) in [
                // "aside" bare, not "set aside": the adjacent-only pattern missed
                // "Set it aside" and "sets the copy aside", both of which were live.
                // After FD-42 no user-facing sentence needs the word at all.
                ("aside", "FD-42", "Archive"),
                ("shelf", "FD-47", "library"),
                ("shelves", "FD-47", "library"),
                // The verb form the noun-only list missed: "shelved" contains
                // neither "shelf" nor "shelves", so it was unswept here.
                ("shelved", "FD-47", "library"),
                // FD-48 retired the whole family for "organize". A substring
                // match is right here: it covers tidy, tidying, tidied and
                // tidy-up in one, and no other English word contains "tidy".
                ("tidy", "FD-48", "organize"),
            ] {
                assert!(
                    !text.contains(word),
                    "{}: remediation carries {word:?}, retired by {decision} in favour of \
                     {successor:?}. This copy is read by a user.",
                    err.code()
                );
            }
        }
    }

    #[test]
    fn every_variant_has_non_empty_remediation() {
        for err in one_of_each() {
            assert!(
                !err.remediation().is_empty(),
                "remediation must be non-empty for {}",
                err.code()
            );
        }
    }

    #[test]
    fn code_matches_serde_tag() {
        // The machine code is defined "via serde rename": the code() string must
        // equal the serde variant tag. This locks the two together so a future
        // rename cannot drift the contract silently. Struct variants serialize
        // externally-tagged as a one-key object; a UNIT variant (e.g.
        // `NothingApproved`) serializes as the bare tag STRING - both forms are
        // valid, and in both the tag is exactly the machine code.
        for err in one_of_each() {
            let value = serde_json::to_value(&err).expect("serialize");
            let tag = match &value {
                serde_json::Value::Object(o) => {
                    o.keys().next().cloned().expect("one-key tagged object")
                }
                serde_json::Value::String(s) => s.clone(),
                other => panic!("unexpected AppError serialization: {other:?}"),
            };
            assert_eq!(tag, err.code(), "serde tag must equal code()");
        }
    }

    #[test]
    fn round_trips_through_serde() {
        for err in one_of_each() {
            let json = serde_json::to_string(&err).expect("serialize");
            let back: AppError = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, err, "AppError must round-trip through serde");
        }
    }
}
