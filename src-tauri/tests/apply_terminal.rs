//! Panic-safety + lock-release cover for the spawned apply job's terminal-state
//! wrapper (v0.5.0 Phase 7).
//!
//! `apply_start` spawns the executor walk on the async runtime and funnels its
//! outcome through [`abo_lib::run_apply_to_terminal`]. These tests drive that
//! wrapper directly (the same code path the command uses, not a copy) to pin that
//! the single-writer lock is released on EVERY terminal path, with the panic case
//! as the load-bearing one: a panic inside the spawned walk (a simulated process
//! kill) must still land the `jobs` row `failed` with error_code `"internal-panic"`
//! AND release the in-process single-writer flag AND deregister the job's pause/Stop
//! control - rather than leaving the flag stuck set (blocking every future apply in
//! this process) and the row stuck `running`.
//!
//! Integration test (in `tests/`) for the same reason as `job_terminal` /
//! `export_bindings`: the src-tauri build script attaches a comctl32-v6 activation
//! manifest to `[[test]]` binaries only, and a Tauri-linked test executable fails
//! to start on Windows without it (STATUS_ENTRYPOINT_NOT_FOUND). See `build.rs`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use abo_core::db::open_db;
use abo_core::ipc::AppError;
use abo_lib::{run_apply_to_terminal, ApplyControl, ApplyControlRegistry};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;

/// Open a fresh migrated pool in a temp dir; the migration creates the `jobs`
/// table the wrapper writes.
async fn fresh_pool() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().expect("tempdir");
    let (pool, _outcome) = open_db(dir.path()).await.expect("open_db on empty dir");
    (dir, pool)
}

/// Insert a `running` apply job row exactly as `apply_start` does (kind `apply`),
/// returning its id.
async fn insert_running_apply(pool: &SqlitePool) -> i64 {
    let result =
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply', 'running', ?)")
            .bind("2026-07-18T00:00:00Z")
            .execute(pool)
            .await
            .expect("insert running apply job");
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

/// A registry pre-populated with a control for `job_id`, plus the shared in-process
/// flag set `true` - the state `apply_start` establishes right before it spawns the
/// walk.
fn armed_lock(job_id: i64) -> (Arc<AtomicBool>, ApplyControlRegistry) {
    let flag = Arc::new(AtomicBool::new(true));
    let mut map = HashMap::new();
    map.insert(job_id, Arc::new(ApplyControl::new()));
    (flag, Arc::new(Mutex::new(map)))
}

/// The load-bearing test (the P7 brief-mandated spawn-safety cover): a panic inside
/// the spawned walk marks the `jobs` row `failed`/`internal-panic` AND releases the
/// in-process single-writer flag AND deregisters the control - so a crashed apply
/// never strands the process-wide lock.
#[tokio::test]
async fn a_panic_in_the_spawned_walk_releases_the_single_writer_lock() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_apply(&pool).await;
    let (flag, registry) = armed_lock(job_id);

    run_apply_to_terminal(
        pool.clone(),
        job_id,
        flag.clone(),
        registry.clone(),
        async {
            panic!("simulated kill inside the apply walk");
            #[allow(unreachable_code)]
            Ok(())
        },
    )
    .await;

    // The durable lock: the row is failed/internal-panic, not stuck running.
    let (state, code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "failed",
        "a panicking apply walk must mark the job failed"
    );
    assert_eq!(
        code.as_deref(),
        Some("internal-panic"),
        "the panic arm records error_code internal-panic"
    );
    // The in-process fast guard: cleared, so a next apply is not blocked forever.
    assert!(
        !flag.load(Ordering::SeqCst),
        "the in-process single-writer flag must be cleared after a panic"
    );
    // The control registry: the job's pause/Stop control is gone.
    assert!(
        registry.lock().expect("registry").get(&job_id).is_none(),
        "the pause/Stop control must be deregistered after a panic"
    );

    pool.close().await;
}

/// A clean completion (the walk marked the row itself) still releases the flag and
/// deregisters the control on the wrapper's normal exit.
#[tokio::test]
async fn a_completed_apply_walk_releases_the_lock() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_apply(&pool).await;
    let (flag, registry) = armed_lock(job_id);

    let pool_for_work = pool.clone();
    run_apply_to_terminal(
        pool.clone(),
        job_id,
        flag.clone(),
        registry.clone(),
        async move {
            // Stand in for `walk_and_finalize` marking the row terminal itself.
            sqlx::query("UPDATE jobs SET state = 'completed' WHERE id = ?")
                .bind(job_id)
                .execute(&pool_for_work)
                .await
                .expect("mark completed");
            Ok(())
        },
    )
    .await;

    let (state, code) = job_state(&pool, job_id).await;
    assert_eq!(
        state, "completed",
        "the wrapper leaves a completed row completed"
    );
    assert_eq!(code, None);
    assert!(
        !flag.load(Ordering::SeqCst),
        "the in-process flag is released on a clean completion"
    );
    assert!(
        registry.lock().expect("registry").get(&job_id).is_none(),
        "the control is deregistered on a clean completion"
    );

    pool.close().await;
}

/// A journaled failure (`Err`) marks the row failed with the AppError's stable code
/// and releases the lock.
#[tokio::test]
async fn a_failed_apply_walk_records_its_code_and_releases_the_lock() {
    let (_dir, pool) = fresh_pool().await;
    let job_id = insert_running_apply(&pool).await;
    let (flag, registry) = armed_lock(job_id);

    let err = AppError::ApplyFailed {
        detail: "boom".to_string(),
    };
    let expected = err.code().to_string();

    run_apply_to_terminal(
        pool.clone(),
        job_id,
        flag.clone(),
        registry.clone(),
        async move { Err(err) },
    )
    .await;

    let (state, code) = job_state(&pool, job_id).await;
    assert_eq!(state, "failed");
    assert_eq!(code.as_deref(), Some(expected.as_str()));
    assert!(
        !flag.load(Ordering::SeqCst),
        "the in-process flag is released on failure"
    );
    assert!(
        registry.lock().expect("registry").get(&job_id).is_none(),
        "the control is deregistered on failure"
    );

    pool.close().await;
}
