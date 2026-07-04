//! Folder classification and library health (v0.2.0 Phase 5): F-201 (folder
//! classification engine), F-202 (library health metrics), and F-203
//! (multi-book folder detection).
//!
//! This module turns a scanned+parsed snapshot into the truth the whole product
//! stands on: exactly one [`FolderClass`] per folder, each with a rule id and
//! evidence (the F-504 explainability surface, v0.4.0), plus aggregate health
//! metrics. It is pure logic - no I/O, no filesystem, nothing `cfg`-gated (the
//! CFG RULE) - consuming the F-303 merge ([`crate::parse::extract`]) and the
//! F-103 file typing ([`crate::scan::typing`]).
//!
//! Four submodules:
//!   - [`engine`] - F-201: the deterministic bottom-up rules and the nine
//!     classes; also the shelf-inheritance suppression and the pattern-2 vs
//!     pattern-9 content resolution the brief calls out.
//!   - [`multibook`] - F-203: several-complete-books detection with the disc /
//!     track / bonus false-positive guards.
//!   - [`metrics`] - F-202: per-class and per-problem counts and byte totals,
//!     every metric declaring its unit (FD-08).
//!   - [`run`] - v0.2.0 Phase 6, F-1001: the one I/O-touching function in this
//!     module. [`run::run_classify`] wraps the pure [`engine::classify`] with
//!     an activity-log append; `engine`, `metrics`, and `multibook` themselves
//!     stay pure, exactly as documented above.

pub mod engine;
pub mod metrics;
pub mod multibook;
pub mod run;

pub use engine::{classify, ClassifyInput, Evidence, FolderClass, FolderClassification};
pub use metrics::{health_metrics, ClassMetric, HealthMetrics, MetricUnit, ProblemMetric};
pub use multibook::{detect_multibook, BookFile, MultiBookVerdict};
pub use run::run_classify;

use std::path::Path;

use crate::ipc::EntryRow;
use crate::parse::extract::NodeKind;
use crate::scan::typing::classify_path;

/// Adapt a persisted snapshot ([`get_scan_entries`](crate::scan::get_scan_entries)
/// output) into the flat [`ClassifyInput`] slice [`classify`] and
/// [`health_metrics`] operate on.
///
/// This is the bridge from a stored scan (a live [`run_scan`](crate::scan::run_scan)
/// or a [`run_csv_import`](crate::scan::run_csv_import) snapshot) to the pure
/// classification pipeline. It is real product API, not test scaffolding: the
/// v0.4.0 shell classifies a persisted snapshot through exactly this path, and
/// the v0.2.0 Phase 7 real-library gate (baseline-delta report, FD-14 probe)
/// consumes it to classify the fresh scan and the 2026-03-25 CSV import
/// identically (a CSV snapshot is schema-identical downstream, F-102's contract).
///
/// The `i64` row ids map to `usize` unchanged (SQLite rowids are positive and
/// far within `usize` range). A file's [`FileClass`](crate::scan::typing::FileClass)
/// is RE-DERIVED from its final name component via
/// [`classify_path`](crate::scan::typing::classify_path) rather than parsed back
/// from the stored `file_class` string: the extension-only typing table is the
/// same one the walker persisted from, so the re-derived class equals the stored
/// one, and re-deriving needs no string-to-enum reverse mapping. Folders carry
/// no file class (`None`), matching the walker and CSV importer.
pub fn inputs_from_snapshot(rows: &[EntryRow]) -> Vec<ClassifyInput> {
    rows.iter()
        .map(|r| {
            let is_dir = r.kind == "dir";
            let kind = if is_dir {
                NodeKind::Folder
            } else {
                NodeKind::File
            };
            let file_class = if is_dir {
                None
            } else {
                Some(classify_path(Path::new(&r.name)))
            };
            ClassifyInput {
                id: r.id as usize,
                parent: r.parent_id.map(|p| p as usize),
                name: r.name.clone(),
                kind,
                file_class,
                size: r.size.max(0) as u64,
            }
        })
        .collect()
}

#[cfg(test)]
mod snapshot_bridge_tests {
    use super::*;
    use crate::scan::typing::FileClass;

    fn row(id: i64, parent_id: Option<i64>, name: &str, kind: &str, size: i64) -> EntryRow {
        EntryRow {
            id,
            scan_id: 1,
            parent_id,
            path: format!("root/{name}"),
            name: name.to_string(),
            kind: kind.to_string(),
            file_class: None,
            size,
            mtime: None,
            depth: 0,
        }
    }

    /// The bridge maps kind, parent linkage, size, and re-derives the file class
    /// from the name so the resulting inputs classify identically to a
    /// hand-built input set.
    #[test]
    fn maps_rows_to_classify_inputs() {
        let rows = vec![
            row(10, None, "Andy Weir - Project Hail Mary", "dir", 0),
            row(11, Some(10), "Project Hail Mary.m4b", "file", 175_000),
            row(12, Some(10), "cover.jpg", "file", 40_000),
        ];
        let inputs = inputs_from_snapshot(&rows);
        assert_eq!(inputs.len(), 3);

        let folder = &inputs[0];
        assert_eq!(folder.id, 10);
        assert_eq!(folder.parent, None);
        assert_eq!(folder.kind, NodeKind::Folder);
        assert_eq!(folder.file_class, None);

        let audio = &inputs[1];
        assert_eq!(audio.id, 11);
        assert_eq!(audio.parent, Some(10));
        assert_eq!(audio.kind, NodeKind::File);
        assert_eq!(audio.file_class, Some(FileClass::Audio));
        assert_eq!(audio.size, 175_000);

        assert_eq!(inputs[2].file_class, Some(FileClass::Image));
    }

    /// The bridge output flows straight into `classify` and `health_metrics`
    /// (the real Phase 7 consumption path), with no panic and coherent counts.
    #[test]
    fn bridged_inputs_classify_and_measure() {
        let rows = vec![
            row(1, None, "Sapiens by Yuval Noah Harari.m4b", "file", 180_000),
            row(2, None, "James Clear - Atomic Habits", "dir", 0),
            row(3, Some(2), "Atomic Habits.m4b", "file", 140_000),
        ];
        let inputs = inputs_from_snapshot(&rows);
        let cs = classify(&inputs);
        let m = health_metrics(&inputs, &cs);
        // One folder classified; the loose root audio counted as a loose book.
        assert_eq!(cs.len(), 1);
        let loose = m
            .problems
            .iter()
            .find(|p| p.problem == "loose-root-books")
            .unwrap();
        assert_eq!(loose.count, 1);
        assert_eq!(m.total_bytes, 320_000);
    }
}
