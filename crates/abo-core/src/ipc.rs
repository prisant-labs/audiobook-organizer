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
