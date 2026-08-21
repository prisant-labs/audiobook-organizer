//! F-702 / F-905 duplicate verification commands (v0.6.0 P5).
//!
//! [`dupes_hash_verify`] is the production caller the hash engine never had.
//! Phase 2's plan claimed this command was "already in the command surface"; it
//! never was, and `verify_groups` sat complete and unreachable for two releases
//! as a result. Everything here is orchestration: the detect / persist / hash
//! chain lives in `abo_core::dupes::job`, and this module owns only the `jobs`
//! row, the cancel flag, and the events, exactly like [`super::scan_start`].
//!
//! # Why it is a job rather than a call that returns an answer
//!
//! `AC-11` requires progress events and cancellation at safe boundaries, and
//! FD-49 measured why: the hashing code runs at 2,765 MB/s while the library's
//! drive delivers 42 to 80, so on a real library this waits on the disk for
//! minutes. A command that blocked until it finished would freeze the surface
//! that is meant to be showing the progress.

use abo_core::db::dupes::clear_confirmation;
use abo_core::dupes::{
    confirm_resolution_gated, review_for_scan, review_view_for_scan, verify_scan_duplicates,
    ConfirmedResolution, FsContentSource,
};
use abo_core::ipc::{AppError, DuplicatesReviewView, ExportedFile, JobStarted};
use abo_core::job::{CancelFlag, JobContext, ProgressUpdate};
use abo_core::paths::app_data_dir;
use abo_core::reports::{duplicates_export_dir, write_duplicates_csv};
use abo_core::scan::walk::now_iso8601_utc;
use sqlx::SqlitePool;

use crate::commands::{run_job_to_terminal, JobEnd};
use crate::events::{emit_job_completed, emit_job_failed, emit_job_progress};
use crate::AppState;

/// Start the duplicate verification job for one scan (F-702, AC-10, AC-11).
///
/// Returns as soon as the `jobs` row exists, carrying the id the caller listens
/// for. The work runs in the background and reports `job:progress` per file,
/// then `job:completed` or `job:failed`. Cancel it with the existing
/// [`super::scan_cancel`], which flips the flag this registers: the registry is
/// keyed by `jobs.id` and a job id is unique across every kind, so one Stop
/// control serves both without either knowing about the other.
///
/// Hashing is candidates-only (`AC-10`): the job hashes the duplicate groups
/// detected for this scan and nothing else. There is no hash-everything path
/// here or anywhere below it.
#[tauri::command]
#[specta::specta]
pub async fn dupes_hash_verify(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<JobStarted, AppError> {
    let started_at = now_iso8601_utc();
    let job_id = insert_verify_job(&state.pool, &started_at).await?;

    let cancel = CancelFlag::new();
    state
        .jobs
        .lock()
        .expect("jobs registry mutex poisoned")
        .insert(job_id, cancel.clone());

    // Everything the spawned task needs is owned: the task outlives this command
    // and must borrow nothing from State.
    let pool = state.pool.clone();
    let verify_pool = pool.clone();
    let handle_completed = app.clone();
    let handle_failed = app.clone();
    let handle_progress = app.clone();
    let jobs_registry = state.jobs.clone();

    tauri::async_runtime::spawn(async move {
        // Unthrottled, unlike the scan's progress: the scan reports per entry and
        // can walk hundreds of thousands of them, while this reports once per
        // FILE HASHED, and a file that took a second of disk is worth an event.
        let progress = std::sync::Arc::new(move |update: ProgressUpdate| {
            emit_job_progress(
                &handle_progress,
                job_id,
                update.done as i64,
                update.total_estimate.map(|t| t as i64),
                &update.current_label,
            );
        });
        let ctx = JobContext::new(cancel, progress);

        run_job_to_terminal(
            pool,
            job_id,
            async move {
                let now = now_iso8601_utc();
                verify_scan_duplicates(&verify_pool, &FsContentSource, scan_id, &now, &ctx)
                    .await
                    .map(|outcome| {
                        // A cancelled pass is NOT a failure: it kept every hash it
                        // finished, and the next run resumes by finding fewer
                        // unhashed members. Reporting it as failed would tell the
                        // user their work was lost when it was not.
                        if outcome.cancelled {
                            JobEnd::Cancelled(scan_id)
                        } else {
                            JobEnd::Completed(scan_id)
                        }
                    })
            },
            move |scan_id| emit_job_completed(&handle_completed, job_id, scan_id),
            || {},
            move |code| emit_job_failed(&handle_failed, job_id, code),
        )
        .await;

        jobs_registry
            .lock()
            .expect("jobs registry mutex poisoned")
            .remove(&job_id);
    });

    Ok(JobStarted { job_id })
}

/// The duplicates surface's read model for one scan (F-905, AC-17 to AC-19).
///
/// Cheap and filesystem-free: it re-detects from the snapshot and lays whatever
/// has been hashed over the result. Safe to call before anything has ever been
/// verified, which is the ordinary first visit: every copy simply reads "not
/// checked yet", which is true.
#[tauri::command]
#[specta::specta]
pub async fn dupes_review(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<DuplicatesReviewView, AppError> {
    review_view_for_scan(&state.pool, scan_id, std::path::MAIN_SEPARATOR).await
}

/// Write the F-703 duplicates CSV into the Reports folder (AC-20) and return
/// where it landed.
///
/// The file is built from the SAME review the surface renders, so the export and
/// the screen cannot disagree: `AC-20`'s actual bar is that they match, and the
/// way that breaks is one of them quietly counting a different population.
///
/// Writes only into the Reports folder, never the library. The destination is a
/// pure function of the scan, so exporting twice overwrites one file rather than
/// growing a pile of near-identical folders.
#[tauri::command]
#[specta::specta]
pub async fn dupes_export_csv(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<ExportedFile, AppError> {
    let review = review_for_scan(&state.pool, scan_id, std::path::MAIN_SEPARATOR).await?;
    // The scan's OWN timestamp, never the wall clock at export time. Reading the
    // clock here would contradict this function's own promise and grow a new
    // folder on every export, which is the behaviour `plan_export_dir`'s contract
    // exists to forbid.
    let started_at = scan_started_at(&state.pool, scan_id).await?;
    let dir = duplicates_export_dir(&app_data_dir(), scan_id, &started_at);
    let path = write_duplicates_csv(&dir, &review.to_csv()).map_err(|e| {
        AppError::DuplicateExportFailed {
            detail: format!("could not write the duplicates export: {e}"),
        }
    })?;
    Ok(ExportedFile {
        path: path.to_string_lossy().to_string(),
    })
}

/// Record the user's decision for one duplicate group (AC-24, AC-30).
///
/// `keeper_entry_id` and `loser_entry_ids` are `entries.id` values from
/// `scan_id`'s snapshot, and the confirmation is stored against that scan. That
/// is what stops a decision outliving the thing it was made about: ids are
/// per-snapshot, FD-39 re-plans from a fresh scan after an interruption, and a
/// confirmation carried across a re-scan would archive whatever file happens to
/// hold that id next.
///
/// Re-confirming a group REPLACES the previous answer rather than adding one.
/// Nothing is archived by this call: it records a decision, and the Archive
/// operations appear the next time a plan is built from this scan.
///
/// # AC-12's gate is enforced HERE, not by the screen that calls this
///
/// A resolution is accepted only when the group's copies are proven identical
/// (every one hashed, all the hashes agreeing), or when `unverified_override` is
/// explicitly true. Otherwise it is refused with
/// [`AppError::DuplicateNotVerified`].
///
/// Putting the check in the caller would make the guarantee a convention: it
/// would hold for exactly as long as every present and future caller remembered
/// it, which is the same shape as a run mode taken as a parameter from whoever
/// calls. This is the thing standing between the app and a file it cannot get
/// back, so it refuses on its own behalf.
///
/// The override is RECORDED on the confirmation rather than inferred later.
/// Hashes can arrive after the fact, so "were the copies verified when this was
/// decided?" is unanswerable from the hashes alone an hour afterwards.
#[tauri::command]
#[specta::specta]
pub async fn dupes_confirm(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
    method: String,
    group_key: String,
    keeper_entry_id: i64,
    loser_entry_ids: Vec<i64>,
    unverified_override: bool,
) -> Result<(), AppError> {
    let resolution = ConfirmedResolution {
        keeper: keeper_entry_id as usize,
        losers: loser_entry_ids.into_iter().map(|l| l as usize).collect(),
    };
    confirm_resolution_gated(
        &state.pool,
        scan_id,
        &method,
        &group_key,
        &resolution,
        unverified_override,
        &now_iso8601_utc(),
    )
    .await
}

/// Withdraw a decision for one duplicate group, putting it back to undecided.
///
/// The losers go with it. A confirmation without its losers is not a record of
/// anything, and a half-withdrawn decision is the shape that archives a file
/// nobody meant to archive.
#[tauri::command]
#[specta::specta]
pub async fn dupes_clear_confirmation(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
    method: String,
    group_key: String,
) -> Result<(), AppError> {
    clear_confirmation(&state.pool, scan_id, &method, &group_key)
        .await
        .map_err(|e| AppError::DuplicateConfirmFailed {
            detail: format!("could not withdraw the decision: {e}"),
        })
}

/// The scan's own `started_at`, so an export destination is a pure function of
/// the scan rather than of when the button was pressed.
async fn scan_started_at(pool: &SqlitePool, scan_id: i64) -> Result<String, AppError> {
    sqlx::query_scalar::<_, String>("SELECT started_at FROM scans WHERE id = ?")
        .bind(scan_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::DuplicateExportFailed {
            detail: format!("could not read the scan: {e}"),
        })?
        .ok_or_else(|| AppError::DuplicateExportFailed {
            detail: format!("scan {scan_id} does not exist"),
        })
}

/// Insert the initial `running` verification `jobs` row and return its id.
///
/// `kind` is its own value rather than reusing `'scan'`: the two are different
/// work with different failure modes, and a history that cannot tell them apart
/// cannot explain what the app was doing.
async fn insert_verify_job(pool: &SqlitePool, started_at: &str) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO jobs (kind, state, started_at) VALUES ('dupes-verify', 'running', ?)",
    )
    .bind(started_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::DuplicateVerifyFailed {
        detail: format!("could not record verification job: {e}"),
    })?;
    Ok(result.last_insert_rowid())
}
