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
//! module docs). Phase 4 adds `plan::builder`: the F-403 plan builder that
//! turns a snapshot + classifications + merged fields + a ruleset into the
//! ordered, immutable, deterministic operation list grouped by the FD-26
//! campaign groups (see `plan::builder` module docs), also `cfg`-free.
//!
//! v0.3.0 Phase 7 adds `plan::export` (F-505: pure CSV/JSON/Markdown plan
//! export generators) and `reports` (F-1002: the thin, `cfg`-free
//! filesystem-writing step that lands those exports, plus the Phase 6
//! `plan::provenance` report, in the Reports folder beside the app data - see
//! `reports` module docs for the platform-seam and deterministic-naming
//! contract).
//!
//! v0.3.0 Phase 8 adds `plan::report` (F-506: the pure, self-contained dry-run
//! HTML report generator, with the Literata font baked in via `include_str!` so
//! the report makes zero network requests), plus the one impure full-chain
//! orchestration `plan::report::generate_and_report` (read snapshot -> classify
//! -> extract -> build -> detect duplicates -> validate -> persist -> export ->
//! render HTML), which writes every artifact through `reports` into the Reports
//! folder. The report's `reports::write_html_report` sibling lands the HTML
//! beside the F-505/F-507 exports.
//!
//! v0.5.0 Phase 1 adds `exec`: the F-607 virtual-filesystem seam. `exec::vfs`
//! defines the `Vfs` trait with `RealFs` (the one place that touches the real
//! filesystem, routing paths through the `paths` extended-length seam) and
//! `MemFs` (a disk-inert in-memory tree seeded from a snapshot); `exec` holds the
//! `Executor<V: Vfs>`, generic over the filesystem so a dry run and a Real apply
//! share one code path, plus the journal row shape (`JournalEntry`/`JournalPhase`)
//! the next phase persists. Operation dispatch is a deliberate skeleton this
//! phase (no mutation); a unit test greps the executor to prove it makes no direct
//! standard-library filesystem call (AC-1). No `cfg`-gating leaks past `RealFs`.

pub mod classify;
pub mod db;
pub mod dupes;
pub mod error;
pub mod exec;
pub mod ipc;
pub mod job;
pub mod parse;
pub mod paths;
pub mod plan;
pub mod reports;
pub mod ruleset;
pub mod scan;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;

// v0.2.0 Phase 7 adds `probe`: the FD-14 tag-quality probe (release gate G-08),
// a bounded, READ-ONLY sample of embedded audio tags measured against the
// folder-first parse. The MODULE compiles only under the `probe` Cargo feature
// and is never in the default build or the shell; it is a measurement instrument
// for the G-08 verdict, not a runtime feature. As of v0.4.0 Phase 3 its `lofty`
// dependency is no longer optional (lofty was promoted to a normal dependency for
// F-907 cover extraction, `scan::cover`), so `probe` gates only this module now;
// enabling covers does NOT enable the probe.
#[cfg(feature = "probe")]
pub mod probe;
