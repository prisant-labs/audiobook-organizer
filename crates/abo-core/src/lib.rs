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

pub mod db;
pub mod error;
pub mod ipc;
pub mod paths;
pub mod scan;
