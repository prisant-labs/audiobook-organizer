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

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use abo_core::db::DbOpenOutcome;
// `AppError` is re-exported from `abo-core::ipc`, so the whole command surface
// names one taxonomy; every `Result<_, AppError>` here is a valid tauri-specta
// return because `AppError` derives `specta::Type` in the core.
use abo_core::ipc::{AppError, DbStatus, EntryRow, JobStarted};
use abo_core::scan::walk::now_iso8601_utc;
use futures::FutureExt;
use sqlx::SqlitePool;

use crate::events::{emit_job_completed, emit_job_failed};
use crate::AppState;

/// Start a live scan of `root` as a background job (F-104), returning
/// immediately with the new job's id.
///
/// Records a `running` `jobs` row, spawns `abo_core::scan::run_scan` on Tauri's
/// async runtime, and returns [`JobStarted`] straight away so the IPC call never
/// blocks on the walk. When the spawned scan finishes, the task marks the `jobs`
/// row `completed` (and emits `job:completed { job_id, scan_id }`) or `failed`
/// with the stable error `code` (and emits `job:failed { job_id, code }`).
///
/// Spine scope (this phase's brief): the `jobs` handling is deliberately minimal
/// - no progress rows and no cancellation. `root` arrives as a plain string from
/// the frontend because the spine has no dialog plugin; the backend-mediated
/// folder picker (tauri-plugin-dialog, F-909) arrives at v0.4.0, together with
/// the capability-model change it needs. That is acceptable here because the
/// tracer UI (Phase 6) is disposable (FD-29).
#[tauri::command]
#[specta::specta]
pub async fn scan_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    root: String,
) -> Result<JobStarted, AppError> {
    // Record the running job row up front so the returned job_id is real and the
    // later event can correlate to it. A failure to even record the job is the
    // only thing that fails the call itself; the scan runs in the background.
    let started_at = now_iso8601_utc();
    let job_id = insert_scan_job(&state.pool, &started_at).await?;

    // Everything the spawned task needs is owned (`'static`): the pool clones
    // cheaply (it wraps a shared handle), the AppHandle is clonable, and the root
    // is already owned. The task outlives this command, so nothing borrows State.
    let pool = state.pool.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Clone the pool once for the scan work itself; the terminal-state wrapper
        // takes ownership of the outer clone so it can still write the `jobs` row
        // even if the scan future is dropped or unwinds.
        let scan_pool = pool.clone();
        let root_path = PathBuf::from(&root);
        // `job_id` is Copy; `handle` is cloned so the completed and failed emit
        // closures each own one (only one of the two ever runs).
        let handle_completed = handle.clone();
        run_job_to_terminal(
            pool,
            job_id,
            async move {
                abo_core::scan::run_scan(&scan_pool, &root_path)
                    .await
                    .map(|summary| summary.scan_id)
            },
            move |scan_id| emit_job_completed(&handle_completed, job_id, scan_id),
            move |code| emit_job_failed(&handle, job_id, code),
        )
        .await;
    });

    Ok(JobStarted { job_id })
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
/// parallel copy of the completed/failed logic). It awaits `work` under
/// [`FutureExt::catch_unwind`] so all three outcomes reach a terminal row:
///
/// - `Ok(scan_id)`: mark the row `completed`, then call `on_completed(scan_id)`.
/// - `Err(e)`: mark the row `failed` with `e.code()`, then call `on_failed(code)`.
/// - a panic inside `work`: catch the unwind, mark the row `failed` with
///   error_code `"internal-panic"`, then call `on_failed("internal-panic")`.
///
/// The panic arm is the fix for the swallowed-panic hole: without it, a panic in
/// the scan future is discarded by the async runtime, leaving the `jobs` row stuck
/// in `running` forever and never emitting `job:failed`. The success and error
/// arms are behavior-identical to the pre-fix inline `match`. Event emission is
/// injected as closures so the wrapper stays Tauri-free and unit-testable.
///
/// (Under the release profile's `panic = "abort"`, a panic aborts the process
/// before it can be caught; catch_unwind's guarantee therefore holds for the
/// unwinding builds used by tests and `cargo run`, which is where a stuck
/// `running` row would otherwise be observable.)
pub async fn run_job_to_terminal<Fut, C, E>(
    pool: SqlitePool,
    job_id: i64,
    work: Fut,
    on_completed: C,
    on_failed: E,
) where
    Fut: std::future::Future<Output = Result<i64, AppError>>,
    C: FnOnce(i64),
    E: FnOnce(&str),
{
    match AssertUnwindSafe(work).catch_unwind().await {
        Ok(Ok(scan_id)) => {
            mark_job_completed(&pool, job_id).await;
            on_completed(scan_id);
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
