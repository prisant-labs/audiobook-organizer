//! F-40x planning: turning a snapshot plus a ruleset into a validated,
//! reviewable operation list.
//!
//! v0.3.0 Phase 2 (this dispatch) lands [`templates`] (F-401, naming
//! templates and presets), pure string logic with no `cfg`-gating, no I/O,
//! and no database access, exactly like `crate::parse` (see that module's
//! doc comment for why: naming is filesystem-name SEMANTICS, not a
//! platform-specific concern, so a plan built on one host renders
//! byte-identically on another, matching the NFR Determinism requirement).
//! v0.3.0 Phase 4 adds [`builder`] (F-403): the ordered, immutable,
//! deterministic plan builder that consumes a snapshot + classifications +
//! merged fields + a ruleset and emits the campaign-grouped operation list.
//! v0.3.0 Phase 5 adds [`validate`] (F-404): the per-operation validation
//! backstop (verdicts + machine codes), the validate-before-insert persistence
//! path, and the F-405 approval state machine. v0.3.0 Phase 6 adds [`disc`]
//! (F-204 disc-structure detection, feeding the builder's `normalize-series`
//! pass) and `provenance` (the F-507 provenance report generator, exported
//! beside the plan); F-701 duplicate detection lands in `crate::dupes`. Later
//! v0.3.0 phases add `export`/`report` (F-505/F-506, Phases 7-8).

pub mod builder;
pub mod disc;
pub mod provenance;
pub mod templates;
pub mod validate;
