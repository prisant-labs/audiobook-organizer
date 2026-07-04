//! F-40x planning: turning a snapshot plus a ruleset into a validated,
//! reviewable operation list.
//!
//! v0.3.0 Phase 2 (this dispatch) lands [`templates`] (F-401, naming
//! templates and presets), pure string logic with no `cfg`-gating, no I/O,
//! and no database access, exactly like `crate::parse` (see that module's
//! doc comment for why: naming is filesystem-name SEMANTICS, not a
//! platform-specific concern, so a plan built on one host renders
//! byte-identically on another, matching the NFR Determinism requirement).
//! Later v0.3.0 phases add the rest of this module: `builder` (F-403, Phase
//! 4), `validate` (F-404, Phase 5), `disc`/`dupes::detect` (F-204/F-205/
//! F-701, Phase 6), and `export`/`report` (F-505/F-506, Phases 7-8).

pub mod templates;
