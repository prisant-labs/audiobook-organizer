//! `abo-core`: the Tauri-free engine crate for Audiobook Organizer.
//!
//! v0.1.0 spine, Phase 1 scaffolding. This crate has zero dependency on
//! `tauri`, even transitively (D-01, D-07 engine-first order); the core
//! purity CI gate runs `cargo tree -p abo-core -e normal` and fails the
//! build if `tauri` appears anywhere in it (AC-3).
//!
//! Every module below is a stub for now. Later phases of the v0.1.0
//! implementation plan (docs/internal/releases/v0.1.0-spine/implementation-plan.md)
//! fill them in:
//!   - Phase 2 fills `db` (sqlx pool, WAL, migrations, corrupt-DB recovery).
//!   - Phase 3 fills `scan` (walker, file typing, snapshot persistence).
//!   - Phase 4 fills `error` (`AppError` taxonomy) and `ipc` (payload
//!     structs), both deriving `serde` and `specta::Type`.

pub mod db;
pub mod error;
pub mod ipc;
pub mod paths;
pub mod scan;
