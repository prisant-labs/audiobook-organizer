//! F-607 / F-601 apply command handler - v0.5.0 (acting) Phase 3 (executor core).
//!
//! [`apply_start`] is the executor's IPC entry point. It loads the approved plan,
//! acquires the single-writer lock (AC-8), records the apply `jobs` row, and walks
//! the plan's APPROVED operations through the [`Executor`](abo_core::exec::Executor):
//! - [`ApplyMode::DryRun`] walks a [`MemFs`](abo_core::exec::MemFs) seeded from the
//!   plan's snapshot, touching no real path (AC-2);
//! - [`ApplyMode::Real`] walks [`RealFs`](abo_core::exec::RealFs), the actual disk
//!   (this phase flips Real mode on; the operation logic - rename-first, cross-
//!   volume copy+verify+delete, TOCTOU, never-overwrite, access-denied - lives in
//!   `abo_core::exec`).
//!
//! Thin-adapter rule (same as [`super::plan`]): the product logic lives in
//! `abo-core`; this command orchestrates lock -> job row -> executor -> undo file,
//! and maps an [`ExecHalt`](abo_core::exec::ExecHalt) onto the typed error surface.
//!
//! Single-writer (AC-8): a fast in-process guard (`AppState::apply_in_flight`)
//! refuses a second concurrent apply in this process instantly; the durable
//! `running` apply `jobs` row (via [`acquire_apply_job`]) is the cross-restart
//! backstop, released by marking the job terminal here and by the startup reclaim.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use abo_core::db::plans::{get_plan, get_plan_ops};
use abo_core::exec::lock::acquire_apply_job;
use abo_core::exec::manifest::export_after_apply;
use abo_core::exec::{ApplyMode, ExecHalt, Executor, MemFs, RealFs, SeedEntry, SqliteJournal, Vfs};
use abo_core::ipc::{AppError, ApplyReport, EntryRow};
use abo_core::scan::walk::now_iso8601_utc;
use sqlx::SqlitePool;

use crate::AppState;

/// RAII guard for the in-process apply flag: clears `apply_in_flight` on every
/// exit path (success, error, or a panic that unwinds the command), so a refused
/// or crashed apply never leaves the fast guard stuck set.
struct ApplyInFlightGuard(Arc<AtomicBool>);

impl Drop for ApplyInFlightGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Start applying an approved plan (F-601/F-607).
///
/// Loads the plan, acquires the single-writer lock (AC-8), records the apply
/// `jobs` row carrying `mode`, and walks the plan's APPROVED operations through the
/// executor: a `DryRun` against a snapshot-seeded `MemFs` (AC-2), a `Real` apply
/// against `RealFs` (the actual disk). Journal-before-act (F-602, AC-10): each op's
/// intent row is flushed and committed BEFORE the filesystem call, a terminal
/// `done`/`failed` row after. An operation that fails halts the group and surfaces
/// the matching error (AC-5/6/7/9); a clean run exports the self-contained undo
/// file and re-emits the F-507 provenance report (AC-11, AC-12).
#[tauri::command]
#[specta::specta]
pub async fn apply_start(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
    mode: ApplyMode,
) -> Result<ApplyReport, AppError> {
    // In-process single-writer guard (AC-8): refuse a second concurrent apply in
    // THIS process instantly, before any DB work. The guard clears the flag on
    // every exit path (including the `?` early returns below) via Drop.
    let flag = state.apply_in_flight.clone();
    if flag
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::JobAlreadyRunning);
    }
    let _in_flight = ApplyInFlightGuard(flag);

    let pool = &state.pool;

    // The plan and its operations. A missing plan is a precise PlanNotFound, never
    // a silent empty walk.
    let plan = get_plan(pool, plan_id)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?
        .ok_or(AppError::PlanNotFound { plan_id })?;
    let ops = get_plan_ops(pool, plan_id)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?;

    // Durable single-writer lock (AC-8): refuse if an apply is already `running`,
    // else insert the `running` apply job (the lock) carrying `mode`. Released by
    // marking the job terminal below (and by the startup reclaim after a crash).
    let started_at = now_iso8601_utc();
    let job_id = acquire_apply_job(pool, mode, &started_at).await?;

    let app_data_dir = abo_core::paths::app_data_dir();
    let reports_dir = abo_core::reports::plan_export_dir(&app_data_dir, plan_id, &plan.created_at);
    let journal = SqliteJournal::new(pool.clone());

    // The walk is identical code over either backend (the Vfs seam is what makes a
    // dry run a first-class product): seed a MemFs for a dry run, use RealFs for a
    // Real apply, then run+finalize through the shared generic helper.
    match mode {
        ApplyMode::DryRun => {
            // Seed a MemFs from the plan's snapshot so the dry run walks a memory
            // tree identical to what the plan was built over, resolving nothing to a
            // real path (AC-2). MemFs is disk-inert by construction.
            let entries = abo_core::scan::get_scan_entries(pool, plan.scan_id)
                .await
                .map_err(|e| AppError::ApplyFailed {
                    detail: e.to_string(),
                })?;
            let memfs = MemFs::from_seed(&seed_from_entries(&entries));
            let executor = Executor::new(memfs, job_id, ops);
            walk_and_finalize(
                pool,
                executor,
                &journal,
                &reports_dir,
                plan_id,
                mode,
                job_id,
                &started_at,
            )
            .await
        }
        ApplyMode::Real => {
            // Real apply against the actual filesystem. The human-only gate to run a
            // Real apply against a real library is procedural (EXECUTION.md); this is
            // the RealFs executor that gate authorizes.
            let executor = Executor::new(RealFs::new(), job_id, ops);
            walk_and_finalize(
                pool,
                executor,
                &journal,
                &reports_dir,
                plan_id,
                mode,
                job_id,
                &started_at,
            )
            .await
        }
    }
}

/// Run the executor walk over `executor` and finalize the apply job: mark it
/// terminal, export the undo file on a clean run, or surface the halt on a failure.
/// Generic over the `Vfs` backend so the DryRun (`MemFs`) and Real (`RealFs`) paths
/// share one implementation. Awaited inline (never spawned over a generic journal),
/// so no `Send` bound over a generic `Journal` is needed.
#[allow(clippy::too_many_arguments)]
async fn walk_and_finalize<V: Vfs>(
    pool: &SqlitePool,
    executor: Executor<V>,
    journal: &SqliteJournal,
    reports_dir: &std::path::Path,
    plan_id: i64,
    mode: ApplyMode,
    job_id: i64,
    started_at: &str,
) -> Result<ApplyReport, AppError> {
    // journal-before-act: intent flushed and committed before each filesystem call
    // (F-602, AC-10). A failed intent flush is a hard stop (journal-write-failed).
    let outcome = match executor.run(journal, started_at).await {
        Ok(outcome) => outcome,
        Err(e) => {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    };

    // An operation failed and halted the group (AC-5/6/7/9). The journal already
    // carries a consistent `failed` row for it; mark the job failed with the halt
    // code and surface it. No undo file is exported for a halted run: the journal is
    // the durable record a v0.6.0 reconciliation reads, and an undo file over all
    // approved ops would claim moves that never happened.
    if let Some(halt) = &outcome.halt {
        let err = halt_to_error(halt);
        mark_apply_job_failed(pool, job_id, err.code()).await;
        return Err(err);
    }

    // Clean run: export the self-contained undo file (manifest) and re-emit the
    // F-507 provenance report reflecting final locations (AC-11, AC-12).
    if let Err(e) =
        export_after_apply(pool, reports_dir, mode, job_id, plan_id, executor.ops()).await
    {
        mark_apply_job_failed(pool, job_id, e.code()).await;
        return Err(e);
    }

    mark_apply_job_completed(pool, job_id).await;

    Ok(ApplyReport {
        plan_id,
        job_id,
        dry_run: mode == ApplyMode::DryRun,
        ops_walked: outcome.ops_walked as i64,
    })
}

/// Map an executor [`ExecHalt`] onto the typed IPC error taxonomy. The stable code
/// on the halt is the single source of truth; the path shown is the one that
/// matters for each hazard (the vacant/denied source, or the occupied target).
fn halt_to_error(halt: &ExecHalt) -> AppError {
    match halt.code {
        "source-vanished" => AppError::SourceVanished {
            path: halt.source_path.clone(),
        },
        "target-appeared" => AppError::TargetAppeared {
            path: halt.target_path.clone(),
        },
        "copy-verify-mismatch" => AppError::CopyVerifyMismatch {
            path: halt.source_path.clone(),
        },
        "access-denied" => AppError::AccessDenied {
            path: halt.source_path.clone(),
        },
        _ => AppError::ApplyFailed {
            detail: halt.detail.clone(),
        },
    }
}

/// Map a snapshot's [`EntryRow`]s to the executor's seed shape (path + size +
/// is_dir). A directory (or an unstatable entry) contributes size 0.
fn seed_from_entries(entries: &[EntryRow]) -> Vec<SeedEntry> {
    entries
        .iter()
        .map(|e| SeedEntry {
            path: e.path.clone(),
            size: e.size.max(0) as u64,
            is_dir: e.kind == "dir",
        })
        .collect()
}

/// Best-effort: mark the apply `jobs` row `completed`. Runs after the dry-run walk
/// already finished, so a secondary DB error here is logged and swallowed rather
/// than reported as an apply failure (the walk itself succeeded).
async fn mark_apply_job_completed(pool: &SqlitePool, job_id: i64) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query("UPDATE jobs SET state = 'completed', finished_at = ? WHERE id = ?")
        .bind(&finished_at)
        .bind(job_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        log::warn!("failed to mark apply job {job_id} completed: {e}");
    }
}

/// Best-effort: mark the apply `jobs` row `failed`, recording the stable error
/// `code`. Runs on the walk-or-export error path; the real error is already being
/// returned to the caller, so a secondary DB error here is only logged.
async fn mark_apply_job_failed(pool: &SqlitePool, job_id: i64, error_code: &str) {
    let finished_at = now_iso8601_utc();
    let result = sqlx::query(
        "UPDATE jobs SET state = 'failed', finished_at = ?, error_code = ? WHERE id = ?",
    )
    .bind(&finished_at)
    .bind(error_code)
    .bind(job_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        log::warn!("failed to mark apply job {job_id} failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The `Vfs` trait must be in scope to call its methods (exists/is_dir/metadata)
    // on the seeded `MemFs`.
    use abo_core::exec::Vfs;
    use std::path::Path;

    /// One `entries`-row shape, the wire form the snapshot read returns.
    fn entry(path: &str, kind: &str, size: i64) -> EntryRow {
        EntryRow {
            id: 0,
            scan_id: 1,
            parent_id: None,
            path: path.to_string(),
            name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
            kind: kind.to_string(),
            file_class: if kind == "dir" {
                None
            } else {
                Some("audio".to_string())
            },
            size,
            mtime: None,
            depth: 0,
        }
    }

    /// The exact `EntryRow` -> `SeedEntry` -> `MemFs` path `apply_start` uses:
    /// `kind == "dir"` maps to `is_dir`, and a negative (unstatable) size clamps to
    /// 0. The seeded `MemFs` then answers from memory with those mapped kinds/sizes.
    #[test]
    fn seed_from_entries_maps_kind_and_clamps_size() {
        let rows = vec![
            entry(r"E:\lib", "dir", 0),
            entry(r"E:\lib\Book.m4b", "file", 4096),
            // An unstatable entry can carry a negative size; it must clamp to 0.
            entry(r"E:\lib\Bad.m4b", "file", -1),
        ];

        let seed = seed_from_entries(&rows);
        assert_eq!(seed.len(), 3);
        assert!(seed[0].is_dir, "a dir entry maps to is_dir");
        assert_eq!(seed[0].size, 0);
        assert!(!seed[1].is_dir, "a file entry maps to !is_dir");
        assert_eq!(seed[1].size, 4096, "a file's size is preserved");
        assert_eq!(seed[2].size, 0, "a negative size clamps to 0");
        assert!(!seed[2].is_dir);

        // The MemFs the seed builds answers from memory with the mapped kinds.
        let memfs = MemFs::from_seed(&seed);
        assert!(memfs.is_dir(Path::new(r"E:\lib")));
        assert!(!memfs.is_dir(Path::new(r"E:\lib\Book.m4b")));
        assert!(memfs.exists(Path::new(r"E:\lib\Book.m4b")));
        assert_eq!(
            memfs.metadata(Path::new(r"E:\lib\Book.m4b")).unwrap().size,
            4096
        );
    }
}
