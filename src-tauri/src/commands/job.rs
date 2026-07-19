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

use crate::commands::apply::ApplyControlRegistry;
use crate::AppState;

/// The status of one apply job and its after-the-fact check (F-604): lifecycle
/// state, whether it raised an unacknowledged discrepancy blocking further
/// FORWARD tidy-ups (AC-20), how many differences the check found, and whether
/// the job is currently paused at an operation boundary (P8 prelude 0a).
#[tauri::command]
#[specta::specta]
pub async fn job_status(
    state: tauri::State<'_, AppState>,
    job_id: i64,
) -> Result<JobStatus, AppError> {
    build_job_status(&state.pool, job_id, &state.apply_controls).await
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
    build_job_status(&state.pool, job_id, &state.apply_controls).await
}

/// Pause a running apply job between books (F-608, FD-02, AC-24).
///
/// Sets the job's pause flag; the executor stops BEFORE its next operation and
/// parks there (never mid-operation). Pausing is metadata-only in-memory state -
/// NEVER a journal event (AC-25) - and the paused apply keeps holding the
/// single-writer lock (it is still THE apply). Errors plainly with
/// [`AppError::NothingToPause`] if no tidy-up is in progress to pause (already
/// finished, never started, or an unknown id).
///
/// Synchronous: it only flips an in-memory flag in managed state, so it needs no
/// async runtime and returns instantly without waiting for the walk to park.
#[tauri::command]
#[specta::specta]
pub fn job_pause(state: tauri::State<'_, AppState>, job_id: i64) -> Result<(), AppError> {
    pause_apply(&state.apply_controls, job_id)
}

/// Resume a paused apply job (F-608, FD-02, AC-24): continue from the next
/// operation. Errors plainly with [`AppError::NothingToResume`] if the tidy-up is
/// not currently paused (running normally, already finished, or an unknown id).
///
/// Synchronous, like [`job_pause`].
#[tauri::command]
#[specta::specta]
pub fn job_resume(state: tauri::State<'_, AppState>, job_id: i64) -> Result<(), AppError> {
    resume_apply(&state.apply_controls, job_id)
}

/// Request a cooperative Stop of a running apply job (F-104, FD-02, AC-26).
///
/// Flips the job's Stop flag; the executor cancels at its next safe operation
/// boundary, leaving a consistent journal and a coherent partial state (the job
/// ends in the distinct `stopped` terminal state, with no undo file for the
/// partial forward job). Returns `true` if a running apply was found and
/// signalled, `false` if none exists (already finished, never started, or unknown
/// id) - a clear no-op status, not an error, exactly like the scan Stop
/// (`scan_cancel`).
///
/// Synchronous: it only flips an in-memory flag and wakes any parked walk.
#[tauri::command]
#[specta::specta]
pub fn job_stop(state: tauri::State<'_, AppState>, job_id: i64) -> bool {
    stop_apply(&state.apply_controls, job_id)
}

/// Pause the registered control for `job_id` (the [`job_pause`] core logic, factored
/// out so it is unit-testable without a Tauri `State`). Errors plainly when no
/// control is registered (no apply in progress).
fn pause_apply(registry: &ApplyControlRegistry, job_id: i64) -> Result<(), AppError> {
    let reg = registry.lock().expect("apply control registry poisoned");
    match reg.get(&job_id) {
        Some(control) => {
            control.pause();
            Ok(())
        }
        None => Err(AppError::NothingToPause),
    }
}

/// Resume the registered control for `job_id` (the [`job_resume`] core logic).
/// Errors plainly unless a control is registered AND currently paused, so resuming
/// a never-paused (or unknown) job is a plain error, not a silent no-op.
fn resume_apply(registry: &ApplyControlRegistry, job_id: i64) -> Result<(), AppError> {
    let reg = registry.lock().expect("apply control registry poisoned");
    match reg.get(&job_id) {
        Some(control) if control.is_paused() => {
            control.resume();
            Ok(())
        }
        _ => Err(AppError::NothingToResume),
    }
}

/// Signal a cooperative Stop on the registered control for `job_id` (the
/// [`job_stop`] core logic). Returns whether a running apply was found (a Stop of a
/// not-running job is a harmless no-op, like the scan Stop).
fn stop_apply(registry: &ApplyControlRegistry, job_id: i64) -> bool {
    let reg = registry.lock().expect("apply control registry poisoned");
    match reg.get(&job_id) {
        Some(control) => {
            control.stop();
            true
        }
        None => false,
    }
}

/// Read the `jobs` row, the block state, and the live pause state into a
/// [`JobStatus`]. The discrepancy count is parsed from the outstanding block's
/// `detail_json` (`{"count": N}`); zero when the job has no outstanding block.
/// The `paused` field is sourced from the in-memory control registry (P8 prelude
/// 0a): `true` when the job is registered AND currently paused; `false` for all
/// terminal jobs (they have left the registry by the time this runs).
async fn build_job_status(
    pool: &SqlitePool,
    job_id: i64,
    registry: &ApplyControlRegistry,
) -> Result<JobStatus, AppError> {
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

    // P8 prelude 0a: derive `paused` from the in-memory registry. Terminal jobs
    // are removed from the registry by `ApplyControlGuard::drop`, so any job
    // found here is still running; jobs not found are terminal (paused = false).
    let paused = registry
        .lock()
        .expect("apply control registry poisoned")
        .get(&job_id)
        .map(|c| c.is_paused())
        .unwrap_or(false);

    // P8 IMPORTANT 4 (backfill): seed the activity surface's progress counters from
    // the DURABLE journal so a fast dry-run that finished before the UI attached its
    // `apply:op-executed` listeners still shows the true "X of Y books", not
    // "0 of 0". `done_count` is the committed `done` rows; `total` is the plan's
    // approved-op count, found via any journal row's `op_id` (the `jobs` row carries
    // no `plan_id`). Both read 0 only before the job journals its first op, which the
    // live events then fill in.
    let done_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM journal WHERE job_id = ? AND phase = 'done'")
            .bind(job_id)
            .fetch_one(pool)
            .await
            .map_err(|e| AppError::ApplyFailed {
                detail: format!("could not read the tidy-up's progress: {e}"),
            })?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM plan_ops \
         WHERE approval = 'approved' AND plan_id = ( \
             SELECT po.plan_id FROM plan_ops po \
             JOIN journal j ON j.op_id = po.id \
             WHERE j.job_id = ? LIMIT 1)",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not read the tidy-up's total: {e}"),
    })?;

    Ok(JobStatus {
        job_id,
        state,
        error_code,
        blocks_further_tidying,
        discrepancy_count,
        paused,
        done_count,
        total,
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

    use crate::commands::apply::ApplyControl;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// An empty control registry for tests that do not need a running apply.
    fn empty_registry() -> ApplyControlRegistry {
        Arc::new(Mutex::new(HashMap::new()))
    }

    /// A clean job reports no block and a zero discrepancy count.
    #[tokio::test]
    async fn a_clean_job_status_is_unblocked() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id = seed_completed_job(&pool).await;

        let status = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert_eq!(status.state, "completed");
        assert!(!status.blocks_further_tidying);
        assert_eq!(status.discrepancy_count, 0);
        assert!(!status.paused, "a terminal job is never paused");
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

        let status = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert!(status.blocks_further_tidying);
        assert_eq!(status.discrepancy_count, 2);

        acknowledge_block(&pool, job_id, NOW).await.expect("ack");
        let after = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert!(!after.blocks_further_tidying);
        assert_eq!(after.discrepancy_count, 0);
    }

    /// P8 prelude 0a: `paused` is derived from the live control registry, not the
    /// `jobs` row. A job registered with a paused control reports `paused: true`;
    /// the same job with an un-paused control (or not in the registry) reports false.
    #[tokio::test]
    async fn job_status_reflects_pause_from_registry() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id = seed_completed_job(&pool).await;

        // Not in registry -> paused: false.
        let s = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert!(!s.paused, "absent from registry -> not paused");

        // In registry, not paused -> paused: false.
        let control = Arc::new(ApplyControl::new());
        let registry: ApplyControlRegistry = Arc::new(Mutex::new({
            let mut m = HashMap::new();
            m.insert(job_id, control.clone());
            m
        }));
        let s = build_job_status(&pool, job_id, &registry)
            .await
            .expect("status");
        assert!(!s.paused, "registered but not paused -> paused: false");

        // Pause the control -> paused: true.
        control.pause();
        let s = build_job_status(&pool, job_id, &registry)
            .await
            .expect("status");
        assert!(s.paused, "paused control -> paused: true");
    }

    /// P8 IMPORTANT 4 (backfill): `done_count` and `total` are read from the durable
    /// journal + plan, so a job whose events were missed still reports the true
    /// progress. A job with two approved ops, one of which has committed its `done`
    /// row, reports `done_count = 1` and `total = 2`; a job with no journal reads 0.
    #[tokio::test]
    async fn job_status_backfills_progress_from_the_journal() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id = seed_completed_job(&pool).await;

        // No journal yet: both counters read 0 (the pre-first-op transient).
        let empty = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert_eq!(empty.done_count, 0);
        assert_eq!(empty.total, 0);

        // A scan + ruleset + plan so the plan_ops FK resolves, then two approved ops.
        let scan_id =
            sqlx::query("INSERT INTO scans (source, root_path, started_at, status) VALUES ('live','E:/lib',?,'completed')")
                .bind(NOW)
                .execute(&pool)
                .await
                .expect("scan")
                .last_insert_rowid();
        let ruleset_id = abo_core::db::rulesets::insert_ruleset(
            &pool,
            &abo_core::db::rulesets::NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            NOW,
        )
        .await
        .expect("ruleset");
        let plan_id = sqlx::query(
            "INSERT INTO plans (scan_id, ruleset_id, created_at, status) VALUES (?, ?, ?, 'draft')",
        )
        .bind(scan_id)
        .bind(ruleset_id)
        .bind(NOW)
        .execute(&pool)
        .await
        .expect("plan")
        .last_insert_rowid();
        // Two approved ops and one excluded op (excluded must NOT count toward total).
        let mut op_ids = Vec::new();
        for (seq, approval) in [(0, "approved"), (1, "approved"), (2, "excluded")] {
            let id = sqlx::query(
                "INSERT INTO plan_ops (plan_id, seq, op_group, kind, source_path, target_path, \
                 rationale, rule_id, confidence, approval) \
                 VALUES (?, ?, 'loose-root-books', 'move', 'E:/lib/a.m4b', 'E:/lib/A/a.m4b', \
                 'r', 'rule', 'high', ?)",
            )
            .bind(plan_id)
            .bind(seq)
            .bind(approval)
            .execute(&pool)
            .await
            .expect("op")
            .last_insert_rowid();
            op_ids.push(id);
        }

        // One committed `done` row on the first approved op (intent + done).
        for (op_id, phase) in [(op_ids[0], "intent"), (op_ids[0], "done")] {
            sqlx::query("INSERT INTO journal (job_id, seq, op_id, phase, at) VALUES (?, 0, ?, ?, ?)")
                .bind(job_id)
                .bind(op_id)
                .bind(phase)
                .bind(NOW)
                .execute(&pool)
                .await
                .expect("journal row");
        }

        let status = build_job_status(&pool, job_id, &empty_registry())
            .await
            .expect("status");
        assert_eq!(status.done_count, 1, "one op has a committed done row");
        assert_eq!(status.total, 2, "two approved ops; the excluded op is not counted");
    }

    // ---- Phase 7: pause/resume/stop command logic (AC-24, AC-26) ----

    /// A registry holding one control for `job_id`, as `apply_start` would register
    /// while an apply runs.
    fn registry_with(job_id: i64) -> (ApplyControlRegistry, Arc<ApplyControl>) {
        let control = Arc::new(ApplyControl::new());
        let mut map = HashMap::new();
        map.insert(job_id, control.clone());
        (Arc::new(Mutex::new(map)), control)
    }

    /// AC-24: pausing a running apply sets its pause flag; resuming a paused apply
    /// clears it. The control the executor consults is the same instance the
    /// commands flip.
    #[test]
    fn pause_then_resume_flips_the_running_apply_control() {
        let (registry, control) = registry_with(42);
        assert!(!control.is_paused(), "a fresh control is not paused");

        pause_apply(&registry, 42).expect("pausing a running apply succeeds");
        assert!(
            control.is_paused(),
            "pause set the flag the executor parks on"
        );

        resume_apply(&registry, 42).expect("resuming a paused apply succeeds");
        assert!(!control.is_paused(), "resume cleared the pause flag");
    }

    /// AC-24 (brief step 4): pausing a job that is not running errors plainly, not a
    /// silent no-op.
    #[test]
    fn pausing_a_job_that_is_not_running_errors_plainly() {
        let empty: ApplyControlRegistry = Arc::new(Mutex::new(HashMap::new()));
        let err = pause_apply(&empty, 99).expect_err("no apply is running to pause");
        assert_eq!(err.code(), "nothing-to-pause");
    }

    /// AC-24 (brief step 4): resuming a job that was never paused errors plainly -
    /// both when the job is running-but-not-paused and when no such job exists.
    #[test]
    fn resuming_a_never_paused_job_errors_plainly() {
        // Running but not paused.
        let (registry, _control) = registry_with(7);
        let err =
            resume_apply(&registry, 7).expect_err("a running, un-paused apply is not resumable");
        assert_eq!(err.code(), "nothing-to-resume");

        // No such running apply at all.
        let empty: ApplyControlRegistry = Arc::new(Mutex::new(HashMap::new()));
        let err = resume_apply(&empty, 7).expect_err("no apply is running to resume");
        assert_eq!(err.code(), "nothing-to-resume");
    }

    /// AC-26: Stop signals the running apply's control (cooperative cancel) and
    /// reports it was signalled; a Stop of a not-running job is a harmless `false`
    /// no-op (mirroring the scan Stop), never an error.
    #[test]
    fn stop_signals_a_running_apply_and_noops_otherwise() {
        let (registry, control) = registry_with(5);
        assert!(!control.is_stopping());

        assert!(
            stop_apply(&registry, 5),
            "a running apply is signalled to stop"
        );
        assert!(
            control.is_stopping(),
            "Stop set the cancel flag the executor observes at its next boundary"
        );

        let empty: ApplyControlRegistry = Arc::new(Mutex::new(HashMap::new()));
        assert!(
            !stop_apply(&empty, 5),
            "stopping a not-running apply is a false no-op, not an error"
        );
    }
}
