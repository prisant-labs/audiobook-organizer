//! Windows long-path reality (FD-19, AC-101.4): detect whether long-path
//! support is enabled and record structured warnings for paths at or over the
//! legacy 260-char limit.
//!
//! The scanner opens paths past `MAX_PATH` via extended-length (`\\?\`)
//! semantics regardless of the OS setting, so a long path still SCANS. The
//! warnings here are advisory: they tell the user (via the GUI, from v0.4.0)
//! that other Windows tools (Explorer, most non-extended-length APIs) may
//! mishandle those same paths, and - when long-path support is disabled - how to
//! turn it on.
//!
//! Detection reads `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\
//! LongPathsEnabled`. A value of `1` means enabled; anything else (0, absent, or
//! an unreadable key) is treated as NOT enabled, so the how-to warning fires
//! whenever a genuinely over-limit path was recorded. That is the conservative
//! choice: the warning only ever appears when there is a real over-limit path to
//! warn about, so a false "disabled" reading costs at most one advisory note.

use crate::ipc::{ScanWarning, ScanWarningKind};
use crate::scan::walk::WalkedEntry;

/// The legacy Windows `MAX_PATH` limit (including the terminating NUL in the
/// Win32 headers; used here as the plain character threshold for the warning
/// heuristic).
pub const MAX_PATH_LEGACY: usize = 260;

/// Paths of at least this length (up to and including [`MAX_PATH_LEGACY`]) draw
/// the near-limit interop warning: close enough that a small addition by another
/// tool would push them over.
pub const NEAR_LIMIT_THRESHOLD: usize = 248;

/// The guidance link surfaced with a [`ScanWarningKind::LongPathsDisabled`]
/// warning.
pub const LONG_PATHS_HOWTO: &str =
    "https://learn.microsoft.com/windows/win32/fileio/maximum-file-path-limitation";

/// Whether Windows long-path support is enabled on this host.
///
/// Non-Windows targets have no `MAX_PATH` concept, so this is always `true`
/// (no warning is ever warranted). On Windows it reads the registry value; see
/// the module docs for the fallback policy.
#[cfg(windows)]
pub fn long_paths_enabled() -> bool {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.open_subkey(r"SYSTEM\CurrentControlSet\Control\FileSystem") {
        Ok(key) => match key.get_value::<u32, _>("LongPathsEnabled") {
            Ok(value) => value == 1,
            // Absent or wrong-typed value: treat as disabled (conservative).
            Err(_) => false,
        },
        // Cannot open the key at all: treat as disabled (conservative).
        Err(_) => false,
    }
}

/// Non-Windows: no `MAX_PATH`, so long paths are never a problem here.
#[cfg(not(windows))]
pub fn long_paths_enabled() -> bool {
    true
}

/// The character length of a stored path, as the warning heuristic measures it.
fn path_len(entry: &WalkedEntry) -> usize {
    entry.path.as_os_str().to_string_lossy().chars().count()
}

/// Build the FD-19 long-path warnings for a completed walk (AC-101.4).
///
/// A pure function of `(enabled, entries)` so it is unit-testable without a real
/// registry or a real >260-char tree:
///
/// - When `!enabled` and at least one recorded path exceeds [`MAX_PATH_LEGACY`],
///   emit exactly ONE [`ScanWarningKind::LongPathsDisabled`] warning (the
///   setting is global): its `detail` states how many paths are over the limit,
///   its `path` is the first offending path as an example, and it carries the
///   how-to link.
/// - Every path in `[NEAR_LIMIT_THRESHOLD, MAX_PATH_LEGACY]` (near but not over)
///   draws a per-path [`ScanWarningKind::NearMaxPathInterop`] note, regardless of
///   the setting: those are the interop-risk paths other tools may mishandle.
///
/// A path OVER the limit is covered by the single disabled-warning (when the
/// setting is off) and is not also emitted as a near-limit note, so the two
/// bands do not double-report the same path.
pub fn long_path_warnings(enabled: bool, entries: &[WalkedEntry]) -> Vec<ScanWarning> {
    let mut warnings = Vec::new();

    let over_limit: Vec<&WalkedEntry> = entries
        .iter()
        .filter(|e| path_len(e) > MAX_PATH_LEGACY)
        .collect();

    if !enabled {
        if let Some(first) = over_limit.first() {
            warnings.push(ScanWarning {
                kind: ScanWarningKind::LongPathsDisabled,
                path: first.path.to_string_lossy().into_owned(),
                detail: format!(
                    "{} path(s) exceed the {}-character legacy Windows limit and \
                     long-path support is disabled. They were scanned, but other \
                     Windows tools may not be able to open them. Enabling long \
                     paths is recommended.",
                    over_limit.len(),
                    MAX_PATH_LEGACY
                ),
                how_to: Some(LONG_PATHS_HOWTO.to_string()),
            });
        }
    }

    for entry in entries {
        let len = path_len(entry);
        if (NEAR_LIMIT_THRESHOLD..=MAX_PATH_LEGACY).contains(&len) {
            warnings.push(ScanWarning {
                kind: ScanWarningKind::NearMaxPathInterop,
                path: entry.path.to_string_lossy().into_owned(),
                detail: format!(
                    "This path is {len} characters, near the {MAX_PATH_LEGACY}-character \
                     legacy Windows limit. Some Windows tools may not handle it."
                ),
                how_to: None,
            });
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::walk::{EntryKind, WalkedEntry};
    use std::path::PathBuf;

    fn entry_with_path_len(len: usize) -> WalkedEntry {
        // Build a path whose stored-string length is exactly `len`.
        let path = "a".repeat(len);
        WalkedEntry {
            path: PathBuf::from(&path),
            name: "a".to_string(),
            kind: EntryKind::File,
            file_class: None,
            size: 0,
            mtime: None,
            depth: 1,
        }
    }

    #[test]
    fn disabled_and_over_limit_emits_one_howto_warning() {
        let entries = vec![
            entry_with_path_len(300),
            entry_with_path_len(301),
            entry_with_path_len(10),
        ];
        let warnings = long_path_warnings(false, &entries);
        let disabled: Vec<_> = warnings
            .iter()
            .filter(|w| w.kind == ScanWarningKind::LongPathsDisabled)
            .collect();
        assert_eq!(disabled.len(), 1, "exactly one global disabled warning");
        assert!(
            disabled[0].detail.contains('2'),
            "counts the 2 over-limit paths"
        );
        assert!(disabled[0].how_to.is_some(), "carries a how-to link");
    }

    #[test]
    fn enabled_and_over_limit_emits_no_howto_warning() {
        let entries = vec![entry_with_path_len(300)];
        let warnings = long_path_warnings(true, &entries);
        assert!(
            !warnings
                .iter()
                .any(|w| w.kind == ScanWarningKind::LongPathsDisabled),
            "no disabled warning when long paths are enabled"
        );
    }

    #[test]
    fn near_limit_path_emits_interop_warning_regardless_of_setting() {
        let entries = vec![entry_with_path_len(250)];
        for enabled in [true, false] {
            let warnings = long_path_warnings(enabled, &entries);
            assert!(
                warnings
                    .iter()
                    .any(|w| w.kind == ScanWarningKind::NearMaxPathInterop),
                "near-limit path must warn (enabled={enabled})"
            );
        }
    }

    #[test]
    fn over_limit_is_not_double_reported_as_near_limit() {
        let entries = vec![entry_with_path_len(300)];
        let warnings = long_path_warnings(false, &entries);
        assert!(
            !warnings
                .iter()
                .any(|w| w.kind == ScanWarningKind::NearMaxPathInterop),
            "an over-limit path is the disabled warning's concern, not a near-limit note"
        );
    }

    #[test]
    fn clean_tree_yields_no_warnings() {
        let entries = vec![entry_with_path_len(10), entry_with_path_len(100)];
        assert!(long_path_warnings(false, &entries).is_empty());
        assert!(long_path_warnings(true, &entries).is_empty());
    }
}
