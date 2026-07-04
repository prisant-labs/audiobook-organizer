//! IPC payload structs shared between `abo-core` and the `src-tauri` command
//! layer, each deriving `serde::Serialize`, `serde::Deserialize`, and
//! `specta::Type`.
//!
//! v0.1.0 spine. Phase 3 introduces the scanner's two payload shapes here (the
//! natural home the Phase 1 stub reserved): [`ScanSummary`] (returned by
//! [`crate::scan::run_scan`]) and [`EntryRow`] (returned by
//! [`crate::scan::get_scan_entries`] for the Phase 6 tracer UI). Both derive
//! `serde` both ways and `specta::Type` so `Result<T, AppError>` is a valid
//! tauri-specta command return type once the seam is wired (Phase 5, AC-4).
//! [`EntryRow`] additionally derives `sqlx::FromRow` so the persistence layer
//! can read rows straight back out; this keeps the wire shape and the row shape
//! one and the same. Phase 4 may extend these additively.

use serde::{Deserialize, Serialize};

/// The result of a completed [`crate::scan::run_scan`]: the metadata of the one
/// immutable `scans` row it wrote (F-105), plus the count of entries skipped
/// during the walk (permission-denied subtrees, unstatable entries; AC-11).
///
/// `total_bytes` is the sum of file sizes (directories contribute 0). Timestamps
/// are ISO-8601 UTC, whole-second precision (see [`crate::scan`]). `root_path`
/// is the stored, human-readable form (no `\\?\` verbatim prefix).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ScanSummary {
    /// `scans.id` of the snapshot this scan wrote.
    pub scan_id: i64,
    /// The normalized scan root, stored form (no `\\?\` prefix).
    pub root_path: String,
    /// Number of `entries` rows written (files + directories, including root).
    pub entry_count: i64,
    /// Sum of file sizes in bytes (directories contribute 0).
    pub total_bytes: i64,
    /// Entries/subtrees skipped during the walk (permission-denied, unstatable).
    pub skipped_count: i64,
    /// When the scan started (ISO-8601 UTC).
    pub started_at: String,
    /// When the scan completed (ISO-8601 UTC).
    pub completed_at: String,
    /// Terminal status of the snapshot; `completed` for a successful scan.
    pub status: String,
}

/// One persisted `entries` row (F-101 / F-105), read back for the tracer.
///
/// The shape mirrors the `entries` table one-to-one. `parent_id` is the logical
/// tree edge (the `entries.id` of the containing directory), `None` for the
/// scan root. `kind` is `"file"` or `"dir"`; `file_class` is the F-103 class
/// string for files and `None` for directories. `path` is the stored,
/// human-readable full path (no `\\?\` prefix). `mtime` is ISO-8601 UTC, or
/// `None` when the entry could not be stat'd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, sqlx::FromRow)]
pub struct EntryRow {
    /// `entries.id` (assigned at insertion).
    pub id: i64,
    /// The owning snapshot (`scans.id`).
    pub scan_id: i64,
    /// Logical parent `entries.id`; `None` for the scan root.
    pub parent_id: Option<i64>,
    /// Full path, stored form (no `\\?\` prefix).
    pub path: String,
    /// Final path component (file or directory name).
    pub name: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    /// F-103 file class for files (`audio`, `video`, ...); `None` for dirs.
    pub file_class: Option<String>,
    /// Size in bytes; 0 for directories and unstatable entries.
    pub size: i64,
    /// Modification time (ISO-8601 UTC); `None` when unavailable.
    pub mtime: Option<String>,
    /// Depth below the scan root (root is 0).
    pub depth: i64,
}
