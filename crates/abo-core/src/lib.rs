//! `abo-core`: the Tauri-free engine crate for Audiobook Organizer.
//!
//! v0.1.0 spine, Phase 1 scaffolding. This crate has zero dependency on
//! `tauri`, even transitively (D-01, D-07 engine-first order); the core
//! purity CI gate runs `cargo tree -p abo-core -e normal` and fails the
//! build if `tauri` appears anywhere in it (AC-3).
//!
//! Phases of the v0.1.0 implementation plan
//! (docs/internal/releases/v0.1.0-spine/implementation-plan.md) fill the
//! modules in:
//!   - Phase 2 (done) fills `db` (sqlx pool, WAL, migrations, corrupt-DB
//!     recovery) and `paths` (the app-data path seam), and seeds the `error`
//!     Storage family (`db-migration-failed`, `db-corrupt-recovered`).
//!   - Phase 3 fills `scan` (walker, file typing, snapshot persistence).
//!   - Phase 4 extends `error` with the Scan family and fills `ipc` (payload
//!     structs), all deriving `serde` and `specta::Type`.
//!
//! v0.2.0 Phase 1 adds `fixtures`: a deterministic synthetic-library
//! generator that every later v0.2.0 phase's golden tests build on (see
//! `fixtures` module docs). It is test-only surface (`cfg(any(test, feature
//! = "fixtures"))`), so it never touches the production build or the public
//! API the shell depends on.
//!
//! v0.2.0 Phase 4a adds `parse`: the F-301 pattern matcher set and F-302
//! noise strippers, pure string logic with no cfg-gating of any kind (see
//! `parse` module docs). Phase 4b (a later dispatch on the same branch)
//! fills in `parse::extract` (F-303 confidence merge) and `parse::normalize`
//! (F-304 filesystem-safe names) on top of it.
//!
//! v0.2.0 Phase 5 adds `classify`: F-201 (folder classification engine), F-202
//! (library health metrics), and F-203 (multi-book folder detection), pure
//! logic with no `cfg`-gating that consumes the `parse` merge and
//! `scan::typing` (see `classify` module docs). Its rule ids and evidence
//! become the F-504 explainability surface in v0.4.0.
//!
//! v0.2.0 Phase 6 adds `db::activity`: F-1001, the append-only activity log.
//! One `activity_records` row is appended per scan, CSV import, and classify
//! run (`scan::run_scan_with_job`, `scan::run_csv_import`,
//! `classify::run_classify`), success or failure, with no `cfg`-gating (see
//! `db::activity` module docs for why parse has no standalone hook here).
//!
//! v0.3.0 Phase 1 adds migration `0002_plan_and_rulesets.sql` plus
//! `db::plans` and `db::rulesets` (F-403/F-405/F-801 storage). Phase 2 adds
//! `plan::templates`: F-401 naming templates and presets, pure logic with no
//! `cfg`-gating, consuming `parse::ParsedFields` and
//! `parse::normalize::normalize_component` (see `plan` module docs). Phase 3
//! adds `ruleset`: the F-402 structure policies plus the F-801
//! schema-versioned ruleset model (validation, default seed, and the pure
//! policy helpers the plan builder calls), with no `cfg`-gating (see `ruleset`
//! module docs).

pub mod classify;
pub mod db;
pub mod error;
pub mod ipc;
pub mod job;
pub mod parse;
pub mod paths;
pub mod plan;
pub mod ruleset;
pub mod scan;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

// v0.2.0 Phase 7 adds `probe`: the FD-14 tag-quality probe (release gate G-08),
// a bounded, READ-ONLY sample of embedded audio tags measured against the
// folder-first parse. It compiles only under the `probe` Cargo feature (which
// also pulls the optional `lofty` dependency); it is never in the default build,
// never in the shell, and never on any product code path this release ships. It
// is a measurement instrument for the G-08 verdict, not a runtime feature
// (embedded-tag extraction as a real feature is F-1101, deferred past v1 per
// D-15/FD-03).
#[cfg(feature = "probe")]
pub mod probe;
