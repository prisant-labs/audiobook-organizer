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
use crate::scan::walk::{self, WalkOutcome};

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

    // ---- Bulk-insert entries in one transaction, linking parents ----
    let entry_count = entries.len() as i64;
    let total_bytes: i64 = entries.iter().map(|e| e.size as i64).sum();

    let mut tx = pool.begin().await.map_err(scan_failed)?;
    let mut ids: HashMap<PathBuf, i64> = HashMap::with_capacity(entries.len());
    for e in &entries {
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

/// Read back every entry of a snapshot, path-sorted, for the tracer UI.
///
/// Ordering matches the walk's persisted order (by `path`), so a read reproduces
/// the deterministic scan order.
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

/// Map a SQLite/transaction error into the Scan-family hard-failure code.
fn scan_failed(e: sqlx::Error) -> AppError {
    AppError::ScanFailed {
        detail: e.to_string(),
    }
}
