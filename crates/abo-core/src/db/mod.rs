//! SQLite pool (sqlx), WAL mode, and migration application.
//!
//! Stub for v0.1.0 spine, Phase 1. Phase 2 implements the `SqlitePool`
//! builder (`PRAGMA journal_mode=WAL`), applies `sqlx::migrate!()` on open,
//! and implements the corrupt-DB startup recovery path (AC-5 through AC-8).
