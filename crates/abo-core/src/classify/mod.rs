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
