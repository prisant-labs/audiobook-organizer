//! F-601 single-writer apply lock (AC-8): at most one apply job touches the
//! library at a time.
//!
//! The lock has two layers. The DURABLE layer is a `running` apply `jobs` row:
//! [`acquire_apply_job`] refuses to start a second apply while one is already
//! `running` ([`AppError::JobAlreadyRunning`]), and inserting the `running` row IS
//! acquiring the lock. It is released by marking that row terminal
//! (`completed`/`failed`) at the end of the run, and by
//! [`reclaim_stranded_apply_jobs`] at startup for a row a previous session left
//! `running` after a crash (there is no live writer, so a fresh session reclaims
//! the lock rather than being blocked forever). The check-and-insert runs inside a
//! `BEGIN IMMEDIATE` transaction so it is atomic even under a race: a second caller
//! either sees the first caller's `running` row or is serialized behind its write
//! lock.
//!
//! The shell adds a fast in-process guard on top (an atomic flag in `AppState`),
//! so a second `apply_start` in the SAME process is refused instantly without even
//! reaching the database; this durable layer is what makes the guarantee survive a
//! process restart. Both exist by design: the atomic is the common in-session case,
//! the `running` row is the cross-restart backstop.
//!
//! This EXTENDS the existing apply `jobs` lifecycle (kind `apply`, states
//! `running`/`completed`/`failed`) rather than adding a parallel lock table: the
//! same row that records an apply run is its lock. (Scan jobs use the same `jobs`
//! table but a different `kind`, so a stranded scan is never mistaken for an apply
//! lock and vice versa; `reclaim_stranded_apply_jobs` only ever touches
//! `kind = 'apply'`.)

use sqlx::SqlitePool;

use crate::error::AppError;

use super::ApplyMode;

/// The `jobs.kind` value an apply run records (and the single-writer lock keys
/// off). Distinct from `scan`, so a stranded scan job never blocks an apply.
pub const APPLY_JOB_KIND: &str = "apply";

/// Map a SQLite error on the lock path to the apply-bookkeeping family.
fn lock_db_error(e: sqlx::Error) -> AppError {
    AppError::ApplyFailed {
        detail: format!("apply lock bookkeeping failed: {e}"),
    }
}

/// Acquire the single-writer apply lock and record the apply `jobs` row (AC-8).
///
/// Refuses with [`AppError::JobAlreadyRunning`] if any apply job is already
/// `running`; otherwise inserts a `running` apply job carrying `mode` and returns
/// its id. The check and the insert run in one `BEGIN IMMEDIATE` transaction, so
/// two concurrent acquirers cannot both observe "no running apply" and both
/// insert - the second is serialized behind the first's reserved write lock and
/// then sees its `running` row.
///
/// The returned id is the lock handle: release it by marking the row
/// `completed`/`failed` when the run ends (see the `apply_start` command).
pub async fn acquire_apply_job(
    pool: &SqlitePool,
    mode: ApplyMode,
    started_at: &str,
) -> Result<i64, AppError> {
    let mut conn = pool.acquire().await.map_err(lock_db_error)?;

    // BEGIN IMMEDIATE takes the reserved write lock now, so the check+insert is
    // atomic against another acquirer (it would block here, or see the row).
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *conn)
        .await
        .map_err(lock_db_error)?;

    let running: i64 = match sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE kind = 'apply' AND state = 'running'",
    )
    .fetch_one(&mut *conn)
    .await
    {
        Ok(n) => n,
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(lock_db_error(e));
        }
    };
    if running > 0 {
        let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
        return Err(AppError::JobAlreadyRunning);
    }

    let id = match sqlx::query(
        "INSERT INTO jobs (kind, state, started_at, mode) VALUES ('apply', 'running', ?, ?)",
    )
    .bind(started_at)
    .bind(mode.as_str())
    .execute(&mut *conn)
    .await
    {
        Ok(r) => r.last_insert_rowid(),
        Err(e) => {
            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
            return Err(lock_db_error(e));
        }
    };

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .map_err(lock_db_error)?;
    Ok(id)
}

/// Release stranded apply locks at startup (AC-8, crash-detected release): mark any
/// `running` apply job `failed` with error code `interrupted`, so a `running` row a
/// previous session left behind after a crash never blocks a fresh apply. Returns
/// how many rows were reclaimed. Only ever touches `kind = 'apply'`, so a scan
/// job's lifecycle is untouched. Safe to call unconditionally at startup: it is a
/// no-op when nothing is stranded.
pub async fn reclaim_stranded_apply_jobs(pool: &SqlitePool, now: &str) -> Result<u64, AppError> {
    let result = sqlx::query(
        "UPDATE jobs SET state = 'failed', finished_at = ?, error_code = 'interrupted' \
         WHERE kind = 'apply' AND state = 'running'",
    )
    .bind(now)
    .execute(pool)
    .await
    .map_err(lock_db_error)?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use sqlx::Row;
    use tempfile::TempDir;

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    async fn state_of(pool: &SqlitePool, job_id: i64) -> (String, Option<String>) {
        let row = sqlx::query("SELECT state, error_code FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(pool)
            .await
            .expect("job row");
        (
            row.get::<String, _>("state"),
            row.get::<Option<String>, _>("error_code"),
        )
    }

    /// AC-8: the first acquire records a running apply job (the lock); a SECOND
    /// acquire while it is still running is refused immediately with
    /// `job-already-running`.
    #[tokio::test]
    async fn second_acquire_while_running_is_refused() {
        let (_dir, pool) = fresh_pool().await;
        let first = acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T00:00:00Z")
            .await
            .expect("first acquire holds the lock");
        assert_eq!(state_of(&pool, first).await.0, "running");

        let err = acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T00:00:01Z")
            .await
            .expect_err("a second apply while one runs is refused");
        assert_eq!(err.code(), "job-already-running");
    }

    /// AC-8: the lock is RELEASED when the running apply job is marked terminal, so
    /// a later apply acquires cleanly (the single-writer lock is not permanent).
    #[tokio::test]
    async fn releasing_the_lock_lets_a_later_apply_acquire() {
        let (_dir, pool) = fresh_pool().await;
        let first = acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T00:00:00Z")
            .await
            .expect("first acquire");
        // Release: mark the running apply job terminal (what the command does on
        // completion / failure).
        sqlx::query("UPDATE jobs SET state = 'completed', finished_at = ? WHERE id = ?")
            .bind("2026-07-18T00:00:02Z")
            .bind(first)
            .execute(&pool)
            .await
            .expect("release");

        let second = acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T00:00:03Z")
            .await
            .expect("a later apply acquires once the lock is released");
        assert_ne!(first, second);
        assert_eq!(state_of(&pool, second).await.0, "running");
    }

    /// AC-8 (crash-detected release): a `running` apply job a prior session left
    /// behind is reclaimed at startup (marked failed/interrupted), so a fresh apply
    /// is not blocked by a dead lock; a scan job is untouched.
    #[tokio::test]
    async fn reclaim_frees_a_stranded_apply_lock_without_touching_scans() {
        let (_dir, pool) = fresh_pool().await;
        // A stranded apply lock (as if a previous run was killed mid-apply).
        let stranded = acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T00:00:00Z")
            .await
            .expect("stranded acquire");
        // A running scan job from the same era, which must NOT be reclaimed here.
        let scan =
            sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('scan','running',?)")
                .bind("2026-07-18T00:00:00Z")
                .execute(&pool)
                .await
                .expect("scan job")
                .last_insert_rowid();

        let reclaimed = reclaim_stranded_apply_jobs(&pool, "2026-07-18T01:00:00Z")
            .await
            .expect("reclaim");
        assert_eq!(
            reclaimed, 1,
            "exactly the one stranded apply lock is reclaimed"
        );
        let (state, code) = state_of(&pool, stranded).await;
        assert_eq!(state, "failed");
        assert_eq!(code.as_deref(), Some("interrupted"));
        // The scan job is left exactly as it was.
        assert_eq!(state_of(&pool, scan).await.0, "running");

        // With the stranded lock reclaimed, a fresh apply acquires cleanly.
        acquire_apply_job(&pool, ApplyMode::Real, "2026-07-18T02:00:00Z")
            .await
            .expect("a fresh apply acquires after reclaim");
    }
}
