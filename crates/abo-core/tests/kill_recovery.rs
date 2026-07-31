//! F-606 kill-recovery: reconcile a journal left behind by a process that really died.
//!
//! The unit tests in `exec::reconcile` build the in-doubt state by hand - write an
//! `intent`, skip the terminal row, reconcile. That proves the reconciler reasons
//! correctly about a given database state. It cannot prove that a kill actually
//! PRODUCES that state, because the assumption and the test share the same author.
//!
//! These tests close that gap. Each spawns `kill_harness` (see its module docs),
//! which runs a real apply against a real temp library through `RealFs` and then
//! calls `std::process::abort` mid-operation: no unwinding, no `Drop`, no flush.
//! The parent then opens the same database, reconciles, and asserts on both the
//! journal and the actual on-disk tree.
//!
//! The two cases are the two the executor's journal-before-act ordering is
//! designed around:
//!
//! - **intent-then-kill (AC-4).** The intent row is committed, the rename never
//!   happens. The source is still where it was. Resume should restart THIS op.
//! - **act-then-kill (AC-5).** The rename lands, the `done` row never gets
//!   written. The file is already at its target. Resume should continue from the
//!   NEXT op, and the repaired journal must say `done`, not `failed`.
//!
//! Both are things only a real kill can set up: an in-process test that "stops
//! early" has still unwound its stack and flushed its writes.

use std::path::{Path, PathBuf};
use std::process::Command;

use abo_core::db::open_db;
use abo_core::exec::{query_in_doubt, reconcile_stranded_apply_jobs, ApplyMode, OpOutcome, RealFs};
use sqlx::SqlitePool;
use tempfile::TempDir;

const NOW: &str = "2026-07-31T00:00:00Z";
/// The harness is told to die on the middle move, so a completed op sits behind it
/// and an untouched op sits ahead of it.
const KILL_AT: usize = 1;

/// A library with three books, and a `Shelf` directory the moves target.
fn seed_library(root: &Path) {
    std::fs::create_dir_all(root.join("Shelf")).expect("shelf");
    for i in 0..3 {
        std::fs::write(root.join(format!("book{i}.m4b")), b"BOOKDATA").expect("book");
    }
}

/// Run the harness to its death, returning nothing but asserting it did die.
fn run_harness_until_it_dies(db_dir: &Path, lib_root: &Path, phase: &str) {
    let exe = env!("CARGO_BIN_EXE_kill_harness");
    let out = Command::new(exe)
        .arg(db_dir)
        .arg(lib_root)
        .arg(KILL_AT.to_string())
        .arg(phase)
        .output()
        .expect("spawn kill_harness");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("SETUP_OK"),
        "the harness never finished setting up, so nothing was killed mid-apply.\n\
         stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("KILLING"),
        "the harness reached the end of the walk without the kill firing.\n\
         stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !out.status.success(),
        "a process killed with abort() must not report success (status: {:?})",
        out.status
    );
}

/// The single stranded `running` apply job the dead process left behind.
async fn stranded_job(pool: &SqlitePool) -> i64 {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM jobs WHERE kind = 'apply' AND state = 'running'")
            .fetch_all(pool)
            .await
            .expect("stranded jobs");
    assert_eq!(
        ids.len(),
        1,
        "a killed apply leaves exactly one running job row"
    );
    ids[0]
}

/// AC-4: killed with the intent committed and the filesystem untouched.
///
/// The reconciler must see the source still in place, classify the op as never
/// started, close it `failed`, and offer to resume from THIS op.
#[tokio::test]
async fn a_kill_before_the_rename_is_recovered_as_not_started() {
    let db_dir = TempDir::new().expect("db dir");
    let lib = TempDir::new().expect("lib dir");
    let root: PathBuf = lib.path().to_path_buf();
    seed_library(&root);

    run_harness_until_it_dies(db_dir.path(), &root, "before");

    // The first move completed before the kill; the killed one did not.
    assert!(
        !root.join("book0.m4b").exists() && root.join("Shelf/book0.m4b").exists(),
        "the op before the kill should have completed on disk"
    );
    assert!(
        root.join("book1.m4b").exists() && !root.join("Shelf/book1.m4b").exists(),
        "the killed op must NOT have touched the filesystem (journal-before-act)"
    );

    let (pool, _) = open_db(db_dir.path()).await.expect("reopen db");
    let job = stranded_job(&pool).await;
    assert_eq!(
        query_in_doubt(&pool, job).await.unwrap().len(),
        1,
        "a kill leaves exactly ONE intent without a terminal row"
    );

    let result = reconcile_stranded_apply_jobs(&pool, &RealFs::new(), NOW)
        .await
        .expect("reconcile")
        .expect("the interruption is surfaced");

    assert_eq!(result.job_id, job);
    assert_eq!(result.mode, ApplyMode::Real);
    assert!(result.interrupted);
    assert_eq!(result.outcome, Some(OpOutcome::NotStarted));
    assert!(
        result.resume_offered,
        "a provably-not-started op is safe to resume from"
    );
    assert_eq!(
        result.done_count, 1,
        "one op completed before the kill; the killed one did not"
    );
    assert!(
        query_in_doubt(&pool, job).await.unwrap().is_empty(),
        "the journal is repaired: every intent now has a terminal row"
    );

    // The reconciler reads the disk but never writes it.
    assert!(
        root.join("book1.m4b").exists() && !root.join("Shelf/book1.m4b").exists(),
        "reconciliation must not move anything"
    );
    pool.close().await;
}

/// AC-5: killed AFTER the rename landed but before the `done` row was written.
///
/// This is the case a naive recovery gets wrong. The operation really did happen,
/// so the journal must be repaired with `done` (not `failed`), and resume must
/// continue from the NEXT op rather than redoing a move whose source is gone.
#[tokio::test]
async fn a_kill_after_the_rename_is_recovered_as_completed() {
    let db_dir = TempDir::new().expect("db dir");
    let lib = TempDir::new().expect("lib dir");
    let root: PathBuf = lib.path().to_path_buf();
    seed_library(&root);

    run_harness_until_it_dies(db_dir.path(), &root, "after");

    // The killed op's rename DID land, even though its terminal row never did.
    assert!(
        !root.join("book1.m4b").exists() && root.join("Shelf/book1.m4b").exists(),
        "the rename completed on disk before the process died"
    );

    let (pool, _) = open_db(db_dir.path()).await.expect("reopen db");
    let job = stranded_job(&pool).await;
    let in_doubt = query_in_doubt(&pool, job).await.unwrap();
    assert_eq!(
        in_doubt.len(),
        1,
        "the completed act left its intent without a terminal row"
    );

    let result = reconcile_stranded_apply_jobs(&pool, &RealFs::new(), NOW)
        .await
        .expect("reconcile")
        .expect("the interruption is surfaced");

    assert_eq!(result.outcome, Some(OpOutcome::Completed));
    assert!(result.resume_offered);
    assert_eq!(
        result.done_count, 2,
        "the reconciled op counts toward the resume floor, so resume starts at the NEXT op"
    );

    // The repaired row must say `done`. Recording `failed` here would invite a
    // resume that re-runs a move whose source no longer exists.
    let phases: Vec<String> =
        sqlx::query_scalar("SELECT phase FROM journal WHERE job_id = ? AND op_id = ? ORDER BY id")
            .bind(job)
            .bind(in_doubt[0].op_id)
            .fetch_all(&pool)
            .await
            .expect("phases");
    assert_eq!(
        phases,
        vec!["intent".to_string(), "done".to_string()],
        "the kill-prevented terminal row is repaired as `done`"
    );
    assert!(query_in_doubt(&pool, job).await.unwrap().is_empty());

    // Still read-only.
    assert!(
        root.join("Shelf/book1.m4b").exists(),
        "reconciliation must not move anything"
    );
    pool.close().await;
}

/// The reconciler writes AT MOST ONE journal row, even after a real kill: the ops
/// that never started must stay untouched, not be closed out en masse.
#[tokio::test]
async fn reconciliation_repairs_exactly_one_row_after_a_kill() {
    let db_dir = TempDir::new().expect("db dir");
    let lib = TempDir::new().expect("lib dir");
    let root: PathBuf = lib.path().to_path_buf();
    seed_library(&root);

    run_harness_until_it_dies(db_dir.path(), &root, "before");

    let (pool, _) = open_db(db_dir.path()).await.expect("reopen db");
    let job = stranded_job(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal WHERE job_id = ?")
        .bind(job)
        .fetch_one(&pool)
        .await
        .unwrap();

    reconcile_stranded_apply_jobs(&pool, &RealFs::new(), NOW)
        .await
        .expect("reconcile");

    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal WHERE job_id = ?")
        .bind(job)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        after,
        before + 1,
        "exactly one terminal row is written - the one the kill prevented"
    );

    // The third op was never started, so it has no rows at all and its book is
    // still sitting in the library root.
    assert!(
        root.join("book2.m4b").exists(),
        "an op the walk never reached must be left completely alone"
    );
    pool.close().await;
}
