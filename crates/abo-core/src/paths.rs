//! Platform path seam: resolves the application data directory.
//!
//! On Windows this is `%LOCALAPPDATA%\AudiobookOrganizer` (Local, never
//! Roaming, never a OneDrive-synced path). The macOS branch returns
//! `~/Library/Application Support/AudiobookOrganizer` for CI-compiles
//! honesty only (macOS has no behavioral claims in v0.1.0).
//!
//! Stub for v0.1.0 spine, Phase 1. Phase 2 implements the real resolution
//! function and the corrupt-DB recovery path (`corrupt-backups\`).
