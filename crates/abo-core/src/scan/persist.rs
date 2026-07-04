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

use crate::error::AppError;
use crate::ipc::{EntryRow, ScanSummary};
use crate::paths::{strip_extended_length_prefix, to_extended_length};
use crate::scan::walk::{self, WalkOutcome, WalkedEntry};

/// Run a live scan of `root`: validate the root, record a `running` snapshot,
/// walk the tree (F-101) typing each file (F-103), bulk-insert the entries with
/// correct parent linkage, and finalize the snapshot as `completed` (F-105).
///
/// Returns the [`ScanSummary`] for the snapshot written. Errors:
/// [`AppError::RootNotFound`] / [`AppError::RootNotDirectory`] before any row is
/// written when the root is bad, and [`AppError::ScanFailed`] if the database
/// write path fails. Per-entry permission-denied and junction cases are NOT
/// errors - they are recorded and counted in [`ScanSummary::skipped_count`], and
/// the scan runs to completion (AC-11).
pub async fn run_scan(pool: &SqlitePool, root: &Path) -> Result<ScanSummary, AppError> {
    // ---- Validate the root before touching the database ----
    let root_display = root.display().to_string();
    let md = std::fs::metadata(root).map_err(|_| AppError::RootNotFound {
        path: root_display.clone(),
    })?;
    if !md.is_dir() {
        return Err(AppError::RootNotDirectory { path: root_display });
    }

    // Normalize to the extended-length walk root (Windows `\\?\`), and derive the
    // stored (prefix-free) root path for the `scans` row.
    let normalized_root = to_extended_length(root);
    let stored_root = strip_extended_length_prefix(&normalized_root)
        .to_string_lossy()
        .into_owned();

    let started_at = walk::now_iso8601_utc();
    tracing::info!(root = %stored_root, "scan: started");

    // ---- Record the running snapshot ----
    let scan_id = insert_running_scan(pool, &stored_root, &started_at).await?;

    // ---- Walk (never aborts; edge cases become skips) ----
    let WalkOutcome {
        entries,
        skipped_count,
    } = walk::walk(&normalized_root);

    // From here on, the `scans` row exists. Any failure below must not leave it
    // stranded as status='running' forever (a phantom snapshot): on error, mark
    // it 'failed' (best-effort) before propagating the original error.
    let (entry_count, total_bytes, completed_at) =
        match persist_entries_and_finalize(pool, scan_id, &entries).await {
            Ok(v) => v,
            Err(e) => {
                mark_scan_failed(pool, scan_id).await;
                return Err(e);
            }
        };

    tracing::info!(
        scan_id,
        entry_count,
        total_bytes,
        skipped_count,
        "scan: completed"
    );

    Ok(ScanSummary {
        scan_id,
        root_path: stored_root,
        entry_count,
        total_bytes,
        skipped_count: skipped_count as i64,
        started_at,
        completed_at,
        status: "completed".to_string(),
    })
}

/// Bulk-insert `entries` for `scan_id` in one transaction (linking parents),
/// then finalize the `scans` row (running -> completed). Returns
/// `(entry_count, total_bytes, completed_at)` on success.
///
/// Every fallible step here (the transaction, its commit, and the finalize
/// UPDATE) runs after the `scans` row already exists; the caller is
/// responsible for marking that row 'failed' if this returns `Err`.
async fn persist_entries_and_finalize(
    pool: &SqlitePool,
    scan_id: i64,
    entries: &[WalkedEntry],
) -> Result<(i64, i64, String), AppError> {
    // ---- Bulk-insert entries in one transaction, linking parents ----
    let entry_count = entries.len() as i64;
    let total_bytes: i64 = entries.iter().map(|e| e.size as i64).sum();

    let mut tx = pool.begin().await.map_err(scan_failed)?;
    let mut ids: HashMap<PathBuf, i64> = HashMap::with_capacity(entries.len());
    for e in entries {
        // The parent path is a key already inserted (sorted parent-before-child);
        // absent for the root, whose parent lies outside the scanned tree.
        let parent_id = e.path.parent().and_then(|parent| ids.get(parent).copied());

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

/// Insert the initial `running` `scans` row and return its assigned id.
async fn insert_running_scan(
    pool: &SqlitePool,
    stored_root: &str,
    started_at: &str,
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO scans (source, root_path, started_at, status) \
         VALUES ('live', ?, ?, 'running')",
    )
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
async fn mark_scan_failed(pool: &SqlitePool, scan_id: i64) {
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
        let scan_id = insert_running_scan(&pool, "C:\\some\\root", "2026-01-01T00:00:00Z")
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
