//! Integration tests for the database layer (v0.1.0 spine, Phase 2).
//!
//! Covers AC-5 (migrate from empty), AC-6 (idempotent re-migrate), AC-7 (WAL
//! mode), and AC-8 (corrupt-DB startup recovery, including the WAL-sidecar risk
//! from the plan's risk table). Every test runs against a temp dir; the real
//! `%LOCALAPPDATA%` is never touched.

use std::path::PathBuf;

use abo_core::db::{open_db, DbOpenOutcome};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;

/// The five spine tables the first migration must create.
const EXPECTED_TABLES: [&str; 5] = ["scans", "entries", "jobs", "settings", "activity_records"];

/// Whether a table of the given name exists in the schema.
async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    let row = sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
        .expect("query sqlite_master");
    row.is_some()
}

/// Append a SQLite sidecar suffix to a path's file name (test-local helper).
fn with_suffix(path: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[tokio::test]
async fn migrate_from_empty() {
    // AC-5: open_db on an empty temp dir creates the db with all five tables.
    let dir = TempDir::new().expect("tempdir");
    let (pool, outcome) = open_db(dir.path()).await.expect("open_db on empty dir");

    assert_eq!(
        outcome,
        DbOpenOutcome::Normal,
        "opening an empty dir is a normal create, not a recovery"
    );
    for table in EXPECTED_TABLES {
        assert!(
            table_exists(&pool, table).await,
            "expected table {table} to exist after the first migration"
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn migrate_idempotent() {
    // AC-6: open, close, reopen, migrate again - no error, no duplicate objects.
    let dir = TempDir::new().expect("tempdir");

    let (pool1, outcome1) = open_db(dir.path()).await.expect("first open");
    assert_eq!(
        outcome1,
        DbOpenOutcome::Normal,
        "first open is a normal create"
    );
    pool1.close().await;

    // Reopen the same dir: the migration is already applied, so this must be a
    // clean no-op (Normal, not Recovered) with no double-apply error.
    let (pool2, outcome2) = open_db(dir.path()).await.expect("second open");
    assert_eq!(
        outcome2,
        DbOpenOutcome::Normal,
        "reopening an already-migrated db must not trigger recovery"
    );

    for table in EXPECTED_TABLES {
        assert!(
            table_exists(&pool2, table).await,
            "table {table} must still exist after reopen"
        );
    }

    // The migration ledger records exactly one applied migration: proof the
    // migration did not double-apply.
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool2)
        .await
        .expect("count applied migrations")
        .get(0);
    assert_eq!(
        count, 1,
        "exactly one migration must be recorded, not re-applied"
    );

    pool2.close().await;
}

#[tokio::test]
async fn wal_mode_enabled() {
    // AC-7: the pool opens the db in WAL mode.
    let dir = TempDir::new().expect("tempdir");
    let (pool, _outcome) = open_db(dir.path()).await.expect("open_db");

    let row = sqlx::query("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .expect("pragma journal_mode");
    let mode: String = row.get(0);
    assert_eq!(mode.to_lowercase(), "wal", "journal_mode must be wal");

    pool.close().await;
}

#[tokio::test]
async fn corrupt_db_recovers() {
    // AC-8 (+ the WAL-sidecar risk from the plan's risk table): a garbage abo.db
    // plus a leftover -wal sidecar are recovered - open_db succeeds with a
    // Recovered outcome, both the corrupt .db and its -wal sidecar are preserved
    // under corrupt-backups/ (moved, never deleted), and the fresh db has all
    // five tables.
    //
    // Why the -wal fixture is made READ-ONLY: when SQLite opens a WAL-mode db and
    // finds a -wal beside it, it adopts that sidecar, and its sqlite3_close (on
    // the failed open) atomically releases the file lock AND deletes the -wal/-shm
    // it manages. A plain writable garbage -wal is therefore always deleted by
    // SQLite before recovery can move it - there is no instant where it is both
    // unlocked and still present. The realistic case where the recovery's
    // sidecar-move actually matters is precisely a sidecar SQLite cannot clean up
    // (a permission-locked leftover, e.g. from a crashed process or a sync agent
    // holding it). A read-only -wal reproduces exactly that: SQLite fails to
    // delete it, so it persists past close and the recovery genuinely moves it.
    // (The move logic itself is also covered deterministically, with plain files,
    // by db::tests::move_db_aside_relocates_db_and_sidecars.)
    let dir = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(dir.path()).unwrap();

    let db_path = dir.path().join("abo.db");
    let wal_path = with_suffix(&db_path, "-wal");
    std::fs::write(&db_path, b"this is definitely not a sqlite database\n")
        .expect("seed corrupt db");
    std::fs::write(&wal_path, b"garbage wal sidecar bytes").expect("seed garbage wal");
    set_readonly(&wal_path, true);

    let (pool, outcome) = open_db(dir.path())
        .await
        .expect("recovery must succeed, not return an error");

    // The outcome is Recovered and carries the backup path.
    let backup_path = match &outcome {
        DbOpenOutcome::Recovered { backup_path } => backup_path.clone(),
        DbOpenOutcome::Normal => panic!("a corrupt db must recover with a Recovered outcome"),
    };

    let backups_dir = dir.path().join("corrupt-backups");
    assert!(
        backup_path.starts_with(&backups_dir),
        "the backup must live under corrupt-backups/"
    );
    assert!(
        backup_path.exists(),
        "the corrupt .db must be preserved as a backup (moved, not deleted)"
    );

    // The WAL sidecar must be preserved alongside the backup (the sidecar risk).
    let wal_backup = with_suffix(&backup_path, "-wal");
    assert!(
        wal_backup.exists(),
        "the -wal sidecar must be preserved in corrupt-backups/"
    );
    // Clear read-only on the moved sidecar so TempDir cleanup can remove it on
    // Windows (remove_dir_all does not clear the read-only attribute).
    set_readonly(&wal_backup, false);

    // The fresh, replacement db is usable and fully migrated.
    for table in EXPECTED_TABLES {
        assert!(
            table_exists(&pool, table).await,
            "the recovered db must have table {table}"
        );
    }

    pool.close().await;
}

/// Toggle the read-only attribute on a file (test helper for the sidecar fixture).
fn set_readonly(path: &std::path::Path, readonly: bool) {
    let mut perms = std::fs::metadata(path).expect("metadata").permissions();
    perms.set_readonly(readonly);
    std::fs::set_permissions(path, perms).expect("set permissions");
}
