//! F-607 / F-601 / F-904 apply command handler - v0.5.0 (acting) Phase 7 adds the
//! cooperative pause/resume/Stop controls and makes the apply a SPAWNED background
//! job (F-608, F-104, FD-02).
//!
//! [`apply_start`] is the executor's IPC entry point. It loads the approved plan,
//! acquires the single-writer lock (AC-8), records the apply `jobs` row, registers
//! an [`ApplyControl`], then SPAWNS the walk on the async runtime and returns the
//! new `jobs.id` straight away (a [`JobStarted`], like `scan_start`) - so the caller
//! learns the job id WHILE the apply runs, which is what makes `job_pause(job_id)`
//! and Stop usable mid-walk. The spawned walk runs the plan's APPROVED operations
//! through the [`Executor`](abo_core::exec::Executor):
//! - [`ApplyMode::DryRun`] walks a [`MemFs`](abo_core::exec::MemFs) seeded from the
//!   plan's snapshot, touching no real path (AC-2);
//! - [`ApplyMode::Real`] walks [`RealFs`](abo_core::exec::RealFs), the actual disk.
//!
//! The executor consults the [`ApplyControl`] at operation BOUNDARIES only (F-608
//! pause parks the walk there; F-104 Stop ends it there), never mid-op, and neither
//! writes a journal row of its own (AC-24, AC-25). A cooperative Stop ends the walk
//! in the DISTINCT `stopped` terminal state (not `failed`), with no undo file for
//! the partial forward job (AC-26).
//!
//! Thin-adapter rule (same as [`super::plan`]): the product logic lives in
//! `abo-core`; this command orchestrates lock -> job row -> control -> executor ->
//! undo file, and maps an [`ExecHalt`](abo_core::exec::ExecHalt) onto the typed
//! error surface. The per-job outcome (verified ops, discrepancy block) is read back
//! via [`job_status`](super::job::job_status).
//!
//! Single-writer (AC-8): a fast in-process guard (`AppState::apply_in_flight`)
//! refuses a second concurrent apply in this process instantly; the durable
//! `running` apply `jobs` row (via [`acquire_apply_job`]) is the cross-restart
//! backstop. Both are released - on completion, failure, a Stop, OR a panic in the
//! walk - inside the SPAWNED task (via [`run_apply_to_terminal`] and its RAII
//! guards), plus the startup reclaim for a crash. A paused apply is still THE apply:
//! it keeps both, so a second apply is refused while one is paused.

use std::collections::HashMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use abo_core::db::plans::{get_plan, get_plan_ops};
use abo_core::exec::lock::acquire_apply_job;
use abo_core::exec::manifest::export_after_apply;
use abo_core::exec::verify::{affected_roots, write_check_report};
use abo_core::exec::{
    delta_health_metrics, ensure_forward_tidying_allowed, record_block, verify_job, ApplyMode,
    ApplyScope, CheckReport, ExecControl, ExecHalt, Executor, MemFs, RealFs, SeedEntry,
    SqliteJournal, Vfs,
};
use abo_core::ipc::{AppError, ApplyReport, EntryRow, JobStarted};
use abo_core::plan::builder::default_set_aside_root;
use abo_core::scan::walk::now_iso8601_utc;
use futures::FutureExt;
use sqlx::SqlitePool;
use tokio::sync::Notify;

use crate::AppState;

/// Cooperative pause/resume/Stop state for one running apply job (F-608 pause,
/// F-104 Stop, FD-02). Held in [`AppState::apply_controls`](crate::AppState) keyed
/// by `jobs.id` while the apply runs on its spawned task, so the `job_pause` /
/// `job_resume` / `job_stop` commands can reach a job that is mid-walk.
///
/// It implements [`ExecControl`] so the executor consults it at operation
/// BOUNDARIES only: `stop_requested` ends the walk at the next boundary, and
/// `pause_barrier` parks the walk there until it is resumed or stopped. Pause and
/// Stop are metadata-only in-memory state - NEVER a journal event (FD-02, AC-25).
///
/// A paused apply is STILL the apply: it keeps holding the single-writer lock (the
/// `running` `jobs` row and the in-process flag are untouched by a pause), so a
/// second apply is still refused while one is paused (AC-8).
pub struct ApplyControl {
    /// Set once a Stop is requested; the walk ends at the next operation boundary.
    cancel: AtomicBool,
    /// Set while paused; the walk parks at the next boundary until resumed/stopped.
    paused: AtomicBool,
    /// Wakes a parked walk on resume or Stop. `notify_one` buffers a permit if the
    /// walk has not parked yet, so a resume/Stop that races ahead of the park is
    /// never lost (the walk is the single waiter).
    wake: Notify,
}

impl ApplyControl {
    /// A fresh, not-paused, not-stopped control.
    pub fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            wake: Notify::new(),
        }
    }

    /// Request a pause; it takes effect at the next operation boundary.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    /// Resume a paused apply, waking the parked walk.
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Whether a pause is currently in effect.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Whether a cooperative Stop has been requested (the executor observes this at
    /// its next operation boundary via [`ExecControl::stop_requested`]).
    pub fn is_stopping(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Request a cooperative Stop, waking a parked walk so it observes the Stop and
    /// ends at the next boundary.
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.wake.notify_one();
    }
}

impl Default for ApplyControl {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecControl for ApplyControl {
    fn stop_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    async fn pause_barrier(&self) {
        // Park while paused. A Stop wakes and unblocks this too (the `!cancel`
        // guard), so the executor's post-park Stop re-check ends the walk cleanly.
        while self.paused.load(Ordering::SeqCst) && !self.cancel.load(Ordering::SeqCst) {
            self.wake.notified().await;
        }
    }
}

/// The registry of live apply controls, keyed by `jobs.id`. `apply_start` inserts
/// one when it spawns the walk and removes it when the job reaches a terminal
/// state; the pause/resume/stop commands look one up to control a running apply.
/// A plain `std::sync::Mutex`: every access is a brief, non-async insert/remove/
/// lookup, never held across an `.await`.
pub type ApplyControlRegistry = Arc<Mutex<HashMap<i64, Arc<ApplyControl>>>>;

/// RAII guard for the in-process apply flag: clears `apply_in_flight` on every
/// exit path (success, error, or a panic that unwinds), so a refused or crashed
/// apply never leaves the fast guard stuck set.
///
/// It is used in two places for two disjoint windows, and MUST fire in exactly one:
/// - a `preflight` guard in `apply_start` covers the pre-spawn window, so a `?`
///   early-return there clears the flag; on the happy path it is [`disarm`]ed once
///   the spawned task owns the flag, so it does NOT clear it;
/// - the spawned task's guard covers the walk, clearing the flag when the walk ends
///   (any path, including a panic-unwind), which is when `apply_start` has long
///   since returned.
///
/// [`disarm`]: ApplyInFlightGuard::disarm
struct ApplyInFlightGuard {
    flag: Arc<AtomicBool>,
    armed: bool,
}

impl ApplyInFlightGuard {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag, armed: true }
    }
    /// Stop this guard from clearing the flag on drop (the flag's release has been
    /// handed to another guard - the spawned task's).
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ApplyInFlightGuard {
    fn drop(&mut self) {
        if self.armed {
            self.flag.store(false, Ordering::SeqCst);
        }
    }
}

/// RAII guard that deregisters a job's [`ApplyControl`] from the registry when the
/// spawned apply task ends (any path, including a panic-unwind), so the registry
/// never leaks a control for a job that is no longer running.
struct ApplyControlGuard {
    registry: ApplyControlRegistry,
    job_id: i64,
}

impl Drop for ApplyControlGuard {
    fn drop(&mut self) {
        // Best-effort: a poisoned registry mutex during unwind must not double-panic.
        if let Ok(mut reg) = self.registry.lock() {
            reg.remove(&self.job_id);
        }
    }
}

/// Start applying an approved plan as a background job (F-601/F-607/F-904),
/// returning immediately with the new apply `jobs.id`.
///
/// Loads the plan, acquires the single-writer lock (AC-8), records the apply
/// `jobs` row carrying `mode`, registers an [`ApplyControl`] so the pause/resume/
/// stop commands can reach the running job (F-608, F-104), then SPAWNS the walk on
/// the async runtime and returns [`JobStarted`] straight away - like `scan_start`,
/// so the IPC call never blocks on the walk and the caller learns `job_id` while
/// the apply is still running (which is what makes `job_pause(job_id)` usable).
///
/// The spawned walk runs the plan's APPROVED operations through the executor: a
/// `DryRun` against a snapshot-seeded `MemFs` (AC-2), a `Real` apply against
/// `RealFs` (the actual disk). Journal-before-act (F-602, AC-10): each op's intent
/// row is flushed and committed BEFORE the filesystem call, a terminal `done`/
/// `failed` row after. It drives the `jobs` row to a terminal state -
/// `completed`, `failed` (a halted op, AC-5/6/7/9), or `stopped` (a cooperative
/// Stop, AC-26) - releasing both the durable lock (the terminal row) and the
/// in-process flag (on the spawned task's exit) on EVERY path, including a panic.
/// The per-job outcome (verified ops, discrepancy block) is read back via
/// [`job_status`](super::job::job_status).
#[tauri::command]
#[specta::specta]
pub async fn apply_start(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
    mode: ApplyMode,
) -> Result<JobStarted, AppError> {
    // In-process single-writer guard (AC-8): refuse a second concurrent apply in
    // THIS process instantly, before any DB work. Ownership of the flag is handed
    // to the SPAWNED task below (which clears it when the walk ends); a `preflight`
    // guard clears it on the `?` early-return paths BEFORE the spawn.
    let flag = state.apply_in_flight.clone();
    if flag
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::JobAlreadyRunning);
    }
    let mut preflight = ApplyInFlightGuard::new(flag.clone());

    let pool = &state.pool;

    // The plan and its operations. A missing plan is a precise PlanNotFound, never
    // a silent empty walk.
    let plan = get_plan(pool, plan_id)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?
        .ok_or(AppError::PlanNotFound { plan_id })?;
    let ops = get_plan_ops(pool, plan_id)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?;

    // Forward-tidying gate (F-604, AC-20): if a previous apply's after-the-fact
    // check found an UNACKNOWLEDGED discrepancy, forward tidying is paused until a
    // human acknowledges it. This gate is FORWARD-only and structural: an UNDO
    // (inverse) plan is exempt (undo is the remedy for a discrepancy), so a blocked
    // library can always be put back. `rollback_prepare` never routes through here,
    // so preparing an undo is never gated either. Checked before the lock so a
    // refused forward apply never even acquires it.
    ensure_forward_tidying_allowed(pool, &ops).await?;

    // Resolve the FD-34 apply scope (library root + set-aside root): the ONLY two
    // areas the walk may write into, and the roots the executor uses to substitute
    // the real job id into set-aside targets and to refuse any out-of-scope target.
    let scope = resolve_apply_scope(pool, plan.scan_id).await?;

    // Capture what the after-the-fact check needs before `scope` is moved into the
    // executor: the snapshot the plan was built over (the delta "before"), and the
    // library root the incremental rescan re-reads (AC-19).
    let before_scan_id = plan.scan_id;
    let library_root = scope.library_root.clone();

    // Durable single-writer lock (AC-8): refuse if an apply is already `running`,
    // else insert the `running` apply job (the lock) carrying `mode`. Released by
    // marking the job terminal in the spawned task (and by the startup reclaim
    // after a crash).
    let started_at = now_iso8601_utc();
    let job_id = acquire_apply_job(pool, mode, &started_at).await?;

    // Register the pause/resume/Stop control BEFORE spawning, so a pause/stop that
    // arrives the instant the walk starts already finds it. Deregistered when the
    // spawned task ends (any path) via `ApplyControlGuard`.
    let control = Arc::new(ApplyControl::new());
    state
        .apply_controls
        .lock()
        .expect("apply control registry poisoned")
        .insert(job_id, control.clone());

    // The preflight window is over and the walk is about to own the lock: disarm
    // the preflight guard so it does NOT clear the flag on this happy path (the
    // spawned task's own guard takes over releasing it).
    preflight.disarm();
    drop(preflight);

    let app_data_dir = abo_core::paths::app_data_dir();
    let reports_dir = abo_core::reports::plan_export_dir(&app_data_dir, plan_id, &plan.created_at);
    let scan_id = plan.scan_id;

    // Everything the task needs is owned (`'static`): the pool clones cheaply, the
    // ops/scope/paths are owned, and the control is an Arc. The task outlives this
    // command, so nothing borrows State.
    let pool_owned = pool.clone();
    let registry = state.apply_controls.clone();

    tauri::async_runtime::spawn(async move {
        let journal = SqliteJournal::new(pool_owned.clone());
        let pool_for_wrapper = pool_owned.clone();

        // The walk is identical code over either backend (the Vfs seam is what
        // makes a dry run a first-class product); building the executor and running
        // it are done INSIDE the task so even a snapshot-read failure marks the job
        // terminal here rather than stranding the lock. `walk_and_finalize` marks
        // the `jobs` row terminal on all its own paths; `run_apply_to_terminal`
        // adds the panic backstop AND releases the single-writer in-process flag +
        // the control registry on every path (including a panic).
        let work = async move {
            match mode {
                ApplyMode::DryRun => {
                    // Seed a MemFs from the plan's snapshot so the dry run walks a
                    // memory tree identical to what the plan was built over,
                    // resolving nothing to a real path (AC-2).
                    let entries = abo_core::scan::get_scan_entries(&pool_owned, scan_id)
                        .await
                        .map_err(|e| AppError::ApplyFailed {
                            detail: e.to_string(),
                        })?;
                    let memfs = MemFs::from_seed(&seed_from_entries(&entries));
                    let executor = Executor::with_scope(memfs, job_id, ops, scope);
                    walk_and_finalize(
                        &pool_owned,
                        executor,
                        &journal,
                        &reports_dir,
                        plan_id,
                        mode,
                        job_id,
                        &started_at,
                        before_scan_id,
                        &library_root,
                        &*control,
                    )
                    .await
                    .map(|_| ())
                }
                ApplyMode::Real => {
                    // Real apply against the actual filesystem. The human-only gate
                    // to run a Real apply against a real library is procedural
                    // (EXECUTION.md); this is the RealFs executor that gate authorizes.
                    let executor = Executor::with_scope(RealFs::new(), job_id, ops, scope);
                    walk_and_finalize(
                        &pool_owned,
                        executor,
                        &journal,
                        &reports_dir,
                        plan_id,
                        mode,
                        job_id,
                        &started_at,
                        before_scan_id,
                        &library_root,
                        &*control,
                    )
                    .await
                    .map(|_| ())
                }
            }
        };

        run_apply_to_terminal(pool_for_wrapper, job_id, flag, registry, work).await;
    });

    Ok(JobStarted { job_id })
}

/// Drive a spawned apply's `work` future to a terminal `jobs`-row state,
/// panic-safely, releasing the single-writer lock on every exit.
///
/// This is the single wrapper the spawned apply task funnels through (there is no
/// parallel copy of the terminal logic), the apply analogue of
/// [`run_job_to_terminal`](crate::run_job_to_terminal). It holds two RAII guards
/// for the WHOLE walk, so the in-process single-writer flag is cleared and the
/// job's [`ApplyControl`] is deregistered on EVERY exit - completion, a journaled
/// failure, a cooperative Stop, or a panic that unwinds the walk (AC-8).
///
/// `work` (the executor walk + finalize) marks the `jobs` row terminal on every one
/// of its OWN paths - `completed`, `stopped`, or `failed`; this wrapper's remaining
/// job is the panic backstop: it awaits `work` under [`FutureExt::catch_unwind`] so
/// that a panic inside the walk (a simulated process kill, or any bug) still lands
/// the `jobs` row `failed` with error_code `"internal-panic"` rather than leaving
/// it stuck `running` forever (the same hole `run_job_to_terminal` closes for
/// scans). A non-panic `Err` is marked `failed` idempotently here too, covering the
/// one pre-`walk_and_finalize` failure (the DryRun snapshot read) that would
/// otherwise leave the durable lock held; `walk_and_finalize`'s own error paths
/// already marked it, so the second mark is a harmless no-op.
///
/// (Under the release profile's `panic = "abort"` a panic aborts the process
/// before it can be caught; `catch_unwind`'s guarantee therefore holds for the
/// unwinding builds used by tests and `cargo run`, which is where a stuck
/// `running` row would otherwise be observable.)
pub async fn run_apply_to_terminal<Fut>(
    pool: SqlitePool,
    job_id: i64,
    flag: Arc<AtomicBool>,
    registry: ApplyControlRegistry,
    work: Fut,
) where
    Fut: Future<Output = Result<(), AppError>>,
{
    // These release the single-writer in-process flag and deregister the control
    // when this wrapper ends - after the terminal marking below, or on a panic that
    // unwinds this wrapper itself. The walk's OWN panic is contained by the
    // `catch_unwind` below, so on that path the guards drop at the normal block end,
    // AFTER the `jobs` row is marked failed.
    let _in_flight = ApplyInFlightGuard::new(flag);
    let _control_guard = ApplyControlGuard { registry, job_id };

    match AssertUnwindSafe(work).catch_unwind().await {
        // The walk finished (completed or stopped): `walk_and_finalize` already
        // drove the `jobs` row to its terminal state.
        Ok(Ok(())) => {}
        // A journaled/handled failure: ensure the row is terminal (idempotent).
        Ok(Err(err)) => {
            mark_apply_job_failed(&pool, job_id, err.code()).await;
        }
        // A panic unwound past all marking: land the row failed so the lock frees.
        Err(_panic) => {
            mark_apply_job_failed(&pool, job_id, "internal-panic").await;
        }
    }
    // `_in_flight` and `_control_guard` drop here, releasing the lock.
}

/// Run the executor walk over `executor` and finalize the apply job: mark it
/// terminal, export the undo file on a clean run, or surface the halt on a failure.
/// Generic over the `Vfs` backend so the DryRun (`MemFs`) and Real (`RealFs`) paths
/// share one implementation, and over the [`ExecControl`] `C` so the SAME code runs
/// with the live pause/Stop control on the production path and with the inert
/// [`NoControl`](abo_core::exec::NoControl) in tests.
///
/// It is now driven from a SPAWNED task (via [`run_apply_to_terminal`]), so it is
/// monomorphized to concrete `Send` types at the call site (`SqliteJournal` +
/// `RealFs`/`MemFs` + `ApplyControl`); no `Send` bound is asked of the generic
/// `Journal`/`ExecControl` traits themselves, which is what keeps those traits'
/// `async fn`s expressible (the documented P2 landmine).
#[allow(clippy::too_many_arguments)]
async fn walk_and_finalize<V: Vfs, C: ExecControl>(
    pool: &SqlitePool,
    executor: Executor<V>,
    journal: &SqliteJournal,
    reports_dir: &std::path::Path,
    plan_id: i64,
    mode: ApplyMode,
    job_id: i64,
    started_at: &str,
    before_scan_id: i64,
    library_root: &str,
    control: &C,
) -> Result<ApplyReport, AppError> {
    // journal-before-act: intent flushed and committed before each filesystem call
    // (F-602, AC-10). A failed intent flush is a hard stop (journal-write-failed).
    // The control is consulted at operation boundaries only (F-608 pause, F-104
    // Stop), never mid-op, and never writes a journal row of its own (AC-24, AC-25).
    let outcome = match executor
        .run_with_control(journal, started_at, control)
        .await
    {
        Ok(outcome) => outcome,
        Err(e) => {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    };

    // Did every approved op walk without an operation-level halt AND without a
    // cooperative Stop? A halt (AC-5/6/7/9) means a move failed and stopped the
    // group; a Stop (AC-26) means the human stopped it at a safe boundary. Either
    // way the walk did NOT run every approved op, so it does not export an undo file
    // (an undo over all approved ops would claim moves that never happened; the
    // journal is the record of the partial forward job).
    let walk_completed = outcome.halt.is_none() && !outcome.stopped;

    // Guard #2 (STRUCTURAL, by ordering): a walk that COMPLETED exports its
    // self-contained undo file (manifest) and re-emits the F-507 provenance report
    // (AC-11, AC-12) HERE, BEFORE the after-the-fact check runs. So even if the
    // check then finds a difference, the undo file is already written - a job whose
    // walk completed always keeps its undo, because the moves DID happen and the
    // user needs the undo. A HALTED walk exports no undo file (an undo over all
    // approved ops would claim moves that never happened; the journal is the durable
    // record a v0.6.0 reconciliation reads instead).
    if walk_completed {
        if let Err(e) =
            export_after_apply(pool, reports_dir, mode, job_id, plan_id, executor.ops()).await
        {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    }

    // The after-the-fact check (F-604). Prove the moves happened as planned by
    // re-checking the EXECUTOR'S OWN ops against the SAME Vfs the job ran on
    // (AC-18): a dry run verifies its MemFs, a Real apply verifies RealFs. This runs
    // on BOTH a clean and a halted walk, so a halt reports per-op truth (completed
    // ops verified good, the halted op reported as halted) rather than a blanket
    // failure - the P5 teardown-halt (fully restored, teardown op halted) reads
    // fairly, and P8 frames it.
    let verify_report = verify_job(executor.vfs(), executor.ops(), &outcome);

    // AC-19: re-read reality through the EXISTING scanner and compute the delta
    // health metrics, for a COMPLETED Real apply only (a dry run changed nothing on
    // disk; a halted walk left a partial state we do not re-scan). Best-effort: a
    // rescan failure is logged and never fails the apply.
    let delta = if mode == ApplyMode::Real && walk_completed && !library_root.is_empty() {
        match delta_health_metrics(pool, before_scan_id, std::path::Path::new(library_root)).await {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("post-apply rescan/delta for job {job_id} failed: {e}");
                None
            }
        }
    } else {
        None
    };

    // Export the after-the-fact check report artifact beside the undo file (F-1002).
    // Best-effort: a report-write failure is logged, never fails the apply.
    let roots = affected_roots(executor.ops());
    let check = CheckReport::build(mode, &verify_report, roots, delta);
    if let Err(e) = write_check_report(reports_dir, &check) {
        log::warn!("could not write the after-the-fact check report for job {job_id}: {e}");
    }

    // AC-20: a difference the check found blocks further FORWARD tidy-ups until a
    // human acknowledges it. The block is durable (survives a restart) and
    // append-only. If recording it fails, mark the job failed (releasing the lock)
    // and surface the error rather than leaving forward tidying silently open.
    let blocked = verify_report.has_discrepancy();
    if blocked {
        let now = now_iso8601_utc();
        if let Err(e) = record_block(
            pool,
            job_id,
            &now,
            &verify_report.discrepancy_summary_json(),
        )
        .await
        {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    }

    // Terminal state. A halted walk surfaces its halt exactly as before (the
    // teardown-halt case included); the after-the-fact check above already recorded
    // its per-op truth to the report.
    if let Some(halt) = &outcome.halt {
        let err = halt_to_error(halt);
        mark_apply_job_failed(pool, job_id, err.code()).await;
        return Err(err);
    }

    // Cooperative Stop (F-104/FD-02, AC-26): a DISTINCT terminal state, NOT failed.
    // The executed prefix is journaled and was just verified over the ops that DID
    // run; NO undo file was exported (guard #2 above). The stopped state's
    // remediation story is undo-via-journal-tail or resume-forward-later, both owned
    // by v0.6.0 (F-606); this release leaves the state honest and durable.
    if outcome.stopped {
        mark_apply_job_stopped(pool, job_id).await;
        return Ok(ApplyReport {
            plan_id,
            job_id,
            dry_run: mode == ApplyMode::DryRun,
            ops_walked: outcome.ops_walked as i64,
            verified_ops: verify_report.verified_count() as i64,
            discrepancy_count: verify_report.discrepancy_count() as i64,
            blocked,
        });
    }

    mark_apply_job_completed(pool, job_id).await;

    Ok(ApplyReport {
        plan_id,
        job_id,
        dry_run: mode == ApplyMode::DryRun,
        ops_walked: outcome.ops_walked as i64,
        verified_ops: verify_report.verified_count() as i64,
        discrepancy_count: verify_report.discrepancy_count() as i64,
        blocked,
    })
}

/// Resolve the FD-34 apply scope for the plan's `scan_id`: the library root the
/// plan was built over (the scan's `root_path`) plus the resolved set-aside root
/// (the F-803 `set_aside_root` setting when configured, else the FD-34 default
/// sibling `<library-parent>\Set Aside\`). These MUST match what the builder used
/// when it stamped the set-aside targets; in the normal flow (generate then apply
/// without changing settings) they do, and the executor's scope guard is a safe
/// backstop if they ever diverge.
async fn resolve_apply_scope(pool: &SqlitePool, scan_id: i64) -> Result<ApplyScope, AppError> {
    let root_path: Option<String> = sqlx::query_scalar("SELECT root_path FROM scans WHERE id = ?")
        .bind(scan_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?;
    let library_root = root_path.ok_or_else(|| AppError::ApplyFailed {
        detail: "the plan's scan has no recorded library root".to_string(),
    })?;
    let settings = abo_core::db::settings::get_settings(pool).await?;
    let set_aside_root = settings
        .set_aside_root
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_set_aside_root(&library_root));
    Ok(ApplyScope {
        library_root,
        set_aside_root,
    })
}

/// Map an executor [`ExecHalt`] onto the typed IPC error taxonomy. The stable code
/// on the halt is the single source of truth; the path shown is the one that
/// matters for each hazard (the vacant/denied source, or the occupied target).
fn halt_to_error(halt: &ExecHalt) -> AppError {
    match halt.code {
        "source-vanished" => AppError::SourceVanished {
            path: halt.source_path.clone(),
        },
        "target-appeared" => AppError::TargetAppeared {
            path: halt.target_path.clone(),
        },
        "copy-verify-mismatch" => AppError::CopyVerifyMismatch {
            path: halt.source_path.clone(),
        },
        "access-denied" => AppError::AccessDenied {
            path: halt.source_path.clone(),
        },
        _ => AppError::ApplyFailed {
            detail: halt.detail.clone(),
        },
    }
}

/// Map a snapshot's [`EntryRow`]s to the executor's seed shape (path + size +
/// is_dir). A directory (or an unstatable entry) contributes size 0.
fn seed_from_entries(entries: &[EntryRow]) -> Vec<SeedEntry> {
    entries
        .iter()
        .map(|e| SeedEntry {
            path: e.path.clone(),
            size: e.size.max(0) as u64,
            is_dir: e.kind == "dir",
        })
        .collect()
}

/// Best-effort: mark the apply `jobs` row `completed`. Runs after the dry-run walk
/// already finished, so a secondary DB error here is logged and swallowed rather
/// than reported as an apply failure (the walk itself succeeded).
async fn mark_apply_job_completed(pool: &SqlitePool, job_id: i64) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query("UPDATE jobs SET state = 'completed', finished_at = ? WHERE id = ?")
        .bind(&finished_at)
        .bind(job_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        log::warn!("failed to mark apply job {job_id} completed: {e}");
    }
}

/// Best-effort: mark the apply `jobs` row `stopped` (F-104/FD-02 cooperative
/// Stop, AC-26) - a terminal state DISTINCT from `failed` and `completed`. Carries
/// no error_code (a Stop is not a failure); the durable single-writer lock is
/// released by this row no longer being `running`. A secondary DB error here is
/// logged and swallowed (the walk already stopped cleanly; the row-state write is
/// the only durable signal and its failure defers reclaim to the startup sweep).
async fn mark_apply_job_stopped(pool: &SqlitePool, job_id: i64) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query("UPDATE jobs SET state = 'stopped', finished_at = ? WHERE id = ?")
        .bind(&finished_at)
        .bind(job_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        log::warn!("failed to mark apply job {job_id} stopped: {e}");
    }
}

/// Best-effort: mark the apply `jobs` row `failed`, recording the stable error
/// `code`. Runs on the walk-or-export error path; the real error is already being
/// returned to the caller, so a secondary DB error here is only logged.
async fn mark_apply_job_failed(pool: &SqlitePool, job_id: i64, error_code: &str) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query(
        "UPDATE jobs SET state = 'failed', finished_at = ?, error_code = ? WHERE id = ?",
    )
    .bind(&finished_at)
    .bind(error_code)
    .bind(job_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        log::warn!("failed to mark apply job {job_id} failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The `Vfs` trait must be in scope to call its methods (exists/is_dir/metadata)
    // on the seeded `MemFs`.
    use abo_core::exec::Vfs;
    use std::path::Path;

    /// One `entries`-row shape, the wire form the snapshot read returns.
    fn entry(path: &str, kind: &str, size: i64) -> EntryRow {
        EntryRow {
            id: 0,
            scan_id: 1,
            parent_id: None,
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            kind: kind.to_string(),
            file_class: if kind == "dir" {
                None
            } else {
                Some("audio".to_string())
            },
            size,
            mtime: None,
            depth: 0,
        }
    }

    /// The exact `EntryRow` -> `SeedEntry` -> `MemFs` path `apply_start` uses:
    /// `kind == "dir"` maps to `is_dir`, and a negative (unstatable) size clamps to
    /// 0. The seeded `MemFs` then answers from memory with those mapped kinds/sizes.
    #[test]
    fn seed_from_entries_maps_kind_and_clamps_size() {
        let rows = vec![
            entry(r"E:\lib", "dir", 0),
            entry(r"E:\lib\Book.m4b", "file", 4096),
            // An unstatable entry can carry a negative size; it must clamp to 0.
            entry(r"E:\lib\Bad.m4b", "file", -1),
        ];

        let seed = seed_from_entries(&rows);
        assert_eq!(seed.len(), 3);
        assert!(seed[0].is_dir, "a dir entry maps to is_dir");
        assert_eq!(seed[0].size, 0);
        assert!(!seed[1].is_dir, "a file entry maps to !is_dir");
        assert_eq!(seed[1].size, 4096, "a file's size is preserved");
        assert_eq!(seed[2].size, 0, "a negative size clamps to 0");
        assert!(!seed[2].is_dir);

        // The MemFs the seed builds answers from memory with the mapped kinds.
        let memfs = MemFs::from_seed(&seed);
        assert!(memfs.is_dir(Path::new(r"E:\lib")));
        assert!(!memfs.is_dir(Path::new(r"E:\lib\Book.m4b")));
        assert!(memfs.exists(Path::new(r"E:\lib\Book.m4b")));
        assert_eq!(
            memfs.metadata(Path::new(r"E:\lib\Book.m4b")).unwrap().size,
            4096
        );
    }

    /// A halted walk (here a source-vanished op against an empty MemFs) drives
    /// `walk_and_finalize` down its halt branch: it surfaces the halt code, marks the
    /// apply `jobs` row `failed`, and exports NO undo file (an undo file over all
    /// approved ops would claim moves that never happened - the journal is the
    /// durable record instead).
    #[tokio::test]
    async fn a_halted_walk_marks_the_job_failed_and_writes_no_undo_file() {
        use abo_core::db::open_db;
        use abo_core::db::plans::PlanOpRow;
        use abo_core::exec::MANIFEST_JSON_BASENAME;

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        // Acquire the running apply job (the lock + the row walk_and_finalize marks).
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, "2026-07-18T00:00:00Z")
            .await
            .expect("acquire apply job");

        // An approved move whose source does not exist in the (empty) MemFs, so the
        // TOCTOU re-check halts with source-vanished.
        let op = PlanOpRow {
            id: 1,
            plan_id: 1,
            seq: 0,
            op_group: "loose-root-books".to_string(),
            kind: "move".to_string(),
            kind_reason: None,
            source_path: "E:/lib/GONE.m4b".to_string(),
            target_path: "E:/lib/Author/GONE.m4b".to_string(),
            rationale: "test.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        };
        let executor = Executor::new(MemFs::new(), job_id, vec![op]);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let err = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            1,
            ApplyMode::DryRun,
            job_id,
            "2026-07-18T00:00:00Z",
            // before_scan_id + library_root: a DryRun halt does no rescan, so these
            // are inert here.
            0,
            "",
            &abo_core::exec::NoControl,
        )
        .await
        .expect_err("a halted walk surfaces the halt as an error");
        assert_eq!(err.code(), "source-vanished");

        // The jobs row is marked failed (the lock is released), not left running.
        let state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("job state");
        assert_eq!(state, "failed");

        // No undo file was exported for the halted run.
        assert!(
            !reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a halted run must not export an undo file"
        );

        // The journal is consistent: the one intent has a terminal (failed) row.
        let phases: Vec<String> =
            sqlx::query_scalar("SELECT phase FROM journal WHERE job_id = ? ORDER BY id")
                .bind(job_id)
                .fetch_all(&pool)
                .await
                .expect("journal rows");
        assert_eq!(phases, vec!["intent".to_string(), "failed".to_string()]);
    }

    // ---- F-604 after-the-fact check wiring (v0.5.0 Phase 6) ----

    use abo_core::db::plans::PlanOpRow;

    const P6_NOW: &str = "2026-07-18T00:00:00Z";

    /// Seed the minimal `scans`/`rulesets`/`plans` rows so `export_after_apply`'s
    /// `manifests` FK (to `plans` and `jobs`) resolves, returning the new plan id.
    /// The executor is driven with in-memory ops (below); this only needs a plan
    /// row to exist for the undo-file export.
    async fn seed_plan(pool: &SqlitePool) -> i64 {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:/lib', ?, 'completed')",
        )
        .bind(P6_NOW)
        .execute(pool)
        .await
        .expect("scan")
        .last_insert_rowid();
        let ruleset_id = abo_core::db::rulesets::insert_ruleset(
            pool,
            &abo_core::db::rulesets::NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            P6_NOW,
        )
        .await
        .expect("ruleset");
        sqlx::query(
            "INSERT INTO plans (scan_id, ruleset_id, created_at, status) VALUES (?, ?, ?, 'draft')",
        )
        .bind(scan_id)
        .bind(ruleset_id)
        .bind(P6_NOW)
        .execute(pool)
        .await
        .expect("plan")
        .last_insert_rowid()
    }

    /// One approved move op, in memory (the executor walks these directly).
    fn move_op(source: &str, target: &str, size: i64) -> PlanOpRow {
        PlanOpRow {
            id: 1,
            plan_id: 1,
            seq: 0,
            op_group: "loose-root-books".to_string(),
            kind: "move".to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size: size,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    /// AC-18/AC-20: a CLEAN completed walk verifies every op and records NO block;
    /// forward tidying stays open. The undo file is exported (the walk completed).
    #[tokio::test]
    async fn a_clean_completed_walk_records_no_block() {
        use abo_core::exec::{forward_tidying_blocked, MANIFEST_JSON_BASENAME};

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = abo_core::db::open_db(db.path()).await.expect("open_db");
        let plan_id = seed_plan(&pool).await;
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, P6_NOW)
            .await
            .expect("acquire apply job");

        let seed = vec![
            SeedEntry {
                path: "E:/lib".into(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/loose.m4b".into(),
                size: 4096,
                is_dir: false,
            },
        ];
        let ops = vec![move_op("E:/lib/loose.m4b", "E:/lib/Author/loose.m4b", 4096)];
        let executor = Executor::new(MemFs::from_seed(&seed), job_id, ops);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let report = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            plan_id,
            ApplyMode::DryRun,
            job_id,
            P6_NOW,
            0,
            "",
            &abo_core::exec::NoControl,
        )
        .await
        .expect("a clean walk completes");

        assert!(!report.blocked, "a clean apply does not block");
        assert_eq!(report.discrepancy_count, 0);
        assert_eq!(report.verified_ops, 1);
        assert!(
            !forward_tidying_blocked(&pool).await.unwrap(),
            "forward tidying stays open after a clean apply"
        );
        assert!(
            reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a completed walk exports its undo file"
        );
    }

    /// AC-18/AC-20 + guard #2 (structural): a completed walk whose after-the-fact
    /// check finds a difference (an injected fault: the target reports missing at
    /// check time) records a durable block AND STILL exports its undo file - the
    /// moves happened, so the user keeps the undo even though the check failed.
    #[tokio::test]
    async fn a_discrepancy_after_a_completed_walk_blocks_but_keeps_the_undo_file() {
        use abo_core::exec::{
            forward_tidying_blocked, VfsError, VfsMetadata, MANIFEST_JSON_BASENAME,
        };

        // A Vfs that hides ONE target from `exists`/`metadata` while delegating
        // everything else (including the move itself) to an inner MemFs: the move
        // succeeds, but the after-the-fact check sees the target as missing.
        struct HauntedFs {
            inner: MemFs,
            haunted: std::path::PathBuf,
        }
        impl Vfs for HauntedFs {
            fn exists(&self, p: &Path) -> bool {
                if p == self.haunted {
                    return false;
                }
                self.inner.exists(p)
            }
            fn is_dir(&self, p: &Path) -> bool {
                self.inner.is_dir(p)
            }
            fn metadata(&self, p: &Path) -> Result<VfsMetadata, VfsError> {
                if p == self.haunted {
                    return Err(VfsError::NotFound(p.to_path_buf()));
                }
                self.inner.metadata(p)
            }
            fn rename(&self, a: &Path, b: &Path) -> Result<(), VfsError> {
                self.inner.rename(a, b)
            }
            fn copy_file(&self, a: &Path, b: &Path) -> Result<u64, VfsError> {
                self.inner.copy_file(a, b)
            }
            fn remove_file(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_file(p)
            }
            fn remove_dir(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_dir(p)
            }
            fn create_dir_all(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.create_dir_all(p)
            }
        }

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = abo_core::db::open_db(db.path()).await.expect("open_db");
        let plan_id = seed_plan(&pool).await;
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, P6_NOW)
            .await
            .expect("acquire apply job");

        let seed = vec![
            SeedEntry {
                path: "E:/lib".into(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/loose.m4b".into(),
                size: 4096,
                is_dir: false,
            },
        ];
        let haunted = HauntedFs {
            inner: MemFs::from_seed(&seed),
            haunted: std::path::PathBuf::from("E:/lib/Author/loose.m4b"),
        };
        let ops = vec![move_op("E:/lib/loose.m4b", "E:/lib/Author/loose.m4b", 4096)];
        let executor = Executor::new(haunted, job_id, ops);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let report = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            plan_id,
            ApplyMode::DryRun,
            job_id,
            P6_NOW,
            0,
            "",
            &abo_core::exec::NoControl,
        )
        .await
        .expect("the walk completed (the discrepancy is post-facto, not a halt)");

        // AC-20: the check found a difference, so a durable block was recorded.
        assert!(report.blocked, "the discrepancy blocks further tidying");
        assert_eq!(report.discrepancy_count, 1);
        assert!(
            forward_tidying_blocked(&pool).await.unwrap(),
            "forward tidying is blocked until acknowledged"
        );
        // Guard #2: the undo file was STILL exported (the walk completed).
        assert!(
            reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a completed walk keeps its undo file even when the check fails"
        );
        // The block is durable: a `verification_blocks` raised row exists.
        let raised: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_blocks WHERE job_id = ? AND state = 'raised'",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(raised, 1);
    }

    // ---- F-104/FD-02 cooperative Stop wiring (v0.5.0 Phase 7) ----

    use std::sync::atomic::AtomicUsize;

    /// A test [`ExecControl`] that requests a Stop once `stop_requested` has been
    /// polled `after` times. The executor polls it TWICE per completed op boundary
    /// (before and after the pause barrier), so `after = 2` stops the walk after
    /// exactly ONE op has run - a real, journaled prefix, not a degenerate
    /// zero-op stop. Never pauses.
    struct StopAfter {
        checks: AtomicUsize,
        after: usize,
    }
    impl StopAfter {
        fn new(after: usize) -> Self {
            Self {
                checks: AtomicUsize::new(0),
                after,
            }
        }
    }
    impl ExecControl for StopAfter {
        fn stop_requested(&self) -> bool {
            self.checks.fetch_add(1, Ordering::SeqCst) >= self.after
        }
        async fn pause_barrier(&self) {}
    }

    /// AC-26: a cooperative Stop mid-walk drives `walk_and_finalize` down its
    /// STOPPED branch: the executed prefix is journaled and verified, the `jobs` row
    /// is marked with the DISTINCT `stopped` state (not `failed`, not `completed`),
    /// NO undo file is exported for the partial forward job, and the single-writer
    /// lock is released so a fresh apply can acquire.
    #[tokio::test]
    async fn a_stopped_walk_marks_the_job_stopped_and_writes_no_undo_file() {
        use abo_core::db::open_db;
        use abo_core::exec::MANIFEST_JSON_BASENAME;

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let plan_id = seed_plan(&pool).await;
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, P6_NOW)
            .await
            .expect("acquire apply job");

        // Two approved moves; the Stop lands after the first one runs.
        let seed = vec![
            SeedEntry {
                path: "E:/lib".into(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/one.m4b".into(),
                size: 10,
                is_dir: false,
            },
            SeedEntry {
                path: "E:/lib/two.m4b".into(),
                size: 20,
                is_dir: false,
            },
        ];
        let mut op_two = move_op("E:/lib/two.m4b", "E:/lib/Author/two.m4b", 20);
        op_two.id = 2;
        op_two.seq = 1;
        let ops = vec![
            move_op("E:/lib/one.m4b", "E:/lib/Author/one.m4b", 10),
            op_two,
        ];
        let executor = Executor::new(MemFs::from_seed(&seed), job_id, ops);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let report = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            plan_id,
            ApplyMode::DryRun,
            job_id,
            P6_NOW,
            0,
            "",
            &StopAfter::new(2),
        )
        .await
        .expect("a stopped walk is not an error");

        // Only the pre-Stop prefix ran, and it verified.
        assert_eq!(report.ops_walked, 1, "one op ran before the Stop");
        assert_eq!(report.verified_ops, 1, "the executed prefix was verified");

        // The DISTINCT terminal state: stopped, not failed and not completed.
        let state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("job state");
        assert_eq!(state, "stopped");

        // No undo file for the partial forward job (the journal is the record).
        assert!(
            !reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a stopped forward job exports no undo file"
        );

        // The journal is consistent over the executed prefix: the one op has an
        // intent and a done, and there is no failed row.
        let phases: Vec<String> =
            sqlx::query_scalar("SELECT phase FROM journal WHERE job_id = ? ORDER BY id")
                .bind(job_id)
                .fetch_all(&pool)
                .await
                .expect("journal rows");
        assert_eq!(phases, vec!["intent".to_string(), "done".to_string()]);

        // The single-writer lock was released (the row is no longer `running`), so a
        // fresh apply acquires cleanly.
        acquire_apply_job(&pool, ApplyMode::DryRun, "2026-07-18T02:00:00Z")
            .await
            .expect("a fresh apply acquires after a stopped job releases the lock");
    }
}
