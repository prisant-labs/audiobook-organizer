//! Panic-safety cover for the spawned scan job's terminal-state wrapper.
//!
//! v0.1.0 spine, Phase 5. `scan_start` runs its scan on Tauri's async runtime and
//! funnels the outcome through [`abo_lib::run_job_to_terminal`]. These tests drive
//! that wrapper directly (the same code path the command uses, not a copy) to pin
//! all three terminal outcomes, with the panic case as the load-bearing one: a
//! panic inside the scan future must still land the `jobs` row `failed` with
//! error_code `"internal-panic"` and still invoke the failed-emit closure, rather
//! than being swallowed by the runtime and leaving the row stuck in `running`.
//!
//! This is an integration test (in `tests/`) for the same reason as
//! `export_bindings`: the src-tauri build script attaches a comctl32-v6 activation
//! manifest to `[[test]]` binaries only, and a Tauri-linked test executable fails
//! to start on Windows without it (STATUS_ENTRYPOINT_NOT_FOUND). See `build.rs`.

use abo_core::db::open_db;
use abo_lib::JobEnd;
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;

/// Open a fresh migrated pool in a temp dir; the migration creates the `jobs`
/// table the wrapper writes.
async fn fresh_pool() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().expect("tempdir");
    let (pool, _outcome) = open_db(dir.path()).await.expect("open_db on empty dir");
    (dir, pool)
}

/// Insert a `running` scan job row exactly as `scan_start` does, returning its id.
async fn insert_running_job(pool: &SqlitePool) -> i64 {
    let result =
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('scan', 'running', ?)")
            .bind("2026-01-01T00:00:00Z")
            .execute(pool)
            .await
            .expect("insert running job");
    result.last_insert_rowid()
}

/// Read back `(state, error_code)` for a job row.
async fn job_state(pool: &SqlitePool, job_id: i64) -> (String, Option<String>) {
    let row = sqlx::query("SELECT state, error_code FROM jobs WHERE id = ?")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .expect("job row exists");
    (
        row.get::<String, _>("state"),
        row.get::<Option<String>, _>("error_code"),
    )
}

#[tokio::test]
async fn panicking_future_marks_job_internal_panic() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_job(&pool).await;

    let mut completed_called = false;
    let mut cancelled_called = false;
    let mut failed_code: Option<String> = None;

    // A future that panics before yielding any result. Without catch_unwind this
    // panic would be swallowed by the runtime, leaving the row 'running' forever.
    abo_lib::run_job_to_terminal(
        pool.clone(),
        job_id,
        async {
            panic!("deliberate scan panic");
            #[allow(unreachable_code)]
            Ok(JobEnd::Completed(0))
        },
        |_scan_id| completed_called = true,
        || cancelled_called = true,
        |code| failed_code = Some(code.to_string()),
    )
    .await;

    let (state, error_code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "failed",
        "a panicking scan future must mark the job failed"
    );
    assert_eq!(
        error_code.as_deref(),
        Some("internal-panic"),
        "the panic arm must record error_code internal-panic"
    );
    assert_eq!(
        failed_code.as_deref(),
        Some("internal-panic"),
        "the failed-emit closure must fire with the internal-panic code"
    );
    assert!(
        !completed_called,
        "the completed-emit closure must not fire on panic"
    );
    assert!(
        !cancelled_called,
        "the cancelled-emit closure must not fire on panic"
    );

    pool.close().await;
}

#[tokio::test]
async fn ok_future_marks_job_completed() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_job(&pool).await;

    let mut completed_scan_id: Option<i64> = None;
    let mut cancelled_called = false;
    let mut failed_called = false;

    abo_lib::run_job_to_terminal(
        pool.clone(),
        job_id,
        async { Ok(JobEnd::Completed(4242)) },
        |scan_id| completed_scan_id = Some(scan_id),
        || cancelled_called = true,
        |_code| failed_called = true,
    )
    .await;

    let (state, error_code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "completed",
        "an Ok scan future must mark the job completed"
    );
    assert_eq!(error_code, None, "a completed job must carry no error_code");
    assert_eq!(
        completed_scan_id,
        Some(4242),
        "the completed closure must see the scan_id"
    );
    assert!(
        !failed_called,
        "the failed closure must not fire on success"
    );
    assert!(
        !cancelled_called,
        "the cancelled closure must not fire on success"
    );

    pool.close().await;
}

#[tokio::test]
async fn err_future_marks_job_failed_with_code() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_job(&pool).await;

    let mut completed_called = false;
    let mut cancelled_called = false;
    let mut failed_code: Option<String> = None;

    let err = abo_core::ipc::AppError::ScanFailed {
        detail: "boom".to_string(),
    };
    let expected = err.code().to_string();

    abo_lib::run_job_to_terminal(
        pool.clone(),
        job_id,
        async move { Err(err) },
        |_scan_id| completed_called = true,
        || cancelled_called = true,
        |code| failed_code = Some(code.to_string()),
    )
    .await;

    let (state, error_code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "failed",
        "an Err scan future must mark the job failed"
    );
    assert_eq!(
        error_code.as_deref(),
        Some(expected.as_str()),
        "the error arm must record the AppError's stable code"
    );
    assert_eq!(
        failed_code,
        Some(expected),
        "the failed closure must see the same code"
    );
    assert!(
        !completed_called,
        "the completed closure must not fire on error"
    );
    assert!(
        !cancelled_called,
        "the cancelled closure must not fire on error"
    );

    pool.close().await;
}

/// AC-104.2: a cancelled scan future marks the `jobs` row `cancelled` (no error
/// code), fires only the cancelled closure, and the partial snapshot was already
/// discarded core-side (not this wrapper's concern).
#[tokio::test]
async fn cancelled_future_marks_job_cancelled() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_job(&pool).await;

    let mut completed_called = false;
    let mut cancelled_called = false;
    let mut failed_called = false;

    abo_lib::run_job_to_terminal(
        pool.clone(),
        job_id,
        async { Ok(JobEnd::Cancelled(7)) },
        |_scan_id| completed_called = true,
        || cancelled_called = true,
        |_code| failed_called = true,
    )
    .await;

    let (state, error_code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "cancelled",
        "a cancelled scan future must mark the job cancelled"
    );
    assert_eq!(error_code, None, "a cancelled job carries no error_code");
    assert!(cancelled_called, "the cancelled closure must fire");
    assert!(
        !completed_called,
        "the completed closure must not fire on cancel"
    );
    assert!(!failed_called, "the failed closure must not fire on cancel");

    pool.close().await;
}

/// AC-104.4: a `jobs` row persists across a process restart and is visible as
/// not-completed after a killed scan. Simulated by inserting a `running` job,
/// dropping the pool (the "kill"), reopening the SAME database directory (the
/// "restart"), and asserting the row survives as `running`.
#[tokio::test]
async fn killed_scan_job_row_visible_after_restart() {
    let dir = TempDir::new().expect("tempdir");

    // First "process": open, insert a running job, then drop the pool as if the
    // process were killed mid-scan (no terminal state was ever written).
    let (pool1, _outcome) = open_db(dir.path()).await.expect("open_db first");
    let job_id = insert_running_job(&pool1).await;
    pool1.close().await;

    // "Restart": reopen the same database directory.
    let (pool2, _outcome) = open_db(dir.path()).await.expect("open_db after restart");
    let (state, error_code) = job_state(&pool2, job_id).await;
    assert_eq!(
        state, "running",
        "a killed scan's job row must remain visible as not-completed after restart"
    );
    assert_eq!(
        error_code, None,
        "a killed job never recorded an error code"
    );

    pool2.close().await;
}
