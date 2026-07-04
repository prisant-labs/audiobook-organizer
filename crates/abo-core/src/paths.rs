//! Platform path seam: resolves the application data directory.
//!
//! The one place that computes the app data location. On Windows this is
//! `%LOCALAPPDATA%\AudiobookOrganizer` (Local, never Roaming, never a
//! OneDrive-synced path): keeping the SQLite database out of a synced tree is
//! the structural defense against the WAL-sidecar corruption hazard (reference
//! architecture Section 4.10.c). On macOS it is
//! `~/Library/Application Support/AudiobookOrganizer` for CI-compiles honesty
//! only (macOS has no behavioral claims in v0.1.0).
//!
//! The database layer takes its directory as a PARAMETER
//! ([`crate::db::open_db`]); only the production caller (the shell) uses
//! [`app_data_dir`]. Tests pass temp dirs straight to `open_db`, so no
//! environment override is needed and nothing ever touches the real
//! `%LOCALAPPDATA%`.

use std::path::PathBuf;

/// The fixed app folder name under the per-user data root.
const APP_DIR: &str = "AudiobookOrganizer";

/// The per-user application data directory for Audiobook Organizer.
///
/// Windows: `%LOCALAPPDATA%\AudiobookOrganizer`. macOS:
/// `~/Library/Application Support/AudiobookOrganizer`. Other platforms
/// (Linux, used by CI test runners): `$XDG_DATA_HOME/AudiobookOrganizer` else
/// `~/.local/share/AudiobookOrganizer`. This function does not create the
/// directory; `open_db` creates whatever directory it is handed.
pub fn app_data_dir() -> PathBuf {
    resolve_data_dir()
}

#[cfg(target_os = "windows")]
fn resolve_data_dir() -> PathBuf {
    // LOCALAPPDATA is set on all supported Windows versions and is OUTSIDE the
    // roaming/OneDrive-synced tree. Fall back to the current directory only if
    // the environment is unexpectedly empty, so resolution never panics.
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(APP_DIR)
}

#[cfg(target_os = "macos")]
fn resolve_data_dir() -> PathBuf {
    // Compiles-only honesty: macOS is a build target in CI, not a behavioral
    // claim in v0.1.0.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Library")
        .join("Application Support")
        .join(APP_DIR)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn resolve_data_dir() -> PathBuf {
    // Linux / other: not a v1 target platform, but CI runs the test job on
    // ubuntu, so the crate must resolve a sane path there too. Respect
    // XDG_DATA_HOME, else ~/.local/share.
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(APP_DIR);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join(APP_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_ends_in_the_app_folder() {
        // Whatever the host OS, the resolved dir ends in the AudiobookOrganizer
        // app folder. Asserted without mutating the process environment.
        let dir = app_data_dir();
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some(APP_DIR),
            "resolved data dir must end in the AudiobookOrganizer app folder"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_resolves_under_localappdata() {
        // The Windows branch must hang off %LOCALAPPDATA% (never Roaming). We do
        // not mutate the process env (tests run concurrently); instead assert the
        // resolved dir sits under the live LOCALAPPDATA when it is present.
        if let Some(lad) = std::env::var_os("LOCALAPPDATA") {
            let expected = PathBuf::from(lad).join(APP_DIR);
            assert_eq!(app_data_dir(), expected);
        }
    }
}
