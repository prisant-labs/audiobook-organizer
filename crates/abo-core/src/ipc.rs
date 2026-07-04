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
//!
//! Phase 5 (tauri-specta seam) adds the shell's IPC payloads here so the
//! command/event contract stays in the Tauri-free core: [`JobStarted`] (returned
//! by `scan_start`), [`DbStatus`] (returned by `db_status`, the wire form of the
//! startup [`crate::db::DbOpenOutcome`]), and the three job-event payloads
//! [`JobCompletedPayload`], [`JobFailedPayload`], and [`JobProgressPayload`].
//! The `#[tauri_specta::Event]` wrappers and `#[tauri::command]` annotations live
//! in `src-tauri`; only the plain payload shapes (serde + `specta::Type`) live
//! here, so the core never gains a tauri dependency (AC-3).
//!
//! Placement invariant (AC-4): every type that crosses the IPC boundary is
//! reachable through this module and derives `serde::Serialize`,
//! `serde::Deserialize`, and `specta::Type`. The payload structs are defined
//! here; [`AppError`] is defined in [`crate::error`] and re-exported here so a
//! Phase 5 consumer can name the whole IPC surface under `abo_core::ipc`. The
//! [`contract`] test instantiates a bound-checking generic over each of these
//! types, so a dropped derive fails THIS crate's `cargo test` with a named
//! error instead of surfacing as an opaque trait-bound failure inside Phase 5's
//! generated bindings.
//!
//! Deliberately NOT re-exported: [`crate::db::DbOpenOutcome`]. It is a
//! startup-internal Rust type (it carries a `PathBuf`) that the shell reads once
//! and maps to [`AppError::DbCorruptRecovered`] BEFORE anything crosses the
//! boundary; the error, not the outcome, is the wire type. It therefore does not
//! need serde/specta and is not part of the IPC contract.

use serde::{Deserialize, Serialize};

/// Re-export so the entire IPC surface (payloads plus the error type) is
/// reachable under `abo_core::ipc` (AC-4). Defined in [`crate::error`].
pub use crate::error::AppError;

/// The category of a [`ScanWarning`].
///
/// The two per-entry kinds ([`JunctionSkipped`](ScanWarningKind::JunctionSkipped)
/// and [`PermissionDenied`](ScanWarningKind::PermissionDenied)) share their
/// kebab-case wire strings with the corresponding [`AppError`] codes on purpose:
/// a warning is the collected-during-scan record of the same condition the error
/// taxonomy names, so a caller can key off one vocabulary. The scanner records
/// these as warnings (the scan runs to completion, AC-11) rather than raising
/// them as errors; the `AppError` variants remain for the per-entry error case.
/// The two path-length kinds are scan-level (FD-19): [`LongPathsDisabled`] when a
/// recorded path exceeds the legacy 260-char limit while Windows long-path
/// support is off, and [`NearMaxPathInterop`] for paths near the limit that
/// other Windows tools may still mishandle.
///
/// [`LongPathsDisabled`]: ScanWarningKind::LongPathsDisabled
/// [`NearMaxPathInterop`]: ScanWarningKind::NearMaxPathInterop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ScanWarningKind {
    /// A junction or reparse point was recorded but deliberately not followed
    /// (shares the `junction-skipped` wire string with [`AppError::JunctionSkipped`]).
    JunctionSkipped,
    /// An entry (or a subtree) could not be read because the OS denied access;
    /// it was recorded where possible and the scan continued (shares the
    /// `permission-denied` wire string with [`AppError::PermissionDenied`]).
    PermissionDenied,
    /// One or more recorded paths exceed the legacy 260-char Windows limit and
    /// long-path support is disabled; carries a how-to link (FD-19, AC-101.4).
    LongPathsDisabled,
    /// A recorded path is at or near the legacy 260-char limit; some Windows
    /// tools may not handle it even though this scan did (FD-19 interop note).
    NearMaxPathInterop,
}

/// A non-fatal condition recorded during a scan, for the caller (and, from
/// v0.4.0, the GUI) to surface. Structured now so the shape is stable before any
/// UI renders it (spec F-101: "add a warning record to the ScanSummary;
/// structure it; the GUI renders it in v0.4.0").
///
/// `path` is the stored, human-readable path the warning concerns (the first
/// offending path, for the scan-level [`ScanWarningKind::LongPathsDisabled`]).
/// `detail` is a family-safe sentence; `how_to` is an optional link to guidance
/// (populated for [`ScanWarningKind::LongPathsDisabled`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ScanWarning {
    /// The category of condition (see [`ScanWarningKind`]).
    pub kind: ScanWarningKind,
    /// The stored, human-readable path the warning concerns.
    pub path: String,
    /// A family-safe, human-readable explanation.
    pub detail: String,
    /// An optional link to guidance (populated for `long-paths-disabled`).
    pub how_to: Option<String>,
}

impl ScanWarning {
    /// Build a `junction-skipped` warning for a recorded junction/reparse point.
    pub fn junction_skipped(path: &std::path::Path) -> Self {
        Self {
            kind: ScanWarningKind::JunctionSkipped,
            path: path.to_string_lossy().into_owned(),
            detail: "This item is a junction or reparse point (a link to another \
                     location). It was recorded but not opened, so the scan cannot \
                     loop back on itself."
                .to_string(),
            how_to: None,
        }
    }

    /// Build a `permission-denied` warning for a subtree the OS refused to read.
    pub fn permission_denied(path: &std::path::Path) -> Self {
        Self {
            kind: ScanWarningKind::PermissionDenied,
            path: path.to_string_lossy().into_owned(),
            detail: "This item could not be read because Windows denied access. It \
                     was skipped and the rest of the scan continued."
                .to_string(),
            how_to: None,
        }
    }
}

/// The result of a completed [`crate::scan::run_scan`]: the metadata of the one
/// immutable `scans` row it wrote (F-105), plus the count of entries skipped
/// during the walk (permission-denied subtrees, unstatable entries; AC-11) and
/// the structured [`ScanWarning`] records collected during the scan (FD-19).
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
    /// Structured non-fatal conditions recorded during the scan (junctions
    /// skipped, permission-denied subtrees, long-path warnings). Empty for a
    /// clean scan. The GUI renders these from v0.4.0; this release logs them.
    pub warnings: Vec<ScanWarning>,
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

// ---- Phase 5 shell payloads (tauri-specta seam) ----

/// Returned by the `scan_start` command the instant a scan is accepted (F-104).
///
/// `scan_start` inserts a `running` `jobs` row, spawns the scan on the async
/// runtime, and returns this immediately; the caller then waits for the
/// [`JobCompletedPayload`] / [`JobFailedPayload`] event carrying the same
/// `job_id`. This decouples the long-running scan from the IPC call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobStarted {
    /// `jobs.id` of the row created for this scan; correlates the later
    /// `job:completed` / `job:failed` event back to this call.
    pub job_id: i64,
}

/// Returned by the `db_status` command: the wire form of the startup
/// [`crate::db::DbOpenOutcome`] (P2), reporting whether corrupt-DB recovery ran.
///
/// [`crate::db::DbOpenOutcome`] is a startup-internal Rust type (it carries a
/// `PathBuf`) and is deliberately NOT part of the IPC contract; the shell maps it
/// to this serde/specta payload once, at the boundary. `recovered` is true
/// exactly when the existing database was unreadable and was reset; when true,
/// `backup_path` is where the prior database was preserved (the human-readable
/// stored form), else `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DbStatus {
    /// True exactly when startup found a corrupt/unopenable database and reset it
    /// (see [`crate::db::DbOpenOutcome::Recovered`]).
    pub recovered: bool,
    /// Where the corrupt database was preserved, when `recovered`; else `None`.
    pub backup_path: Option<String>,
}

/// Payload of the `job:completed` event, emitted when a spawned scan finishes
/// successfully. Carries the originating `job_id` and the `scan_id` of the
/// snapshot written, so the tracer can read the entries back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobCompletedPayload {
    /// The `jobs.id` returned earlier by `scan_start`.
    pub job_id: i64,
    /// The `scans.id` of the snapshot the completed scan wrote.
    pub scan_id: i64,
}

/// Payload of the `job:failed` event, emitted when a spawned scan errors. Carries
/// the originating `job_id` and the stable machine `code` from
/// [`AppError::code`], so the frontend can key off the same codes the command
/// results use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobFailedPayload {
    /// The `jobs.id` returned earlier by `scan_start`.
    pub job_id: i64,
    /// The stable kebab-case error code (equals [`AppError::code`]).
    pub code: String,
}

/// Payload of the `job:progress` event.
///
/// Frozen NOW so the IPC contract is complete, even though the v0.1.0 spine
/// NEVER emits it: the spine scan is a single spawned unit with no progress
/// reporting or cancellation (this phase's brief). Wiring an emitter is a later
/// release's concern; pinning the shape here means adding progress later is an
/// additive change that does not perturb the frozen bindings surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobProgressPayload {
    /// The `jobs.id` this progress update belongs to.
    pub job_id: i64,
    /// Units of work completed so far.
    pub done: i64,
    /// Best-known total units, or `None` when the total is not yet known
    /// (an indeterminate phase).
    pub total_estimate: Option<i64>,
    /// A short human-readable label for the current step (for example a path or
    /// stage name).
    pub current_label: String,
}

#[cfg(test)]
mod contract {
    use super::*;

    /// Compile-time proof that a type carries the three derives tauri-specta
    /// requires of every IPC payload and return type (AC-4). This function has
    /// an empty body; its VALUE is its `where` clause. If any type below loses a
    /// derive, the instantiation in [`every_cross_boundary_type_is_ipc_ready`]
    /// stops compiling and the failure names the exact type, here in abo-core's
    /// test build, rather than deep inside the Phase 5 bindings generator.
    ///
    /// The deserialize bound is [`serde::de::DeserializeOwned`] (`for<'de>
    /// Deserialize<'de>`), which every owned payload satisfies and which is what
    /// `Result<T, AppError>` needs to round-trip across the boundary.
    fn assert_ipc_ready<T>()
    where
        T: serde::Serialize + serde::de::DeserializeOwned + specta::Type,
    {
    }

    /// One instantiation per cross-boundary type. Keep this list complete: every
    /// struct returned to or accepted from the frontend, plus [`AppError`],
    /// belongs here.
    #[test]
    fn every_cross_boundary_type_is_ipc_ready() {
        assert_ipc_ready::<ScanSummary>();
        assert_ipc_ready::<ScanWarning>();
        assert_ipc_ready::<ScanWarningKind>();
        assert_ipc_ready::<EntryRow>();
        assert_ipc_ready::<AppError>();
        // Phase 5 shell payloads (command returns + event payloads).
        assert_ipc_ready::<JobStarted>();
        assert_ipc_ready::<DbStatus>();
        assert_ipc_ready::<JobCompletedPayload>();
        assert_ipc_ready::<JobFailedPayload>();
        assert_ipc_ready::<JobProgressPayload>();
    }
}
