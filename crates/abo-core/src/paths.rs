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

use std::path::{Path, PathBuf};

/// The fixed app folder name under the per-user data root.
const APP_DIR: &str = "AudiobookOrganizer";

// ---- Windows extended-length (`\\?\`) path seam (F-101, FD-19) ----
//
// Storage invariant (product-wide, established here): paths PERSISTED to the
// database are stored WITHOUT the `\\?\` verbatim prefix, for readability and so
// the UI, logs, and later plan/apply stages all speak the human-facing path.
// The prefix is an OPEN-TIME concern only: the scanner normalizes the root to a
// `\\?\`-prefixed absolute path before walking so paths past the legacy 260-char
// `MAX_PATH` limit open (FD-19), then strips the prefix again before persisting.
// [`to_extended_length`] adds the prefix (Windows); [`strip_extended_length_prefix`]
// removes it. The two are inverse for real absolute paths.

/// Normalize `path` to an absolute form the walker can open past `MAX_PATH`.
///
/// Windows: returns the extended-length (`\\?\`) verbatim form via
/// [`std::fs::canonicalize`], which yields exactly that prefix and resolves any
/// `.`/`..` components. Resolving them is mandatory: the `\\?\` prefix disables
/// Win32 path normalization, so an unresolved `..` would be sent to the
/// filesystem literally and fail. If canonicalization fails (it should not - the
/// scanner validates the root exists first), the prefix is applied manually to
/// the absolute path as a best effort.
///
/// Other platforms: returns the absolute canonical path (no prefix concept).
///
/// The result is the WALK root only; stored paths are run back through
/// [`strip_extended_length_prefix`] so the `\\?\` prefix never reaches the DB.
#[cfg(windows)]
pub fn to_extended_length(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) => manual_extended_length(path),
    }
}

/// Non-Windows: no `\\?\` concept; return the absolute canonical path so stored
/// paths are stable and deterministic across a re-scan (the CI test job runs on
/// Linux). Falls back to the path as-is if it cannot be canonicalized.
#[cfg(not(windows))]
pub fn to_extended_length(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Best-effort manual `\\?\` prefixing used only if [`std::fs::canonicalize`]
/// fails. Handles both drive paths (`C:\x` -> `\\?\C:\x`) and UNC paths
/// (`\\server\share` -> `\\?\UNC\server\share`).
#[cfg(windows)]
fn manual_extended_length(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") {
        abs
    } else if let Some(rest) = s.strip_prefix(r"\\") {
        // UNC: \\server\share -> \\?\UNC\server\share
        PathBuf::from(format!(r"\\?\UNC\{rest}"))
    } else {
        PathBuf::from(format!(r"\\?\{s}"))
    }
}

/// Strip the extended-length (`\\?\`, and `\\?\UNC\`) verbatim prefix so a stored
/// path is the human-readable form. `\\?\C:\x` becomes `C:\x`;
/// `\\?\UNC\server\share` becomes `\\server\share`. A path without the prefix is
/// returned unchanged, so this is a safe no-op on non-Windows paths.
pub fn strip_extended_length_prefix(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest.to_string())
    } else {
        path.to_path_buf()
    }
}

/// Apply the Windows extended-length (`\\?\`) verbatim prefix to `path` WITHOUT
/// requiring it to exist - the executor's counterpart to [`to_extended_length`].
///
/// [`to_extended_length`] resolves the path with [`std::fs::canonicalize`], which
/// only works on a path that already exists: right for the scanner's walk root,
/// wrong for an executor operation's TARGET, which does not exist until the op
/// creates it (a move's destination, a `create_dir_all` target). This prefixes
/// the absolute form directly instead, so those paths still open past the legacy
/// 260-char `MAX_PATH` limit (FD-19). It folds no `.`/`..` components, which is
/// safe here: plan paths are already normalized absolute paths. This is the one
/// path-dialect helper the v0.5.0 executor's [`RealFs`](crate::exec::RealFs)
/// applies at every real filesystem call, keeping `\\?\` handling in this seam
/// rather than scattered through the operation logic.
///
/// Non-Windows (the CI test leg): returns the path unchanged; there is no `\\?\`
/// concept off Windows.
#[cfg(windows)]
pub fn to_extended_length_prefixed(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    manual_extended_length(path)
}

/// Non-Windows counterpart to [`to_extended_length_prefixed`]: no `\\?\` concept,
/// so the path is returned unchanged. Keeps every `RealFs` call site `cfg`-free.
#[cfg(not(windows))]
pub fn to_extended_length_prefixed(path: &Path) -> PathBuf {
    path.to_path_buf()
}

// ---- Free-space seam (F-404 cross-volume sizing) ----
//
// F-404 validation sizes cross-volume `copy+verify+delete` moves against the
// TARGET volume's free space. The pure validator ([`crate::plan::validate`])
// never calls the OS: it takes a [`crate::plan::validate::FreeSpace`] trait
// object, so tests inject scarcity deterministically. The one production
// implementation lives HERE (the platform seam), keeping the plan module free
// of any `cfg` (the CFG RULE) - platform reality is confined to this file, as
// it already is for the `\\?\` helpers above.

/// Bytes available to the calling user on the volume that `path` lives on, or
/// `None` if it cannot be determined (a non-Windows host, or the OS call
/// failed). `None` means "unknown", which validation treats conservatively as
/// "cannot prove insufficiency" rather than as zero.
///
/// Windows: calls `GetDiskFreeSpaceExW` (kernel32) on a NUL-terminated wide
/// path. The "available to caller" figure (not raw total-free) is the right
/// one because it already accounts for per-user quotas.
#[cfg(windows)]
pub fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;

    // GetDiskFreeSpaceExW wants a directory (or any path) on the volume; a
    // volume root like `E:\` is what callers pass. NUL-terminate the wide form.
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Minimal FFI: declaring the extern adds no crate dependency (kernel32 is
    // already linked by the standard library on the Windows target).
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lpDirectoryName: *const u16,
            lpFreeBytesAvailableToCaller: *mut u64,
            lpTotalNumberOfBytes: *mut u64,
            lpTotalNumberOfFreeBytes: *mut u64,
        ) -> i32;
    }

    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 buffer that outlives the
    // call; the three out-pointers are valid, aligned, and writable for the
    // duration of the call. The function reads none of the out params on entry.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut total_free,
        )
    };
    if ok != 0 {
        Some(free_to_caller)
    } else {
        None
    }
}

/// Non-Windows: no per-volume free-space query is wired (this crate has
/// behavioral claims on Windows only), so return `None` = unknown. Validation
/// then marks cross-volume moves without ever falsely blocking them on a host
/// where it cannot measure free space (the CI test leg runs on Linux).
#[cfg(not(windows))]
pub fn available_bytes(_path: &Path) -> Option<u64> {
    None
}

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

    #[test]
    fn strip_prefix_is_noop_without_prefix() {
        // A path that never had the verbatim prefix is returned unchanged, so
        // the helper is safe to run over every stored path on every platform.
        let p = Path::new("C:/Users/x/scan-basic");
        assert_eq!(strip_extended_length_prefix(p), p.to_path_buf());
        let rel = Path::new("relative/dir");
        assert_eq!(strip_extended_length_prefix(rel), rel.to_path_buf());
    }

    #[test]
    fn strip_prefix_removes_drive_verbatim() {
        // `\\?\C:\x` -> `C:\x` (the stored, human-readable form).
        let p = PathBuf::from(r"\\?\C:\Users\x\scan-basic\file.m4b");
        assert_eq!(
            strip_extended_length_prefix(&p),
            PathBuf::from(r"C:\Users\x\scan-basic\file.m4b")
        );
    }

    #[test]
    fn strip_prefix_removes_unc_verbatim() {
        // `\\?\UNC\server\share\x` -> `\\server\share\x`.
        let p = PathBuf::from(r"\\?\UNC\server\share\audiobooks\a.mp3");
        assert_eq!(
            strip_extended_length_prefix(&p),
            PathBuf::from(r"\\server\share\audiobooks\a.mp3")
        );
    }

    #[cfg(windows)]
    #[test]
    fn to_extended_length_round_trips_a_real_dir() {
        // On Windows the normalized walk root carries the `\\?\` prefix, and
        // stripping it yields a prefix-free absolute path (the storage invariant).
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let ext = to_extended_length(tmp.path());
        let ext_str = ext.as_os_str().to_string_lossy();
        assert!(
            ext_str.starts_with(r"\\?\"),
            "normalized root must be extended-length: {ext_str}"
        );
        let stored = strip_extended_length_prefix(&ext);
        assert!(
            !stored.as_os_str().to_string_lossy().starts_with(r"\\?\"),
            "stored path must not carry the verbatim prefix"
        );
    }
}
