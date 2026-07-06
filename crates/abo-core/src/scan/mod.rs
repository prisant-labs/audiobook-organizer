//! Live tree scanner (F-101), file typing (F-103), snapshot persistence
//! (F-105), and WizTree CSV import (F-102).
//!
//! v0.1.0 spine, Phase 3, plus v0.2.0 Phase 3. Four submodules:
//!   - [`walk`] - the `walkdir`-based traversal with extended-length (`\\?\`)
//!     open semantics, deterministic path-sorted output, and edge handling
//!     (permission-denied recorded and skipped; junctions recorded, not
//!     followed) (AC-9, AC-10, AC-11).
//!   - [`typing`] - the pure extension-to-class table, including the FD-17
//!     `video` class and the `.mp4 -> video` conservative default (AC-12).
//!   - [`persist`] - [`run_scan`] (writes one immutable snapshot: `scans` +
//!     `entries`) and [`get_scan_entries`] (reads a snapshot back) (AC-13).
//!   - [`cover`] - F-907 (v0.4.0) READ-ONLY cover extraction: the first embedded
//!     picture frame of a book's audio, or a `cover.jpg` / `folder.jpg` sidecar,
//!     served to the WebView as base64 over typed IPC (never a filesystem path,
//!     FD-29) and cached under `app_data/covers` (never under the library, D-09).
//!   - [`csv_import`] - [`csv_import::run_csv_import`] (F-102): an alternate
//!     snapshot source that parses a WizTree CSV export into the same
//!     `entries` schema, flagged `source = csv`, sharing `persist`'s insertion
//!     transaction so a CSV-imported snapshot is indistinguishable downstream
//!     from a live one (AC-102.1..102.3).
//!
//! The engine here is Tauri-free and does no network I/O; the `src-tauri` shell
//! wires jobs and the `job:completed` event around [`run_scan`] in Phases 5-6.

pub mod cover;
pub mod csv_import;
pub mod exclude;
pub mod longpath;
pub mod persist;
pub mod typing;
pub mod walk;

pub use cover::{get_cover, read_cover, CoverArt};
pub use csv_import::run_csv_import;
pub use exclude::ExcludeSet;
pub use persist::{get_scan_entries, run_scan, run_scan_with_job, ScanOutcome};
