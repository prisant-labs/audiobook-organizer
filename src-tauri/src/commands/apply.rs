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
use abo_core::exec::verify::{affected_roots, write_check_report};
use abo_core::exec::{
    delta_health_metrics, ensure_forward_tidying_allowed, record_block, verify_job, ApplyMode,
    ApplyScope, CheckReport, ExecHalt, Executor, MemFs, RealFs, SeedEntry, SqliteJournal, Vfs,
};
use abo_core::ipc::{AppError, ApplyReport, EntryRow};
use abo_core::plan::builder::default_set_aside_root;
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

    // Forward-tidying gate (F-604, AC-20): if a previous apply's after-the-fact
    // check found an UNACKNOWLEDGED discrepancy, forward tidying is paused until a
    // human acknowledges it. This gate is FORWARD-only and structural: an UNDO
    // (inverse) plan is exempt (undo is the remedy for a discrepancy), so a blocked
    // library can always be put back. `rollback_prepare` never routes through here,
    // so preparing an undo is never gated either. Checked before the lock so a
    // refused forward apply never even acquires it.
    ensure_forward_tidying_allowed(pool, &ops).await?;

    // Resolve the FD-34 apply scope (library root + set-aside root): the ONLY two
    // areas the walk may write into, and the roots the executor uses to substitute
    // the real job id into set-aside targets and to refuse any out-of-scope target.
    let scope = resolve_apply_scope(pool, plan.scan_id).await?;

    // Capture what the after-the-fact check needs before `scope` is moved into the
    // executor: the snapshot the plan was built over (the delta "before"), and the
    // library root the incremental rescan re-reads (AC-19).
    let before_scan_id = plan.scan_id;
    let library_root = scope.library_root.clone();

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
            let executor = Executor::with_scope(memfs, job_id, ops, scope);
            walk_and_finalize(
                pool,
                executor,
                &journal,
                &reports_dir,
                plan_id,
                mode,
                job_id,
                &started_at,
                before_scan_id,
                &library_root,
            )
            .await
        }
        ApplyMode::Real => {
            // Real apply against the actual filesystem. The human-only gate to run a
            // Real apply against a real library is procedural (EXECUTION.md); this is
            // the RealFs executor that gate authorizes.
            let executor = Executor::with_scope(RealFs::new(), job_id, ops, scope);
            walk_and_finalize(
                pool,
                executor,
                &journal,
                &reports_dir,
                plan_id,
                mode,
                job_id,
                &started_at,
                before_scan_id,
                &library_root,
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
    before_scan_id: i64,
    library_root: &str,
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

    // Did every approved op walk without an operation-level halt? A halt (AC-5/6/7/9)
    // means a move failed and stopped the group.
    let walk_completed = outcome.halt.is_none();

    // Guard #2 (STRUCTURAL, by ordering): a walk that COMPLETED exports its
    // self-contained undo file (manifest) and re-emits the F-507 provenance report
    // (AC-11, AC-12) HERE, BEFORE the after-the-fact check runs. So even if the
    // check then finds a difference, the undo file is already written - a job whose
    // walk completed always keeps its undo, because the moves DID happen and the
    // user needs the undo. A HALTED walk exports no undo file (an undo over all
    // approved ops would claim moves that never happened; the journal is the durable
    // record a v0.6.0 reconciliation reads instead).
    if walk_completed {
        if let Err(e) =
            export_after_apply(pool, reports_dir, mode, job_id, plan_id, executor.ops()).await
        {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    }

    // The after-the-fact check (F-604). Prove the moves happened as planned by
    // re-checking the EXECUTOR'S OWN ops against the SAME Vfs the job ran on
    // (AC-18): a dry run verifies its MemFs, a Real apply verifies RealFs. This runs
    // on BOTH a clean and a halted walk, so a halt reports per-op truth (completed
    // ops verified good, the halted op reported as halted) rather than a blanket
    // failure - the P5 teardown-halt (fully restored, teardown op halted) reads
    // fairly, and P8 frames it.
    let verify_report = verify_job(executor.vfs(), executor.ops(), &outcome);

    // AC-19: re-read reality through the EXISTING scanner and compute the delta
    // health metrics, for a COMPLETED Real apply only (a dry run changed nothing on
    // disk; a halted walk left a partial state we do not re-scan). Best-effort: a
    // rescan failure is logged and never fails the apply.
    let delta = if mode == ApplyMode::Real && walk_completed && !library_root.is_empty() {
        match delta_health_metrics(pool, before_scan_id, std::path::Path::new(library_root)).await {
            Ok(d) => Some(d),
            Err(e) => {
                log::warn!("post-apply rescan/delta for job {job_id} failed: {e}");
                None
            }
        }
    } else {
        None
    };

    // Export the after-the-fact check report artifact beside the undo file (F-1002).
    // Best-effort: a report-write failure is logged, never fails the apply.
    let roots = affected_roots(executor.ops());
    let check = CheckReport::build(mode, &verify_report, roots, delta);
    if let Err(e) = write_check_report(reports_dir, &check) {
        log::warn!("could not write the after-the-fact check report for job {job_id}: {e}");
    }

    // AC-20: a difference the check found blocks further FORWARD tidy-ups until a
    // human acknowledges it. The block is durable (survives a restart) and
    // append-only. If recording it fails, mark the job failed (releasing the lock)
    // and surface the error rather than leaving forward tidying silently open.
    let blocked = verify_report.has_discrepancy();
    if blocked {
        let now = now_iso8601_utc();
        if let Err(e) = record_block(
            pool,
            job_id,
            &now,
            &verify_report.discrepancy_summary_json(),
        )
        .await
        {
            mark_apply_job_failed(pool, job_id, e.code()).await;
            return Err(e);
        }
    }

    // Terminal state. A halted walk surfaces its halt exactly as before (the
    // teardown-halt case included); the after-the-fact check above already recorded
    // its per-op truth to the report.
    if let Some(halt) = &outcome.halt {
        let err = halt_to_error(halt);
        mark_apply_job_failed(pool, job_id, err.code()).await;
        return Err(err);
    }

    mark_apply_job_completed(pool, job_id).await;

    Ok(ApplyReport {
        plan_id,
        job_id,
        dry_run: mode == ApplyMode::DryRun,
        ops_walked: outcome.ops_walked as i64,
        verified_ops: verify_report.verified_count() as i64,
        discrepancy_count: verify_report.discrepancy_count() as i64,
        blocked,
    })
}

/// Resolve the FD-34 apply scope for the plan's `scan_id`: the library root the
/// plan was built over (the scan's `root_path`) plus the resolved set-aside root
/// (the F-803 `set_aside_root` setting when configured, else the FD-34 default
/// sibling `<library-parent>\Set Aside\`). These MUST match what the builder used
/// when it stamped the set-aside targets; in the normal flow (generate then apply
/// without changing settings) they do, and the executor's scope guard is a safe
/// backstop if they ever diverge.
async fn resolve_apply_scope(pool: &SqlitePool, scan_id: i64) -> Result<ApplyScope, AppError> {
    let root_path: Option<String> = sqlx::query_scalar("SELECT root_path FROM scans WHERE id = ?")
        .bind(scan_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::ApplyFailed {
            detail: e.to_string(),
        })?;
    let library_root = root_path.ok_or_else(|| AppError::ApplyFailed {
        detail: "the plan's scan has no recorded library root".to_string(),
    })?;
    let settings = abo_core::db::settings::get_settings(pool).await?;
    let set_aside_root = settings
        .set_aside_root
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_set_aside_root(&library_root));
    Ok(ApplyScope {
        library_root,
        set_aside_root,
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

    /// A halted walk (here a source-vanished op against an empty MemFs) drives
    /// `walk_and_finalize` down its halt branch: it surfaces the halt code, marks the
    /// apply `jobs` row `failed`, and exports NO undo file (an undo file over all
    /// approved ops would claim moves that never happened - the journal is the
    /// durable record instead).
    #[tokio::test]
    async fn a_halted_walk_marks_the_job_failed_and_writes_no_undo_file() {
        use abo_core::db::open_db;
        use abo_core::db::plans::PlanOpRow;
        use abo_core::exec::MANIFEST_JSON_BASENAME;

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        // Acquire the running apply job (the lock + the row walk_and_finalize marks).
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, "2026-07-18T00:00:00Z")
            .await
            .expect("acquire apply job");

        // An approved move whose source does not exist in the (empty) MemFs, so the
        // TOCTOU re-check halts with source-vanished.
        let op = PlanOpRow {
            id: 1,
            plan_id: 1,
            seq: 0,
            op_group: "loose-root-books".to_string(),
            kind: "move".to_string(),
            kind_reason: None,
            source_path: "E:/lib/GONE.m4b".to_string(),
            target_path: "E:/lib/Author/GONE.m4b".to_string(),
            rationale: "test.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        };
        let executor = Executor::new(MemFs::new(), job_id, vec![op]);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let err = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            1,
            ApplyMode::DryRun,
            job_id,
            "2026-07-18T00:00:00Z",
            // before_scan_id + library_root: a DryRun halt does no rescan, so these
            // are inert here.
            0,
            "",
        )
        .await
        .expect_err("a halted walk surfaces the halt as an error");
        assert_eq!(err.code(), "source-vanished");

        // The jobs row is marked failed (the lock is released), not left running.
        let state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .expect("job state");
        assert_eq!(state, "failed");

        // No undo file was exported for the halted run.
        assert!(
            !reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a halted run must not export an undo file"
        );

        // The journal is consistent: the one intent has a terminal (failed) row.
        let phases: Vec<String> =
            sqlx::query_scalar("SELECT phase FROM journal WHERE job_id = ? ORDER BY id")
                .bind(job_id)
                .fetch_all(&pool)
                .await
                .expect("journal rows");
        assert_eq!(phases, vec!["intent".to_string(), "failed".to_string()]);
    }

    // ---- F-604 after-the-fact check wiring (v0.5.0 Phase 6) ----

    use abo_core::db::plans::PlanOpRow;

    const P6_NOW: &str = "2026-07-18T00:00:00Z";

    /// Seed the minimal `scans`/`rulesets`/`plans` rows so `export_after_apply`'s
    /// `manifests` FK (to `plans` and `jobs`) resolves, returning the new plan id.
    /// The executor is driven with in-memory ops (below); this only needs a plan
    /// row to exist for the undo-file export.
    async fn seed_plan(pool: &SqlitePool) -> i64 {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:/lib', ?, 'completed')",
        )
        .bind(P6_NOW)
        .execute(pool)
        .await
        .expect("scan")
        .last_insert_rowid();
        let ruleset_id = abo_core::db::rulesets::insert_ruleset(
            pool,
            &abo_core::db::rulesets::NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            P6_NOW,
        )
        .await
        .expect("ruleset");
        sqlx::query(
            "INSERT INTO plans (scan_id, ruleset_id, created_at, status) VALUES (?, ?, ?, 'draft')",
        )
        .bind(scan_id)
        .bind(ruleset_id)
        .bind(P6_NOW)
        .execute(pool)
        .await
        .expect("plan")
        .last_insert_rowid()
    }

    /// One approved move op, in memory (the executor walks these directly).
    fn move_op(source: &str, target: &str, size: i64) -> PlanOpRow {
        PlanOpRow {
            id: 1,
            plan_id: 1,
            seq: 0,
            op_group: "loose-root-books".to_string(),
            kind: "move".to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size: size,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    /// AC-18/AC-20: a CLEAN completed walk verifies every op and records NO block;
    /// forward tidying stays open. The undo file is exported (the walk completed).
    #[tokio::test]
    async fn a_clean_completed_walk_records_no_block() {
        use abo_core::exec::{forward_tidying_blocked, MANIFEST_JSON_BASENAME};

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = abo_core::db::open_db(db.path()).await.expect("open_db");
        let plan_id = seed_plan(&pool).await;
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, P6_NOW)
            .await
            .expect("acquire apply job");

        let seed = vec![
            SeedEntry {
                path: "E:/lib".into(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/loose.m4b".into(),
                size: 4096,
                is_dir: false,
            },
        ];
        let ops = vec![move_op("E:/lib/loose.m4b", "E:/lib/Author/loose.m4b", 4096)];
        let executor = Executor::new(MemFs::from_seed(&seed), job_id, ops);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let report = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            plan_id,
            ApplyMode::DryRun,
            job_id,
            P6_NOW,
            0,
            "",
        )
        .await
        .expect("a clean walk completes");

        assert!(!report.blocked, "a clean apply does not block");
        assert_eq!(report.discrepancy_count, 0);
        assert_eq!(report.verified_ops, 1);
        assert!(
            !forward_tidying_blocked(&pool).await.unwrap(),
            "forward tidying stays open after a clean apply"
        );
        assert!(
            reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a completed walk exports its undo file"
        );
    }

    /// AC-18/AC-20 + guard #2 (structural): a completed walk whose after-the-fact
    /// check finds a difference (an injected fault: the target reports missing at
    /// check time) records a durable block AND STILL exports its undo file - the
    /// moves happened, so the user keeps the undo even though the check failed.
    #[tokio::test]
    async fn a_discrepancy_after_a_completed_walk_blocks_but_keeps_the_undo_file() {
        use abo_core::exec::{
            forward_tidying_blocked, VfsError, VfsMetadata, MANIFEST_JSON_BASENAME,
        };

        // A Vfs that hides ONE target from `exists`/`metadata` while delegating
        // everything else (including the move itself) to an inner MemFs: the move
        // succeeds, but the after-the-fact check sees the target as missing.
        struct HauntedFs {
            inner: MemFs,
            haunted: std::path::PathBuf,
        }
        impl Vfs for HauntedFs {
            fn exists(&self, p: &Path) -> bool {
                if p == self.haunted {
                    return false;
                }
                self.inner.exists(p)
            }
            fn is_dir(&self, p: &Path) -> bool {
                self.inner.is_dir(p)
            }
            fn metadata(&self, p: &Path) -> Result<VfsMetadata, VfsError> {
                if p == self.haunted {
                    return Err(VfsError::NotFound(p.to_path_buf()));
                }
                self.inner.metadata(p)
            }
            fn rename(&self, a: &Path, b: &Path) -> Result<(), VfsError> {
                self.inner.rename(a, b)
            }
            fn copy_file(&self, a: &Path, b: &Path) -> Result<u64, VfsError> {
                self.inner.copy_file(a, b)
            }
            fn remove_file(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_file(p)
            }
            fn remove_dir(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_dir(p)
            }
            fn create_dir_all(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.create_dir_all(p)
            }
        }

        let db = tempfile::TempDir::new().expect("db tempdir");
        let (pool, _) = abo_core::db::open_db(db.path()).await.expect("open_db");
        let plan_id = seed_plan(&pool).await;
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, P6_NOW)
            .await
            .expect("acquire apply job");

        let seed = vec![
            SeedEntry {
                path: "E:/lib".into(),
                size: 0,
                is_dir: true,
            },
            SeedEntry {
                path: "E:/lib/loose.m4b".into(),
                size: 4096,
                is_dir: false,
            },
        ];
        let haunted = HauntedFs {
            inner: MemFs::from_seed(&seed),
            haunted: std::path::PathBuf::from("E:/lib/Author/loose.m4b"),
        };
        let ops = vec![move_op("E:/lib/loose.m4b", "E:/lib/Author/loose.m4b", 4096)];
        let executor = Executor::new(haunted, job_id, ops);
        let journal = SqliteJournal::new(pool.clone());
        let reports = tempfile::TempDir::new().expect("reports tempdir");

        let report = walk_and_finalize(
            &pool,
            executor,
            &journal,
            reports.path(),
            plan_id,
            ApplyMode::DryRun,
            job_id,
            P6_NOW,
            0,
            "",
        )
        .await
        .expect("the walk completed (the discrepancy is post-facto, not a halt)");

        // AC-20: the check found a difference, so a durable block was recorded.
        assert!(report.blocked, "the discrepancy blocks further tidying");
        assert_eq!(report.discrepancy_count, 1);
        assert!(
            forward_tidying_blocked(&pool).await.unwrap(),
            "forward tidying is blocked until acknowledged"
        );
        // Guard #2: the undo file was STILL exported (the walk completed).
        assert!(
            reports.path().join(MANIFEST_JSON_BASENAME).exists(),
            "a completed walk keeps its undo file even when the check fails"
        );
        // The block is durable: a `verification_blocks` raised row exists.
        let raised: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_blocks WHERE job_id = ? AND state = 'raised'",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(raised, 1);
    }
}
