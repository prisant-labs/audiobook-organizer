//! F-604 after-the-fact check status + acknowledge commands - v0.5.0 (acting)
//! Phase 6 (post-apply verification + block-further-groups).
//!
//! [`job_status`] reports an apply job's lifecycle state plus its after-the-fact
//! check outcome (whether it raised an unacknowledged discrepancy that blocks
//! further FORWARD tidy-ups, AC-20). [`acknowledge_check`] clears that block by
//! appending an acknowledgement (append-only: it never rewrites the raised row),
//! then returns the refreshed status. The F-904 activity surface (P8) consumes
//! both to render the done state and the blocked-further-groups state.
//!
//! Thin-adapter rule (same as [`super::apply`]): the block logic lives in
//! `abo_core::exec::verify`; this module orchestrates the pool and the clock and
//! reads the `jobs` row.

use abo_core::exec::verify::{acknowledge_block, job_is_blocked, outstanding_block_detail};
use abo_core::ipc::{AppError, JobStatus};
use abo_core::scan::walk::now_iso8601_utc;
use sqlx::SqlitePool;

use crate::AppState;

/// The status of one apply job and its after-the-fact check (F-604): lifecycle
/// state, whether it raised an unacknowledged discrepancy blocking further
/// FORWARD tidy-ups (AC-20), and how many differences the check found.
#[tauri::command]
#[specta::specta]
pub async fn job_status(
    state: tauri::State<'_, AppState>,
    job_id: i64,
) -> Result<JobStatus, AppError> {
    build_job_status(&state.pool, job_id).await
}

/// Acknowledge a job's after-the-fact check discrepancy (F-604, AC-20): append an
/// acknowledgement (append-only; the raised record is preserved), clearing the
/// block so forward tidy-ups resume, and return the refreshed status. Undo was
/// never blocked, so this only ever re-opens forward tidying.
#[tauri::command]
#[specta::specta]
pub async fn acknowledge_check(
    state: tauri::State<'_, AppState>,
    job_id: i64,
) -> Result<JobStatus, AppError> {
    let now = now_iso8601_utc();
    acknowledge_block(&state.pool, job_id, &now).await?;
    build_job_status(&state.pool, job_id).await
}

/// Read the `jobs` row and the block state into a [`JobStatus`]. The discrepancy
/// count is parsed from the outstanding block's `detail_json` (`{"count": N}`);
/// zero when the job has no outstanding block.
async fn build_job_status(pool: &SqlitePool, job_id: i64) -> Result<JobStatus, AppError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT state, error_code FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AppError::ApplyFailed {
                detail: format!("could not read the tidy-up's status: {e}"),
            })?;
    let (state, error_code) = row.ok_or(AppError::ApplyFailed {
        detail: format!("no tidy-up job with id {job_id}"),
    })?;

    let blocks_further_tidying = job_is_blocked(pool, job_id).await?;
    let discrepancy_count = if blocks_further_tidying {
        outstanding_block_detail(pool, job_id)
            .await?
            .and_then(|d| {
                serde_json::from_str::<serde_json::Value>(&d)
                    .ok()
                    .and_then(|v| v.get("count").and_then(|c| c.as_i64()))
            })
            .unwrap_or(0)
    } else {
        0
    };

    Ok(JobStatus {
        job_id,
        state,
        error_code,
        blocks_further_tidying,
        discrepancy_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use abo_core::db::open_db;
    use abo_core::exec::record_block;
    use tempfile::TempDir;

    const NOW: &str = "2026-07-18T00:00:00Z";

    async fn seed_completed_job(pool: &SqlitePool) -> i64 {
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply','completed',?)")
            .bind(NOW)
            .execute(pool)
            .await
            .expect("insert job")
            .last_insert_rowid()
    }

    /// A clean job reports no block and a zero discrepancy count.
    #[tokio::test]
    async fn a_clean_job_status_is_unblocked() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id = seed_completed_job(&pool).await;

        let status = build_job_status(&pool, job_id).await.expect("status");
        assert_eq!(status.state, "completed");
        assert!(!status.blocks_further_tidying);
        assert_eq!(status.discrepancy_count, 0);
    }

    /// A job with a recorded discrepancy reports the block and the count, and an
    /// acknowledgement clears it (the block state is durable and append-only).
    #[tokio::test]
    async fn status_surfaces_and_acknowledge_clears_the_block() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id = seed_completed_job(&pool).await;

        record_block(&pool, job_id, NOW, r#"{"count":2,"discrepancies":[]}"#)
            .await
            .expect("record block");

        let status = build_job_status(&pool, job_id).await.expect("status");
        assert!(status.blocks_further_tidying);
        assert_eq!(status.discrepancy_count, 2);

        acknowledge_block(&pool, job_id, NOW).await.expect("ack");
        let after = build_job_status(&pool, job_id).await.expect("status");
        assert!(!after.blocks_further_tidying);
        assert_eq!(after.discrepancy_count, 0);
    }
}
