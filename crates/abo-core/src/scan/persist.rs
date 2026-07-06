//! Snapshot persistence (F-105): write a completed scan as one immutable `scans`
//! row plus one `entries` row per walked entry, and read entries back.
//!
//! Immutability contract (AC-13): a completed snapshot is never mutated. Each
//! [`run_scan`] INSERTs a brand-new `scans` row and a fresh set of `entries`
//! rows; it never UPDATEs or DELETEs a prior scan's rows. The only UPDATE it
//! issues targets the `scans` row it just created (running -> completed).
//!
//! Parent linkage: the walk returns entries sorted by stored path, which is also
//! parent-before-child order (a parent path is a strict prefix of its
//! descendants). Inserting in that order, we record each inserted path's rowid
//! in a map, so a child's `parent_id` is a single map lookup of its parent path.
//! The scan root's parent is outside the tree, so the root gets `parent_id NULL`.
//!
//! The bulk insert runs in one transaction for speed and atomicity: either the
//! whole entry set commits or none of it does, so a snapshot is never partial.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use crate::db::activity::{append_activity, json_object, ActivityOutcome};
use crate::error::AppError;
use crate::ipc::{EntryRow, ScanSummary};
use crate::job::JobContext;
use crate::paths::{strip_extended_length_prefix, to_extended_length};
use crate::scan::exclude::ExcludeSet;
use crate::scan::longpath;
use crate::scan::walk::{self, WalkOutcome, WalkStatus, WalkedEntry};

/// The terminal result of [`run_scan_with_job`]: either a completed snapshot or a
/// cancellation that discarded its partial work.
///
/// Cancellation is NOT modeled as an [`AppError`]: it is a cooperative, expected
/// outcome (FD-02), so it flows as a distinct success-side variant rather than an
/// error the taxonomy would have to name. The shell maps it to a `cancelled`
/// `jobs`-row state via the single `run_job_to_terminal` termination path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanOutcome {
    /// The scan finished; carries the full [`ScanSummary`].
    Completed(ScanSummary),
    /// The scan was cancelled at an entry boundary. The partial snapshot was
    /// DISCARDED (no `entries` rows were written); the `scans` row exists and was
    /// marked `cancelled`. `scan_id` is that row's id, kept so the caller can
    /// correlate the cancellation to the row.
    Cancelled { scan_id: i64 },
}

/// Run a plain live scan of `root` (no excludes, no cancellation, no progress).
///
/// A thin wrapper over [`run_scan_with_job`] with an empty exclude list and an
/// inert [`JobContext`], preserved for callers and tests that want the simple
/// path. Because an inert context can never be cancelled, this always returns a
/// completed [`ScanSummary`] (or an error).
pub async fn run_scan(pool: &SqlitePool, root: &Path) -> Result<ScanSummary, AppError> {
    match run_scan_with_job(pool, root, &[], &JobContext::inert()).await? {
        ScanOutcome::Completed(summary) => Ok(summary),
        // Unreachable: an inert JobContext holds a fresh, un-cancellable flag, so
        // the walk can never report WalkStatus::Cancelled on this path.
        ScanOutcome::Cancelled { scan_id } => Err(AppError::ScanFailed {
            detail: format!(
                "internal invariant: run_scan (inert context) observed a cancellation \
                 for scan {scan_id}"
            ),
        }),
    }
}

/// Run a live scan of `root` under the job model (F-101 + F-104): validate the
/// root, record a `running` snapshot, walk the tree honoring `excludes` and
/// `ctx` (cancellation + progress), bulk-insert the entries with correct parent
/// linkage, finalize the snapshot as `completed`, and attach the FD-19 long-path
/// warnings (F-105).
///
/// Returns [`ScanOutcome::Completed`] with the [`ScanSummary`], or
/// [`ScanOutcome::Cancelled`] when `ctx` was cancelled at an entry boundary.
///
/// **Cancel semantics (the decision-gate contract, AC-104.2/104.3).** A cancelled
/// scan DISCARDS its partial snapshot: the walk stops between entries, NO
/// `entries` rows are ever written (they persist only after a full walk, in one
/// atomic transaction), and the `scans` row is marked `cancelled`. There is thus
/// no such thing as a torn or half-complete snapshot from a cancel; the discarded
/// row is inert history, distinct from a `completed` one. (A `failed` scans row,
/// by contrast, marks a DB-write failure, not a user cancel.)
///
/// Errors: [`AppError::RootNotFound`] / [`AppError::RootNotDirectory`] before any
/// row is written when the root is bad; [`AppError::ScanFailed`] for an invalid
/// exclude pattern (before any row is written) or a database write failure.
/// Per-entry permission-denied and junction cases are NOT errors - they are
/// recorded, counted in [`ScanSummary::skipped_count`], and surfaced as
/// [`ScanSummary::warnings`]; the scan runs to completion (AC-11).
///
/// F-1001 (v0.2.0 Phase 6): after [`run_scan_with_job_impl`] returns, this
/// wrapper appends exactly one `activity_records` row for the run - action
/// `"scan"`, whichever outcome resulted (`Completed` -> succeeded, `Cancelled`
/// -> cancelled, `Err` -> failed with the [`AppError::code`]) - regardless of
/// which return point inside the impl produced it, so an early root-validation
/// failure is logged just as reliably as a late DB-write failure.
pub async fn run_scan_with_job(
    pool: &SqlitePool,
    root: &Path,
    excludes: &[String],
    ctx: &JobContext,
) -> Result<ScanOutcome, AppError> {
    let result = run_scan_with_job_impl(pool, root, excludes, ctx).await;

    let params = json_object(&[
        ("root", &root.display().to_string()),
        ("excludes", &excludes.join(",")),
    ]);
    let outcome = match &result {
        Ok(ScanOutcome::Completed(_)) => ActivityOutcome::Succeeded,
        Ok(ScanOutcome::Cancelled { .. }) => ActivityOutcome::Cancelled,
        Err(e) => ActivityOutcome::Failed {
            error_code: e.code().to_string(),
        },
    };
    append_activity(pool, "scan", &params, &outcome).await;

    result
}

/// The scan implementation [`run_scan_with_job`] wraps with the F-1001
/// activity-log append. See that function's doc for the full contract; this
/// split exists only so the append happens exactly once, after every return
/// point below, without repeating it at each one.
async fn run_scan_with_job_impl(
    pool: &SqlitePool,
    root: &Path,
    excludes: &[String],
    ctx: &JobContext,
) -> Result<ScanOutcome, AppError> {
    // ---- Validate the root before touching the database ----
    let root_display = root.display().to_string();
    let md = std::fs::metadata(root).map_err(|_| AppError::RootNotFound {
        path: root_display.clone(),
    })?;
    if !md.is_dir() {
        return Err(AppError::RootNotDirectory { path: root_display });
    }

    // Compile excludes up front: an invalid pattern is a caller/config error and
    // fails the scan before any snapshot row is written.
    let exclude_set = ExcludeSet::compile(excludes).map_err(|detail| AppError::ScanFailed {
        detail: format!("scan not started: {detail}"),
    })?;

    // Normalize to the extended-length walk root (Windows `\\?\`), and derive the
    // stored (prefix-free) root path for the `scans` row.
    let normalized_root = to_extended_length(root);
    let stored_root = strip_extended_length_prefix(&normalized_root)
        .to_string_lossy()
        .into_owned();

    let started_at = walk::now_iso8601_utc();
    tracing::info!(root = %stored_root, "scan: started");

    // ---- Record the running snapshot ----
    let scan_id = insert_running_scan(pool, "live", &stored_root, &started_at).await?;

    // ---- Walk (never aborts on per-entry errors; may stop early on cancel) ----
    let WalkOutcome {
        entries,
        skipped_count,
        mut warnings,
        status,
    } = walk::walk_with_job(&normalized_root, &exclude_set, ctx);

    // Cancel: DISCARD the partial snapshot. No entries are persisted; the running
    // scans row is marked `cancelled` (best-effort) so it is inert history, not a
    // phantom `running` row (AC-104.2/104.3).
    if status == WalkStatus::Cancelled {
        mark_scan_cancelled(pool, scan_id).await;
        tracing::info!(
            scan_id,
            "scan: cancelled at an entry boundary; partial snapshot discarded"
        );
        return Ok(ScanOutcome::Cancelled { scan_id });
    }

    // From here on, the `scans` row exists. Any failure below must not leave it
    // stranded as status='running' forever (a phantom snapshot): on error, mark
    // it 'failed' (best-effort) before propagating the original error.
    // Native `Path::parent()` is correct here because every walked path was
    // built by walkdir on THIS host using this host's own separator
    // conventions (backslash on Windows, `/` elsewhere) - unlike a CSV import,
    // whose paths are always Windows-backslash text regardless of host OS (see
    // `crate::scan::csv_import`, which supplies its own parent function).
    let (entry_count, total_bytes, completed_at) =
        match persist_entries_and_finalize(pool, scan_id, &entries, |path| {
            path.parent().map(PathBuf::from)
        })
        .await
        {
            Ok(v) => v,
            Err(e) => {
                mark_scan_failed(pool, scan_id).await;
                return Err(e);
            }
        };

    // Attach the FD-19 long-path warnings, which need the completed entry list
    // and the OS setting (AC-101.4). Junction/permission warnings were collected
    // during the walk; long-path warnings are appended here.
    let long_paths_enabled = longpath::long_paths_enabled();
    warnings.extend(longpath::long_path_warnings(long_paths_enabled, &entries));

    if !warnings.is_empty() {
        tracing::warn!(
            scan_id,
            warning_count = warnings.len(),
            "scan: completed with warnings (junctions/permission/long-path)"
        );
    }

    tracing::info!(
        scan_id,
        entry_count,
        total_bytes,
        skipped_count,
        "scan: completed"
    );

    Ok(ScanOutcome::Completed(ScanSummary {
        scan_id,
        root_path: stored_root,
        entry_count,
        total_bytes,
        skipped_count: skipped_count as i64,
        started_at,
        completed_at,
        status: "completed".to_string(),
        warnings,
    }))
}

/// Bulk-insert `entries` for `scan_id` in one transaction (linking parents),
/// then finalize the `scans` row (running -> completed). Returns
/// `(entry_count, total_bytes, completed_at)` on success.
///
/// `parent_of` derives an entry's parent path (the lookup key into the
/// already-inserted map below) from its stored `path`. This is parameterized,
/// rather than hardcoding `Path::parent()`, because that method is correct
/// only when `path` was built using the CURRENT host's native separator
/// convention (true for a live walk, since walkdir builds paths on this host);
/// a WizTree CSV import's paths are always Windows-backslash text regardless
/// of the host the import runs on, so it supplies its own backslash-aware
/// `parent_of` (see `crate::scan::csv_import`). Both callers share this one
/// INSERT statement and transaction, which is how a CSV-imported snapshot
/// stays schema-identical to a live one (F-102's "indistinguishable
/// downstream" contract).
///
/// Every fallible step here (the transaction, its commit, and the finalize
/// UPDATE) runs after the `scans` row already exists; the caller is
/// responsible for marking that row 'failed' if this returns `Err`.
pub(crate) async fn persist_entries_and_finalize(
    pool: &SqlitePool,
    scan_id: i64,
    entries: &[WalkedEntry],
    parent_of: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<(i64, i64, String), AppError> {
    // ---- Bulk-insert entries in one transaction, linking parents ----
    let entry_count = entries.len() as i64;
    let total_bytes: i64 = entries.iter().map(|e| e.size as i64).sum();

    let mut tx = pool.begin().await.map_err(scan_failed)?;
    let mut ids: HashMap<PathBuf, i64> = HashMap::with_capacity(entries.len());
    for e in entries {
        // The parent path is a key already inserted (sorted parent-before-child);
        // absent for the root, whose parent lies outside the scanned tree.
        let parent_id = parent_of(&e.path).and_then(|parent| ids.get(&parent).copied());

        let path_str = e.path.to_string_lossy().into_owned();
        let result = sqlx::query(
            "INSERT INTO entries \
             (scan_id, parent_id, path, name, kind, file_class, size, mtime, depth) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(scan_id)
        .bind(parent_id)
        .bind(&path_str)
        .bind(&e.name)
        .bind(e.kind.as_str())
        .bind(e.file_class.map(|c| c.as_str()))
        .bind(e.size as i64)
        .bind(e.mtime.as_deref())
        .bind(e.depth as i64)
        .execute(&mut *tx)
        .await
        .map_err(scan_failed)?;

        ids.insert(e.path.clone(), result.last_insert_rowid());
    }
    tx.commit().await.map_err(scan_failed)?;

    // ---- Finalize the snapshot (running -> completed). Only this scan's own
    // row is touched; prior snapshots are never mutated (AC-13). ----
    let completed_at = walk::now_iso8601_utc();
    sqlx::query(
        "UPDATE scans SET entry_count = ?, total_bytes = ?, completed_at = ?, \
         status = 'completed' WHERE id = ?",
    )
    .bind(entry_count)
    .bind(total_bytes)
    .bind(&completed_at)
    .bind(scan_id)
    .execute(pool)
    .await
    .map_err(scan_failed)?;

    Ok((entry_count, total_bytes, completed_at))
}

/// Read back every entry of a snapshot, path-sorted, for the tracer UI.
///
/// The read order is deterministic SQL byte order over the stored `path`
/// column, not a re-derivation of the walk's traversal order. In practice the
/// two coincide, since paths are inserted parent-before-child, but SQL byte
/// collation and `PathBuf` component order can diverge on exotic sibling names
/// (for example names differing only by case or by characters that sort
/// differently as raw bytes than as path components), so do not rely on this
/// order exactly reproducing the walk's component order in edge cases.
pub async fn get_scan_entries(pool: &SqlitePool, scan_id: i64) -> Result<Vec<EntryRow>, AppError> {
    let rows = sqlx::query_as::<_, EntryRow>(
        "SELECT id, scan_id, parent_id, path, name, kind, file_class, size, mtime, depth \
         FROM entries WHERE scan_id = ? ORDER BY path",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await
    .map_err(scan_failed)?;
    Ok(rows)
}

/// The id of the most recently STARTED snapshot whose `status = 'completed'`,
/// or `None` when no scan has ever completed (the honest pre-first-scan state,
/// v0.4.0 Phase 4). "Most recent" is by `scans.id` (an autoincrementing
/// rowid), which agrees with insertion/start order since rows are never
/// renumbered (AC-13 immutability).
///
/// Used by the `classify_overview` command (F-902 library home, T-15): the
/// frontend never names a `scan_id` itself, so this is how the shell finds
/// "the library, right now" without a live job to correlate to.
pub async fn latest_completed_scan_id(pool: &SqlitePool) -> Result<Option<i64>, AppError> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM scans WHERE status = 'completed' ORDER BY id DESC LIMIT 1")
            .fetch_optional(pool)
            .await
            .map_err(scan_failed)?;
    Ok(row.map(|(id,)| id))
}

/// Insert the initial `running` `scans` row and return its assigned id.
/// `source` is `"live"` for [`run_scan_with_job`] or `"csv"` for a WizTree
/// import (`crate::scan::csv_import::run_csv_import`); both values are the
/// only two the `scans.source` CHECK constraint (migration 0001) allows.
pub(crate) async fn insert_running_scan(
    pool: &SqlitePool,
    source: &str,
    stored_root: &str,
    started_at: &str,
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO scans (source, root_path, started_at, status) \
         VALUES (?, ?, ?, 'running')",
    )
    .bind(source)
    .bind(stored_root)
    .bind(started_at)
    .execute(pool)
    .await
    .map_err(scan_failed)?;
    Ok(result.last_insert_rowid())
}

/// Best-effort mark a `scans` row as `status = 'failed'` after some later step
/// of the scan (the entries transaction, its commit, or the finalize UPDATE)
/// has failed. Without this, a mid-scan failure leaves the row stranded as
/// `status = 'running'` with a NULL `entry_count` forever: a phantom snapshot
/// that never completes and is never cleaned up.
///
/// This is deliberately best-effort: it is called from an error path that is
/// already about to return the original error, so a secondary failure here
/// must never replace or mask it. On failure to mark, log via `tracing` and
/// move on.
pub(crate) async fn mark_scan_failed(pool: &SqlitePool, scan_id: i64) {
    let result = sqlx::query("UPDATE scans SET status = 'failed' WHERE id = ?")
        .bind(scan_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        tracing::error!(
            scan_id,
            error = %e,
            "scan: failed to mark stranded scans row as 'failed'"
        );
    }
}

/// Best-effort mark a `scans` row as `status = 'cancelled'` after the walk was
/// stopped by a cancellation at an entry boundary. No `entries` rows were
/// written (the partial snapshot is discarded), so the `cancelled` row is inert
/// history: distinct from a `completed` snapshot (which has entries) and from a
/// `failed` one (a DB-write failure). See [`run_scan_with_job`]'s cancel-semantics
/// contract.
///
/// Deliberately best-effort like [`mark_scan_failed`]: it runs on the way out of
/// a cancelled scan, so a secondary DB error here is logged, not propagated.
async fn mark_scan_cancelled(pool: &SqlitePool, scan_id: i64) {
    let result = sqlx::query("UPDATE scans SET status = 'cancelled' WHERE id = ?")
        .bind(scan_id)
        .execute(pool)
        .await;
    if let Err(e) = result {
        tracing::error!(
            scan_id,
            error = %e,
            "scan: failed to mark cancelled scans row"
        );
    }
}

/// Map a SQLite/transaction error into the Scan-family hard-failure code.
fn scan_failed(e: sqlx::Error) -> AppError {
    AppError::ScanFailed {
        detail: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use tempfile::TempDir;

    /// A fresh, migrated pool in its own temp dir (kept alive by the returned
    /// `TempDir`).
    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    async fn scan_status(pool: &SqlitePool, scan_id: i64) -> String {
        let (status,): (String,) = sqlx::query_as("SELECT status FROM scans WHERE id = ?")
            .bind(scan_id)
            .fetch_one(pool)
            .await
            .expect("fetch status");
        status
    }

    /// `mark_scan_failed` flips a `running` row to `failed`, in isolation from
    /// any real scan.
    #[tokio::test]
    async fn mark_scan_failed_flips_running_row_to_failed() {
        let (_dir, pool) = fresh_pool().await;
        let scan_id = insert_running_scan(&pool, "live", "C:\\some\\root", "2026-01-01T00:00:00Z")
            .await
            .expect("insert running scan");
        assert_eq!(scan_status(&pool, scan_id).await, "running");

        mark_scan_failed(&pool, scan_id).await;

        assert_eq!(scan_status(&pool, scan_id).await, "failed");
    }

    /// `mark_scan_failed` against a non-existent row is a no-op UPDATE (matches
    /// zero rows), not an error: it must never panic or propagate a failure,
    /// since it is always called from a path that is already unwinding with the
    /// real error.
    #[tokio::test]
    async fn mark_scan_failed_on_missing_row_does_not_panic() {
        let (_dir, pool) = fresh_pool().await;
        mark_scan_failed(&pool, 999_999).await;
    }

    /// `latest_completed_scan_id` is `None` before any scan has completed, then
    /// tracks the most recently completed row - skipping a later `running` or
    /// `failed` row rather than mistaking it for the current library state
    /// (F-902 library home, T-15).
    #[tokio::test]
    async fn latest_completed_scan_id_tracks_the_newest_completed_row() {
        let (_dir, pool) = fresh_pool().await;
        assert_eq!(latest_completed_scan_id(&pool).await.unwrap(), None);

        let first = insert_running_scan(&pool, "live", "C:\\a", "2026-01-01T00:00:00Z")
            .await
            .expect("insert first");
        sqlx::query("UPDATE scans SET status = 'completed' WHERE id = ?")
            .bind(first)
            .execute(&pool)
            .await
            .expect("complete first");
        assert_eq!(latest_completed_scan_id(&pool).await.unwrap(), Some(first));

        // A second scan that FAILS must not shadow the still-good first snapshot.
        let failed = insert_running_scan(&pool, "live", "C:\\b", "2026-01-02T00:00:00Z")
            .await
            .expect("insert second");
        mark_scan_failed(&pool, failed).await;
        assert_eq!(latest_completed_scan_id(&pool).await.unwrap(), Some(first));

        // A third scan that completes becomes the new latest.
        let second = insert_running_scan(&pool, "live", "C:\\c", "2026-01-03T00:00:00Z")
            .await
            .expect("insert third");
        sqlx::query("UPDATE scans SET status = 'completed' WHERE id = ?")
            .bind(second)
            .execute(&pool)
            .await
            .expect("complete third");
        assert_eq!(latest_completed_scan_id(&pool).await.unwrap(), Some(second));
    }

    /// Count `entries` rows persisted for a given scan id.
    async fn entry_count_for(pool: &SqlitePool, scan_id: i64) -> i64 {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entries WHERE scan_id = ?")
            .bind(scan_id)
            .fetch_one(pool)
            .await
            .expect("count entries");
        n
    }

    /// AC-104.2/104.3: a scan cancelled at an entry boundary DISCARDS its partial
    /// snapshot - the `scans` row is marked `cancelled` and NO `entries` rows are
    /// written - while a readable tree that the walk had begun to enumerate is
    /// simply dropped. The cancel is driven from the progress sink (flip the flag
    /// after the first reported entry), so it stops mid-walk, not before it began.
    #[tokio::test]
    async fn cancelled_scan_discards_snapshot() {
        use crate::job::{CancelFlag, JobContext, ProgressUpdate};
        use std::sync::Arc;

        let (_db, pool) = fresh_pool().await;

        // A small tree with enough entries that a cancel after entry #1 leaves
        // real un-walked entries behind.
        let tree = TempDir::new().expect("scan tree tempdir");
        for i in 0..5 {
            let sub = tree.path().join(format!("dir{i}"));
            std::fs::create_dir(&sub).unwrap();
            std::fs::write(sub.join("book.m4b"), b"audio").unwrap();
        }

        let cancel = CancelFlag::new();
        let sink_cancel = cancel.clone();
        // Cancel as soon as the first entry is reported; the next boundary stops.
        let progress = Arc::new(move |_u: ProgressUpdate| {
            sink_cancel.cancel();
        });
        let ctx = JobContext::new(cancel, progress);

        let outcome = run_scan_with_job(&pool, tree.path(), &[], &ctx)
            .await
            .expect("scan runs");

        let scan_id = match outcome {
            ScanOutcome::Cancelled { scan_id } => scan_id,
            other => panic!("expected Cancelled, got {other:?}"),
        };
        assert_eq!(
            scan_status(&pool, scan_id).await,
            "cancelled",
            "the scans row must be marked cancelled"
        );
        assert_eq!(
            entry_count_for(&pool, scan_id).await,
            0,
            "a cancelled scan writes no entries (partial snapshot discarded)"
        );
    }

    /// A scan whose exclude pattern is invalid fails cleanly with ScanFailed
    /// before any `scans` row is written (an invalid glob is a config error).
    #[tokio::test]
    async fn invalid_exclude_pattern_fails_before_writing() {
        use crate::job::JobContext;

        let (_db, pool) = fresh_pool().await;
        let tree = TempDir::new().expect("scan tree tempdir");

        let result =
            run_scan_with_job(&pool, tree.path(), &["[".to_string()], &JobContext::inert()).await;
        assert!(
            matches!(result, Err(AppError::ScanFailed { .. })),
            "an invalid exclude glob must fail with ScanFailed, got {result:?}"
        );

        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scans")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "no scans row is written for an invalid exclude");
    }

    /// End-to-end (real mid-scan failure): drop the `entries` table after the
    /// `scans` row is inserted but before the walk's entries are persisted, so
    /// the bulk-insert transaction inside `run_scan` fails for real. The `scans`
    /// row must come out `failed`, not stranded as `running`, and `run_scan`
    /// must still surface the original error to the caller.
    #[tokio::test]
    async fn run_scan_marks_scans_row_failed_on_real_mid_scan_failure() {
        let (_db_dir, pool) = fresh_pool().await;

        // Sabotage: the `entries` table no longer exists, so any INSERT into it
        // fails. run_scan always inserts at least the root entry, so this forces
        // a real failure inside persist_entries_and_finalize on every scan.
        sqlx::query("DROP TABLE entries")
            .execute(&pool)
            .await
            .expect("drop entries table");

        let scan_root = TempDir::new().expect("scan root tempdir");

        let result = run_scan(&pool, scan_root.path()).await;
        assert!(
            matches!(result, Err(AppError::ScanFailed { .. })),
            "expected ScanFailed, got {result:?}"
        );

        let (scan_id,): (i64,) = sqlx::query_as("SELECT id FROM scans ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("fetch the scans row run_scan inserted");
        assert_eq!(
            scan_status(&pool, scan_id).await,
            "failed",
            "the scans row must be marked failed, not left stranded as running"
        );
    }
}
