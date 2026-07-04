//! Live tree scanner (F-101), file typing (F-103), and snapshot persistence
//! (F-105).
//!
//! v0.1.0 spine, Phase 3. Three submodules:
//!   - [`walk`] - the `walkdir`-based traversal with extended-length (`\\?\`)
//!     open semantics, deterministic path-sorted output, and edge handling
//!     (permission-denied recorded and skipped; junctions recorded, not
//!     followed) (AC-9, AC-10, AC-11).
//!   - [`typing`] - the pure extension-to-class table, including the FD-17
//!     `video` class and the `.mp4 -> video` conservative default (AC-12).
//!   - [`persist`] - [`run_scan`] (writes one immutable snapshot: `scans` +
//!     `entries`) and [`get_scan_entries`] (reads a snapshot back) (AC-13).
//!
//! The engine here is Tauri-free and does no network I/O; the `src-tauri` shell
//! wires jobs and the `job:completed` event around [`run_scan`] in Phases 5-6.

pub mod exclude;
pub mod longpath;
pub mod persist;
pub mod typing;
pub mod walk;

pub use exclude::ExcludeSet;
pub use persist::{get_scan_entries, run_scan, run_scan_with_job, ScanOutcome};
