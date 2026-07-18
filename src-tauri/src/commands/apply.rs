//! F-607 (dry-run harness) apply command handler - v0.5.0 (acting) Phase 1 seam.
//!
//! [`apply_start`] is the executor's IPC entry point. This phase wires only the
//! DRY-RUN walk: it seeds a [`MemFs`](abo_core::exec::MemFs) from the plan's
//! snapshot and runs the [`Executor`](abo_core::exec::Executor) over the approved
//! operations, proving the `Vfs` seam end to end without touching disk (AC-1,
//! AC-2). A `Real` apply is deliberately refused with `apply-not-supported`
//! (D-09 safety invariant): the executor's operation logic is a skeleton this
//! phase, so no intermediate build can half-apply.
//!
//! Thin-adapter rule (same as [`super::plan`]): pull the pool out of managed
//! state, call into `abo-core`, return the typed result verbatim. No product
//! logic lives here. The real event-driven apply + activity surface (F-904), the
//! single-writer lock (AC-8), and the journal writes (F-602) are later phases;
//! this command records the apply `jobs` row (kind `apply`) so those phases have
//! a real job to hang the journal and lock off.

use abo_core::db::plans::{get_plan, get_plan_ops};
use abo_core::exec::manifest::export_after_apply;
use abo_core::exec::{ApplyMode, Executor, MemFs, SeedEntry, SqliteJournal};
use abo_core::ipc::{AppError, ApplyReport, EntryRow};
use abo_core::scan::walk::now_iso8601_utc;
use sqlx::SqlitePool;

use crate::AppState;

/// Start applying an approved plan (F-607 seam).
///
/// v0.5.0 Phase 1 supports [`ApplyMode::DryRun`] only: it loads the plan, seeds a
/// `MemFs` from the plan's snapshot (path + size + is_dir), records an apply
/// `jobs` row, and walks the plan's APPROVED operations through the executor
/// against memory, touching no real path (AC-2). [`ApplyMode::Real`] is refused
/// immediately with [`AppError::ApplyNotSupported`] - before any database read or
/// executor construction - so no filesystem work can begin while the operation
/// logic is a skeleton (D-09).
#[tauri::command]
#[specta::specta]
pub async fn apply_start(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
    mode: ApplyMode,
) -> Result<ApplyReport, AppError> {
    // D-09 safety invariant: a Real apply performs NO filesystem work in this
    // build. Refuse it FIRST, before any executor is constructed or any row is
    // read, so no intermediate build can walk (let alone mutate) the real
    // filesystem.
    if mode == ApplyMode::Real {
        return Err(AppError::ApplyNotSupported);
    }

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

    // Seed a MemFs from the plan's snapshot so the dry run walks a memory tree
    // identical to what the plan was built over, and nothing resolves to a real
    // path (AC-2). MemFs is disk-inert by construction. A snapshot-read failure is
    // mapped to ApplyFailed (not the scan family) so every apply failure carries
    // consistent apply-family provenance, matching get_plan/get_plan_ops above.
    let entries = abo_core::scan::get_scan_entries(pool, plan.scan_id)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?;
    let memfs = MemFs::from_seed(&seed_from_entries(&entries));

    // Record the apply job (kind `apply`), mirroring the scan job lifecycle.
    let started_at = now_iso8601_utc();
    let job_id = insert_apply_job(pool, &started_at).await?;

    // Walk the approved plan through the executor against MemFs (dry run),
    // journal-before-act: each op's intent row is flushed through the SqliteJournal
    // and committed BEFORE the (skeleton) filesystem call, then a done row after
    // (F-602, AC-10). A failed intent flush is a hard stop (journal-write-failed,
    // AC-13); on any walk error the job is marked failed and the error surfaced.
    let journal = SqliteJournal::new(pool.clone());
    let executor = Executor::new(memfs, job_id, ops);
    let outcome = match executor.run(&journal, &started_at).await {
        Ok(outcome) => outcome,
        Err(e) => {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    };

    // On completion export the self-contained undo file (manifest) to the Reports
    // folder and re-emit the F-507 provenance report reflecting final locations
    // (AC-11, AC-12). The Reports subfolder is the same deterministic per-plan one
    // the plan exports use, so a family sees the undo file beside the plan.
    let app_data_dir = abo_core::paths::app_data_dir();
    let reports_dir = abo_core::reports::plan_export_dir(&app_data_dir, plan_id, &plan.created_at);
    if let Err(e) = export_after_apply(pool, &reports_dir, job_id, plan_id, executor.ops()).await {
        mark_apply_job_failed(pool, job_id, e.code()).await;
        return Err(e);
    }

    mark_apply_job_completed(pool, job_id).await;

    Ok(ApplyReport {
        plan_id,
        job_id,
        dry_run: true,
        ops_walked: outcome.ops_walked as i64,
    })
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

/// Insert the initial `running` apply `jobs` row and return its assigned id. A
/// failure here maps to [`AppError::ApplyFailed`]: the apply never started.
async fn insert_apply_job(pool: &SqlitePool, started_at: &str) -> Result<i64, AppError> {
    let result =
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply', 'running', ?)")
            .bind(started_at)
            .execute(pool)
            .await
            .map_err(|e| AppError::ApplyFailed {
                detail: format!("could not record apply job: {e}"),
            })?;
    Ok(result.last_insert_rowid())
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
