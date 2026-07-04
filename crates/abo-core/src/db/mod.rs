//! SQLite pool (sqlx), WAL mode, migrations, and corrupt-DB startup recovery.
//!
//! v0.1.0 spine, Phase 2. [`open_db`] is the single entry point the shell calls
//! at startup. It opens (creating if missing) `abo.db` in the given app data
//! directory in WAL mode, runs the embedded migrations, and - if an existing
//! database cannot be opened or migrated - moves the bad file aside and starts
//! fresh rather than crashing or (crucially) deleting the user's data.
//!
//! sqlx is used through its RUNTIME query API throughout (`sqlx::query`, binding
//! at runtime). There are no compile-time-checked `query!` macros, so the build
//! never needs `DATABASE_URL` or an offline query cache. `sqlx::migrate!()`
//! embeds the `migrations/` directory at compile time, so the shipped binary
//! needs no external `.sql` files and applying migrations touches no network.
//!
//! Safety invariant (this is a safety-adjacent path): recovery NEVER deletes the
//! corrupt database. The file is moved into `corrupt-backups/`, together with
//! its `-wal`/`-shm` sidecars, so nothing is ever lost.

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::AppError;

/// The database file name inside the app data directory.
const DB_FILE_NAME: &str = "abo.db";

/// The directory (under the app data dir) a corrupt database is moved into.
const CORRUPT_BACKUPS_DIR: &str = "corrupt-backups";

/// What happened while opening the database, carried alongside the ready pool.
///
/// The shell reads this once after startup: on [`Recovered`] it surfaces the
/// one-time family-safe notice (mapping to
/// [`AppError::DbCorruptRecovered`](crate::error::AppError::DbCorruptRecovered))
/// pointing at `backup_path`, so the user knows their prior data was preserved.
///
/// [`Recovered`]: DbOpenOutcome::Recovered
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbOpenOutcome {
    /// The database opened and migrated normally (fresh create or existing db).
    Normal,
    /// The existing database was unreadable; it was moved aside to `backup_path`
    /// and a fresh, migrated database is now in use. Data was preserved, never
    /// deleted.
    Recovered {
        /// Where the corrupt database was preserved (under `corrupt-backups/`).
        backup_path: PathBuf,
    },
}

/// Open the app database, creating and migrating it if needed, recovering from a
/// corrupt/unopenable existing database instead of crashing.
///
/// Steps: create `app_data_dir` if missing; open (or create) `abo.db` in WAL
/// mode; run the embedded migrations. On success returns the pool with
/// [`DbOpenOutcome::Normal`].
///
/// If opening or migrating an EXISTING database fails, the error is logged, the
/// pool (if one opened) is closed to release Windows file locks on the sidecars,
/// the `abo.db` file and any `-wal`/`-shm` sidecars are moved into
/// `corrupt-backups/abo-<timestamp>.db`, a fresh database is created and
/// migrated, and the pool is returned with [`DbOpenOutcome::Recovered`]. The
/// corrupt file is preserved, never deleted.
///
/// Returns [`AppError::DbMigrationFailed`] only if the app data directory cannot
/// be created or if recovery itself fails (a fresh database cannot be created or
/// migrated).
pub async fn open_db(app_data_dir: &Path) -> Result<(SqlitePool, DbOpenOutcome), AppError> {
    // The app data dir must exist before we touch the db file.
    std::fs::create_dir_all(app_data_dir).map_err(|e| AppError::DbMigrationFailed {
        detail: format!(
            "could not create app data directory {}: {e}",
            app_data_dir.display()
        ),
    })?;

    let db_path = app_data_dir.join(DB_FILE_NAME);

    // Try the happy path: open the pool, then migrate. EITHER step can fail on a
    // corrupt/incompatible database (a non-SQLite file fails with "file is not a
    // database"; a half-applied or checksum-mismatched migration fails in the
    // runner), and both recover the same way: move the bad file aside and start
    // fresh. So any error from this block is treated uniformly.
    //
    // When the pool opened but migration failed, hold it in `opened_pool` so it
    // can be CLOSED before the move: a held `-wal`/`-shm` keeps the file locked
    // on Windows, which would fail the rename.
    let mut opened_pool: Option<SqlitePool> = None;
    let attempt: Result<SqlitePool, String> = match open_pool(&db_path).await {
        Ok(pool) => match run_migrations(&pool).await {
            Ok(()) => Ok(pool),
            Err(e) => {
                opened_pool = Some(pool);
                Err(e.to_string())
            }
        },
        Err(e) => Err(e.to_string()),
    };

    match attempt {
        Ok(pool) => Ok((pool, DbOpenOutcome::Normal)),
        Err(cause) => recover(&db_path, app_data_dir, opened_pool, cause).await,
    }
}

/// The corrupt-DB recovery path (see [`open_db`]). Split out for readability.
async fn recover(
    db_path: &Path,
    app_data_dir: &Path,
    opened_pool: Option<SqlitePool>,
    cause: String,
) -> Result<(SqlitePool, DbOpenOutcome), AppError> {
    tracing::warn!(
        error = %cause,
        db = %db_path.display(),
        "database open/migration failed; moving the existing database aside and creating a fresh one"
    );

    // Release file handles before moving (Windows lock release).
    if let Some(pool) = opened_pool {
        pool.close().await;
    }

    let backup_dir = app_data_dir.join(CORRUPT_BACKUPS_DIR);
    let backup_path = move_db_aside(db_path, &backup_dir);
    match &backup_path {
        Some(moved) => tracing::info!(
            backup = %moved.display(),
            "previous database preserved"
        ),
        None => tracing::warn!(
            "could not move the existing database aside (it may be locked); \
             starting a fresh database under a unique name instead"
        ),
    }

    // If the move failed, the original is still in place and likely locked, so a
    // fresh pool at the SAME path would re-hit the bad file. Open the fresh db at
    // a unique sibling path in that case.
    let fresh_path = if backup_path.is_some() {
        db_path.to_path_buf()
    } else {
        unique_fresh_db_path(db_path)
    };

    let fresh_pool = open_pool(&fresh_path)
        .await
        .map_err(|e| AppError::DbMigrationFailed {
            detail: format!("recovery failed: could not create a fresh database: {e}"),
        })?;
    run_migrations(&fresh_pool)
        .await
        .map_err(|e| AppError::DbMigrationFailed {
            detail: format!("recovery failed: could not migrate the fresh database: {e}"),
        })?;

    // The corrupt file is preserved either at the backup dest (move succeeded) or
    // in place at the original path (move failed); report whichever holds it.
    let preserved = backup_path.unwrap_or_else(|| db_path.to_path_buf());
    Ok((
        fresh_pool,
        DbOpenOutcome::Recovered {
            backup_path: preserved,
        },
    ))
}

/// Open (creating if missing) the SQLite pool for `db_path`.
///
/// Configures WAL journaling (the load-bearing pragma per this phase's brief),
/// plus production-sane defaults inherited from the reference architecture: a 5s
/// busy timeout, NORMAL synchronous (safe under WAL), and enforced foreign keys.
async fn open_pool(db_path: &Path) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
}

/// Run the embedded migrations (`./migrations`) against `pool`.
///
/// The migration directory is embedded into the binary at compile time, so the
/// shipped app needs no external `.sql` files and no `DATABASE_URL`.
async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!().run(pool).await
}

/// Move a database file and its `-wal`/`-shm` sidecars into `backup_dir`, named
/// `abo-<timestamp>.db` (sidecars keep their suffix). Returns the path the `.db`
/// was moved to, or `None` if the primary `.db` could not be moved (a locked
/// file on Windows, a permissions error). Best effort on the sidecars: a failed
/// sidecar move is logged but does not fail the whole operation. The corrupt
/// file is never deleted; a failure leaves it in place for the caller to handle.
fn move_db_aside(db_path: &Path, backup_dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(backup_dir).is_err() {
        return None;
    }
    let stamp = timestamp();
    let dest = unique_backup_dest(backup_dir, &stamp);

    // Move the primary database first; if THIS fails, the whole move failed.
    if db_path.exists() {
        if let Err(e) = rename_with_retry(db_path, &dest) {
            tracing::warn!(db = %db_path.display(), error = %e, "could not move database aside");
            return None;
        }
    } else {
        // No primary file to move (e.g. an empty/absent db). Nothing to preserve,
        // but the caller can safely reuse the path, so report the dest.
        return Some(dest);
    }

    // Move the sidecars best-effort, preserving their suffix next to the backup.
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(db_path, suffix);
        if side.exists() {
            let side_dest = sidecar(&dest, suffix);
            if let Err(e) = rename_with_retry(&side, &side_dest) {
                tracing::warn!(sidecar = %side.display(), error = %e, "could not move sidecar aside");
            }
        }
    }

    Some(dest)
}

/// Rename with a short, bounded retry.
///
/// On Windows a SQLite connection that just failed to open (or was just closed)
/// can leave the database file briefly locked while its background worker
/// finalizes, so an immediate rename hits `os error 32` ("used by another
/// process"). A corrupt `-wal` sidecar in particular makes the initial open fail
/// during WAL recovery, and that failed connection is exactly this case. Retry a
/// handful of times with a short backoff before giving up (total worst case
/// under a second, on a rare startup path); the common case succeeds in the
/// first attempt or two once the worker releases the handle.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    const ATTEMPTS: u32 = 20;
    const BACKOFF: Duration = Duration::from_millis(25);
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..ATTEMPTS {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt + 1 < ATTEMPTS {
                    std::thread::sleep(BACKOFF);
                }
            }
        }
    }
    Err(last_err.expect("at least one rename attempt was made"))
}

/// A non-colliding backup destination under `backup_dir` for stamp `stamp`.
///
/// The stamp is whole-seconds, so two recoveries in the same second would derive
/// the same `abo-<stamp>.db` name. The first candidate keeps the clean
/// `abo-<stamp>.db` shape; on a collision an incrementing `-N` suffix is
/// appended until a free path is found, so consecutive backups stay distinct.
fn unique_backup_dest(backup_dir: &Path, stamp: &str) -> PathBuf {
    let first = backup_dir.join(format!("abo-{stamp}.db"));
    if !first.exists() {
        return first;
    }
    let mut n: u32 = 1;
    loop {
        let candidate = backup_dir.join(format!("abo-{stamp}-{n}.db"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Append a SQLite sidecar suffix (`-wal` / `-shm`) to a db path's file name.
fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// A unique fresh-db path next to `db_path`, used when the corrupt original could
/// not be moved aside (so reusing its name would re-open the bad file).
fn unique_fresh_db_path(db_path: &Path) -> PathBuf {
    let stamp = timestamp();
    let stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("abo");
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}-fresh-{stamp}.db"))
}

/// Current unix time in whole seconds, as a string suitable for a filename.
fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn move_db_aside_relocates_db_and_sidecars() {
        // The move helper relocates the .db and its -wal/-shm sidecars into the
        // backup dir, naming the primary abo-<timestamp>.db, and leaves nothing
        // at the original location (moved, never copied-and-kept nor deleted).
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("abo.db");
        let db_bytes: &[u8] = b"db";
        let wal_bytes: &[u8] = b"wal";
        let shm_bytes: &[u8] = b"shm";
        std::fs::write(&db, db_bytes).unwrap();
        std::fs::write(sidecar(&db, "-wal"), wal_bytes).unwrap();
        std::fs::write(sidecar(&db, "-shm"), shm_bytes).unwrap();

        let backup_dir = tmp.path().join("corrupt-backups");
        let moved = move_db_aside(&db, &backup_dir).expect("move should succeed");

        assert!(moved.exists(), "the .db was moved into the backup dir");
        assert!(
            moved.starts_with(&backup_dir),
            "moved under corrupt-backups/"
        );
        assert!(
            !db.exists(),
            "the original .db no longer sits at the db path"
        );
        // "Moved intact", not merely "not deleted": the bytes at the new
        // location must match what was written at the original location.
        assert_eq!(
            std::fs::read(&moved).unwrap(),
            db_bytes,
            "the moved .db must contain the exact original bytes"
        );

        let wal_dest = sidecar(&moved, "-wal");
        assert!(wal_dest.exists(), "the -wal sidecar moved alongside");
        assert_eq!(
            std::fs::read(&wal_dest).unwrap(),
            wal_bytes,
            "the moved -wal sidecar must contain the exact original bytes"
        );

        let shm_dest = sidecar(&moved, "-shm");
        assert!(shm_dest.exists(), "the -shm sidecar moved alongside");
        assert_eq!(
            std::fs::read(&shm_dest).unwrap(),
            shm_bytes,
            "the moved -shm sidecar must contain the exact original bytes"
        );
    }

    #[test]
    fn move_db_aside_twice_produces_distinct_paths() {
        // Two recoveries in the same whole second must not collide (the stamp is
        // whole-seconds). Assert two consecutive backups land on distinct,
        // both-present files - the corrupt data is never overwritten.
        let tmp = TempDir::new().expect("tempdir");
        let backup_dir = tmp.path().join("corrupt-backups");

        let db1 = tmp.path().join("abo.db");
        std::fs::write(&db1, b"db one").unwrap();
        let first = move_db_aside(&db1, &backup_dir).expect("first move");

        let db2 = tmp.path().join("abo.db");
        std::fs::write(&db2, b"db two").unwrap();
        let second = move_db_aside(&db2, &backup_dir).expect("second move");

        assert_ne!(first, second, "consecutive backups must not collide");
        assert!(first.exists(), "the first backup must still be present");
        assert!(
            second.exists(),
            "the second backup must be present, not overwritten"
        );
    }
}
