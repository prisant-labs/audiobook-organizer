//! Tauri IPC command handlers (the IPC command layer).
//!
//! v0.1.0 spine, Phase 5. Each `#[tauri::command]` + `#[specta::specta]` here is
//! a thin adapter: it pulls the shared pool (and, for events, the `AppHandle`)
//! out of managed [`AppState`](crate::AppState), calls into `abo-core`, and
//! returns the core's typed result/error verbatim. No product logic lives here;
//! the shell only crosses the IPC boundary (reference architecture Section 4).
//!
//! The three spine commands: [`scan_start`] (kick off a background scan and
//! return its `job_id`), [`scan_entries`] (read a snapshot back for the tracer),
//! and [`db_status`] (report whether startup recovered a corrupt database).
//!
//! v0.4.0 Phase 2 adds the F-803 settings commands (see [`settings`]).

pub mod settings;

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use abo_core::db::DbOpenOutcome;
// `AppError` is re-exported from `abo-core::ipc`, so the whole command surface
// names one taxonomy; every `Result<_, AppError>` here is a valid tauri-specta
// return because `AppError` derives `specta::Type` in the core.
use abo_core::ipc::{AppError, DbStatus, EntryRow, JobStarted};
use abo_core::job::{CancelFlag, JobContext, ProgressUpdate};
use abo_core::scan::walk::now_iso8601_utc;
use abo_core::scan::ScanOutcome;
use futures::FutureExt;
use sqlx::SqlitePool;

use crate::events::{emit_job_completed, emit_job_failed, emit_job_progress};
use crate::AppState;

/// Emit a `job:progress` event at most once per this many recorded entries (plus
/// the very first), so a large scan does not flood the IPC channel while progress
/// stays visibly live. The underlying core reports every entry; this is purely
/// the shell's presentation-layer throttle.
const PROGRESS_EMIT_STEP: u64 = 64;

/// The terminal outcome of a scan job's work future, richer than a bare
/// `scan_id` so [`run_job_to_terminal`] can distinguish a completed scan from a
/// cooperatively cancelled one and drive the `jobs` row accordingly.
pub enum JobEnd {
    /// The scan finished; carries the `scans.id` of the snapshot written.
    Completed(i64),
    /// The scan was cancelled at a safe boundary; carries the `scans.id` of the
    /// discarded (cancelled) snapshot row.
    Cancelled(i64),
}

/// Start a live scan of `root` as a background job (F-104), returning
/// immediately with the new job's id.
///
/// Records a `running` `jobs` row, registers a [`CancelFlag`] under the job id so
/// [`scan_cancel`] can stop it, spawns `abo_core::scan::run_scan_with_job` on
/// Tauri's async runtime with a progress sink that emits `job:progress`, and
/// returns [`JobStarted`] straight away so the IPC call never blocks on the walk.
/// When the spawned scan reaches a terminal state the task marks the `jobs` row:
/// `completed` (emits `job:completed { job_id, scan_id }`), `failed` with the
/// stable error `code` (emits `job:failed { job_id, code }`), or `cancelled` (no
/// event: the requester already knows, and the `jobs` row is the durable signal).
/// The cancel flag is deregistered once the job is terminal, whichever way it ends.
///
/// The scan root is NO LONGER a frontend argument (FD-29 re-allowance, v0.4.0
/// Phase 2): `scan_start` uses the backend's sanctioned library root, loaded from
/// persisted settings at startup and updated by `settings_set` when the user
/// picks or changes the library folder. The frontend cannot ask the backend to
/// scan an arbitrary path; the only way a path enters the backend is the OS folder
/// picker (`tauri-plugin-dialog`) -> `settings_set` -> persisted `library_root`.
/// If no library is configured (first-run not completed), there is nothing to
/// scan and this returns [`AppError::RootNotFound`] so the shell can route to
/// first-run / re-pick.
#[tauri::command]
#[specta::specta]
pub async fn scan_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<JobStarted, AppError> {
    // The sanctioned root, snapshotted out of managed state (the lock is released
    // immediately; the PathBuf is owned by the spawned task below). No library
    // configured -> nothing to scan.
    let root_path = state
        .library_root
        .lock()
        .expect("library_root mutex poisoned")
        .clone()
        .ok_or_else(|| AppError::RootNotFound {
            path: "no library folder is configured".to_string(),
        })?;

    // Record the running job row up front so the returned job_id is real and the
    // later event can correlate to it. A failure to even record the job is the
    // only thing that fails the call itself; the scan runs in the background.
    let started_at = now_iso8601_utc();
    let job_id = insert_scan_job(&state.pool, &started_at).await?;

    // Register a cancel flag for this job so `scan_cancel` can flip it. A clone
    // goes into the job's JobContext; the registry keeps another until terminal.
    let cancel = CancelFlag::new();
    state
        .jobs
        .lock()
        .expect("jobs registry mutex poisoned")
        .insert(job_id, cancel.clone());

    // Everything the spawned task needs is owned (`'static`): the pool clones
    // cheaply (it wraps a shared handle), the AppHandle is clonable, and the root
    // is already owned. The task outlives this command, so nothing borrows State.
    let pool = state.pool.clone();
    let handle = app.clone();
    let jobs_registry = state.jobs.clone();
    tauri::async_runtime::spawn(async move {
        // Clone the pool once for the scan work itself; the terminal-state wrapper
        // takes ownership of the outer clone so it can still write the `jobs` row
        // even if the scan future is dropped or unwinds. `root_path` is the
        // sanctioned root captured above; move it into the task.
        let scan_pool = pool.clone();
        // `job_id` is Copy; `handle` is cloned so each terminal-emit closure owns
        // one (only one of the three ever runs).
        let handle_completed = handle.clone();
        let handle_failed = handle.clone();

        // Progress sink: emit `job:progress`, throttled to one event per
        // PROGRESS_EMIT_STEP recorded entries (plus the first). The core reports
        // every entry; this keeps the IPC channel calm on a huge tree.
        let handle_progress = handle.clone();
        let last_emitted = Arc::new(AtomicU64::new(0));
        let progress = Arc::new(move |update: ProgressUpdate| {
            let done = update.done;
            let last = last_emitted.load(Ordering::Relaxed);
            if done == 1 || done.saturating_sub(last) >= PROGRESS_EMIT_STEP {
                last_emitted.store(done, Ordering::Relaxed);
                emit_job_progress(
                    &handle_progress,
                    job_id,
                    done as i64,
                    update.total_estimate.map(|t| t as i64),
                    &update.current_label,
                );
            }
        });

        let ctx = JobContext::new(cancel, progress);

        run_job_to_terminal(
            pool,
            job_id,
            async move {
                abo_core::scan::run_scan_with_job(&scan_pool, &root_path, &[], &ctx)
                    .await
                    .map(|outcome| match outcome {
                        ScanOutcome::Completed(summary) => JobEnd::Completed(summary.scan_id),
                        ScanOutcome::Cancelled { scan_id } => JobEnd::Cancelled(scan_id),
                    })
            },
            move |scan_id| emit_job_completed(&handle_completed, job_id, scan_id),
            // Cancelled: the `jobs` row is marked `cancelled` by the wrapper; no
            // event is emitted (the requester initiated it, and this release has
            // no other listener - the durable `jobs` row is the signal).
            || {},
            move |code| emit_job_failed(&handle_failed, job_id, code),
        )
        .await;

        // Deregister the cancel flag now that the job is terminal, whichever way
        // it ended, so the registry does not leak entries.
        jobs_registry
            .lock()
            .expect("jobs registry mutex poisoned")
            .remove(&job_id);
    });

    Ok(JobStarted { job_id })
}

/// Request cancellation of a running scan job (F-104, FD-02 cooperative Stop).
///
/// Flips the [`CancelFlag`] registered for `job_id`, which the running scan
/// observes at its next safe entry boundary and stops there (discarding its
/// partial snapshot; see `run_scan_with_job`). Returns `true` if a running job
/// was found and signalled, `false` if no such in-flight job exists (already
/// finished, never started, or unknown id) - a clear no-op status, not an error
/// (a cancel of a not-running job is expected and harmless).
///
/// Synchronous: it only flips an atomic in managed state, so it needs no async
/// runtime and returns instantly without waiting for the scan to actually stop.
#[tauri::command]
#[specta::specta]
pub fn scan_cancel(state: tauri::State<'_, AppState>, job_id: i64) -> bool {
    let registry = state.jobs.lock().expect("jobs registry mutex poisoned");
    if let Some(flag) = registry.get(&job_id) {
        flag.cancel();
        true
    } else {
        false
    }
}

/// Read every entry of a completed snapshot back for the tracer UI (F-105).
///
/// Thin wrapper over [`abo_core::scan::get_scan_entries`]; returns the rows
/// path-sorted, exactly as the core produced them.
#[tauri::command]
#[specta::specta]
pub async fn scan_entries(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<Vec<EntryRow>, AppError> {
    abo_core::scan::get_scan_entries(&state.pool, scan_id).await
}

/// Report whether startup had to recover a corrupt database (P2).
///
/// Reads the [`DbOpenOutcome`] captured at startup out of managed state and maps
/// it to the wire [`DbStatus`]. Synchronous: it only reads already-resolved
/// state, so it needs no async runtime.
#[tauri::command]
#[specta::specta]
pub fn db_status(state: tauri::State<'_, AppState>) -> DbStatus {
    match &state.db_outcome {
        DbOpenOutcome::Normal => DbStatus {
            recovered: false,
            backup_path: None,
        },
        DbOpenOutcome::Recovered { backup_path } => DbStatus {
            recovered: true,
            backup_path: Some(backup_path.display().to_string()),
        },
    }
}

/// Drive a spawned job's `work` future to a terminal `jobs`-row state,
/// panic-safely.
///
/// This is the single wrapper [`scan_start`]'s spawned task uses (there is no
/// parallel copy of the terminal logic) - the one termination path every long
/// operation funnels through. It awaits `work` under [`FutureExt::catch_unwind`]
/// so all four outcomes reach a terminal row:
///
/// - `Ok(JobEnd::Completed(scan_id))`: mark the row `completed`, then call
///   `on_completed(scan_id)`.
/// - `Ok(JobEnd::Cancelled(_))`: mark the row `cancelled`, then call
///   `on_cancelled()` (F-104; the partial snapshot was already discarded core-side).
/// - `Err(e)`: mark the row `failed` with `e.code()`, then call `on_failed(code)`.
/// - a panic inside `work`: catch the unwind, mark the row `failed` with
///   error_code `"internal-panic"`, then call `on_failed("internal-panic")`.
///
/// The panic arm is the fix for the swallowed-panic hole: without it, a panic in
/// the scan future is discarded by the async runtime, leaving the `jobs` row stuck
/// in `running` forever and never emitting `job:failed`. The completed and error
/// arms are behavior-identical to the v0.1.0 wrapper; the cancelled arm is the
/// F-104 addition. Event emission is injected as closures so the wrapper stays
/// Tauri-free and unit-testable.
///
/// (Under the release profile's `panic = "abort"`, a panic aborts the process
/// before it can be caught; catch_unwind's guarantee therefore holds for the
/// unwinding builds used by tests and `cargo run`, which is where a stuck
/// `running` row would otherwise be observable.)
pub async fn run_job_to_terminal<Fut, C, X, E>(
    pool: SqlitePool,
    job_id: i64,
    work: Fut,
    on_completed: C,
    on_cancelled: X,
    on_failed: E,
) where
    Fut: std::future::Future<Output = Result<JobEnd, AppError>>,
    C: FnOnce(i64),
    X: FnOnce(),
    E: FnOnce(&str),
{
    match AssertUnwindSafe(work).catch_unwind().await {
        Ok(Ok(JobEnd::Completed(scan_id))) => {
            mark_job_completed(&pool, job_id).await;
            on_completed(scan_id);
        }
        Ok(Ok(JobEnd::Cancelled(_scan_id))) => {
            mark_job_cancelled(&pool, job_id).await;
            on_cancelled();
        }
        Ok(Err(err)) => {
            let code = err.code();
            mark_job_failed(&pool, job_id, code).await;
            on_failed(code);
        }
        Err(_panic) => {
            mark_job_failed(&pool, job_id, "internal-panic").await;
            on_failed("internal-panic");
        }
    }
}

/// Insert the initial `running` scan `jobs` row and return its assigned id.
///
/// A failure here is mapped to [`AppError::ScanFailed`]: the scan never started,
/// which is the scan-side hard-failure end of `scan_start`.
async fn insert_scan_job(pool: &SqlitePool, started_at: &str) -> Result<i64, AppError> {
    let result =
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('scan', 'running', ?)")
            .bind(started_at)
            .execute(pool)
            .await
            .map_err(|e| AppError::ScanFailed {
                detail: format!("could not record scan job: {e}"),
            })?;
    Ok(result.last_insert_rowid())
}

/// Best-effort: mark the `jobs` row `completed`. Runs in the spawned task after a
/// scan that already succeeded, so a secondary DB error here must not be treated
/// as a scan failure; it is logged and swallowed.
async fn mark_job_completed(pool: &SqlitePool, job_id: i64) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query("UPDATE jobs SET state = 'completed', finished_at = ? WHERE id = ?")
        .bind(&finished_at)
        .bind(job_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        log::warn!("failed to mark job {job_id} completed: {e}");
    }
}

/// Best-effort: mark the `jobs` row `cancelled` (F-104). Runs in the spawned task
/// after the core stopped the scan at a safe boundary and discarded its partial
/// snapshot; a secondary DB error here is logged and swallowed, since the durable
/// signal (the row state) is what matters and the scan already stopped cleanly.
async fn mark_job_cancelled(pool: &SqlitePool, job_id: i64) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query("UPDATE jobs SET state = 'cancelled', finished_at = ? WHERE id = ?")
        .bind(&finished_at)
        .bind(job_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        log::warn!("failed to mark job {job_id} cancelled: {e}");
    }
}

/// Best-effort: mark the `jobs` row `failed` and record the stable error `code`.
/// Runs in the spawned task on the scan-error path; a secondary DB error here is
/// logged and swallowed so it never masks the original scan error already carried
/// to the frontend via `job:failed`.
async fn mark_job_failed(pool: &SqlitePool, job_id: i64, code: &str) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query(
        "UPDATE jobs SET state = 'failed', error_code = ?, finished_at = ? WHERE id = ?",
    )
    .bind(code)
    .bind(&finished_at)
    .bind(job_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        log::warn!("failed to mark job {job_id} failed: {e}");
    }
}
