//! Deterministic synthetic-library fixture harness (v0.2.0 Phase 1).
//!
//! This module is the "single highest-leverage asset" of v0.2.0
//! (docs/internal/releases/v0.2.0-understanding/spec.md, Context section):
//! every golden test in the scanner-hardening, classification, and parsing
//! phases (Phases 2-5) materializes its input tree from
//! [`standard_library_manifest`] via [`generate`], rather than hand-building
//! ad hoc fixtures per test.
//!
//! Test-only surface, on purpose: this module compiles only when the crate
//! is built for tests (`cfg(test)`, which is automatic for `cargo test`) or
//! with the `fixtures` Cargo feature explicitly enabled (for integration
//! tests in `tests/*.rs`, which are separate compilation units from the
//! library's own unit tests; see the crate's `Cargo.toml` for the
//! feature-unification dev-dependency that turns `fixtures` on automatically
//! for `cargo test -p abo-core`). It never ships in the production shell
//! build, so it adds nothing to `abo-core`'s public API surface or its
//! zero-tauri / zero-network posture.
//!
//! Three submodules:
//!   - [`families`] - the [`FixtureFamily`] coverage taxonomy (AC-F2, AC-F3).
//!   - [`manifest`] - the declarative tree types and
//!     [`standard_library_manifest`], the one manifest this release ships.
//!   - [`generate`] - [`generate`] (the materializer) and [`GeneratedLibrary`]
//!     (the index every downstream test reads its expectations from).

pub mod families;
pub mod generate;
pub mod hostile;
pub mod manifest;

pub use families::FixtureFamily;
pub use hostile::{hostile_plan, HostileExpectation, HostilePlan};
pub use generate::{
    generate, FixtureError, GeneratedEntry, GeneratedKind, GeneratedLibrary, SkippedFixture,
};
pub use manifest::{
    standard_library_manifest, DupGroup, FileNode, FixtureManifest, FixtureNode, FolderNode,
    PackProvenance,
};
