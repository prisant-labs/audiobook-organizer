//! Live tree traversal (F-101): a `walkdir` walk of a root that captures per
//! entry the fields the `entries` table stores, with Windows edge correctness as
//! the point of the phase.
//!
//! Design notes (the load-bearing invariants):
//!
//! - **Extended-length open (FD-19).** The caller passes a root already
//!   normalized to the `\\?\` verbatim form (see [`crate::paths::to_extended_length`]),
//!   so paths past the legacy 260-char `MAX_PATH` limit open. Every path is then
//!   run back through [`crate::paths::strip_extended_length_prefix`] before it
//!   lands in [`WalkedEntry::path`], so stored paths never carry the prefix.
//!
//! - **Determinism (NFR).** Entries are sorted by their stored path before being
//!   returned, so a re-scan of an unchanged tree yields an identical, stably
//!   ordered set. That order is also parent-before-child (a parent path is a
//!   strict prefix of each descendant), which the persistence layer relies on to
//!   assign `parent_id` in a single pass.
//!
//! - **Edge handling (AC-11, AC-101.2/101.3).** `follow_links(false)` means
//!   symlinks and directory junctions are never followed, so a junction loop
//!   terminates. Such a reparse point is still RECORDED (as a `dir` entry) and
//!   draws a `junction-skipped` [`ScanWarning`] - it just is not descended into.
//!   An explicit `FILE_ATTRIBUTE_REPARSE_POINT` check on Windows is the
//!   belt-and-suspenders that also stops descent into the rare non-symlink
//!   reparse directory. A permission-denied (or otherwise unstatable) subtree is
//!   recorded where possible, counted in [`WalkOutcome::skipped_count`], and
//!   draws a `permission-denied` [`ScanWarning`]; the walk always runs to
//!   completion, never aborting.
//!
//! - **Job model (F-104).** [`walk_with_job`] threads a [`JobContext`]: it polls
//!   cancellation at the boundary between entries only (never mid-entry, so no
//!   torn snapshot is possible) and reports monotonic progress after each
//!   recorded entry. It also honors an [`ExcludeSet`] of glob patterns, pruning
//!   matched entries (and, for directories, their subtrees). [`walk`] is the
//!   plain wrapper with no excludes and an inert context.
//!
//! - **Timestamps.** `mtime` is ISO-8601 UTC, whole-second precision (e.g.
//!   `2026-07-04T12:34:56Z`), hand-formatted so the core carries no chrono/time
//!   dependency (migration 0001 comment).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::ipc::ScanWarning;
use crate::job::{JobContext, ProgressUpdate};
use crate::paths::strip_extended_length_prefix;
use crate::scan::exclude::ExcludeSet;
use crate::scan::typing::{classify_path, FileClass};

/// Whether a walked entry is a file or a directory. Reparse points / junctions
/// (which are not followed) are recorded as [`EntryKind::Dir`] when they carry
/// the directory attribute (the junction case), else [`EntryKind::File`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory (or a not-followed directory reparse point / junction).
    Dir,
}

impl EntryKind {
    /// Stable string persisted in `entries.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryKind::File => "file",
            EntryKind::Dir => "dir",
        }
    }
}

/// One walked entry, in memory, before persistence. `path` is the stored form
/// (no `\\?\` prefix). `file_class` is `Some` for files and `None` for
/// directories. `size` is 0 for directories and for entries that could not be
/// stat'd. `mtime` is `None` when unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedEntry {
    /// Full path, stored form (no `\\?\` prefix).
    pub path: PathBuf,
    /// Final path component.
    pub name: String,
    /// File or directory.
    pub kind: EntryKind,
    /// F-103 class for files; `None` for directories.
    pub file_class: Option<FileClass>,
    /// Size in bytes (0 for directories / unstatable entries).
    pub size: u64,
    /// Modification time (ISO-8601 UTC) or `None` when unavailable.
    pub mtime: Option<String>,
    /// Depth below the scan root (root is 0).
    pub depth: usize,
}

/// Whether a walk ran to completion or was stopped early by a cancellation
/// request observed at a safe boundary (AC-104.2/104.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WalkStatus {
    /// The walk enumerated the whole (non-excluded) tree.
    #[default]
    Completed,
    /// A cancellation was observed at an entry boundary and the walk stopped.
    /// The entries collected so far are partial and the caller
    /// ([`crate::scan::run_scan_with_job`]) DISCARDS them (never persists).
    Cancelled,
}

/// The result of a walk: the deterministic, path-sorted entry list; the count of
/// entries/subtrees skipped because of errors (permission-denied, unstatable);
/// the structured [`ScanWarning`] records collected during the walk
/// (junction-skipped, permission-denied); and whether the walk completed or was
/// cancelled. The walk never aborts on a per-entry error; a bad root is rejected
/// by the caller (`run_scan`) before the walk starts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalkOutcome {
    /// Every recorded entry, sorted by stored path (parent-before-child).
    pub entries: Vec<WalkedEntry>,
    /// Entries/subtrees that could not be fully traversed (recorded where
    /// possible, then skipped); surfaced on `ScanSummary.skipped_count`.
    pub skipped_count: u64,
    /// Structured non-fatal conditions recorded during the walk: one
    /// `junction-skipped` per recorded junction/reparse point, one
    /// `permission-denied` per unreadable subtree. Long-path warnings are added
    /// later by the caller (they need the completed entry list and the OS
    /// setting). Empty for a clean walk.
    pub warnings: Vec<ScanWarning>,
    /// Whether the walk completed or was cancelled at a safe boundary.
    pub status: WalkStatus,
}

// Windows file-attribute bits used for the reparse-point / directory checks.
// Hardcoded (documented) so the crate needs no winapi/windows-sys direct dep;
// the values are stable Win32 constants. See MetadataExt::file_attributes.
#[cfg(windows)]
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

/// Walk `normalized_root` (already extended-length on Windows) and return the
/// sorted entries plus the skip count. No excludes, no cancellation, and no
/// progress: the plain traversal used by the fixture harness self-test and by
/// [`crate::scan::run_scan`]. Equivalent to [`walk_with_job`] with an empty
/// exclude set and an inert [`JobContext`].
///
/// The root itself is recorded as the depth-0 entry, so the returned tree is
/// self-contained and every non-root entry has a parent inside the set.
pub fn walk(normalized_root: &Path) -> WalkOutcome {
    walk_with_job(normalized_root, &ExcludeSet::empty(), &JobContext::inert())
}

/// Walk `normalized_root`, honoring exclude globs, cancellation, and progress.
///
/// This is the hardened F-101 / F-104 traversal:
///
/// - **Excludes (F-101 ruleset scope).** A non-root entry matching `excludes` is
///   not recorded; if it is a directory its whole subtree is pruned.
/// - **Junctions/reparse points (AC-101.2).** `follow_links(false)` means they
///   are never descended into (a junction loop terminates); each is RECORDED as
///   an entry AND draws a `junction-skipped` [`ScanWarning`].
/// - **Permission-denied (AC-101.3).** An unreadable subtree is counted in
///   `skipped_count` and draws a `permission-denied` [`ScanWarning`]; the walk
///   never aborts.
/// - **Cancellation (AC-104.2/104.3).** `ctx.is_cancelled()` is polled only at
///   the boundary between entries; when set, the walk stops with
///   [`WalkStatus::Cancelled`], never mid-entry.
/// - **Progress (AC-104.1).** After each recorded entry, `ctx.report` fires with
///   a monotonically non-decreasing `done` count, an unknown total, and the
///   current path.
pub fn walk_with_job(
    normalized_root: &Path,
    excludes: &ExcludeSet,
    ctx: &JobContext,
) -> WalkOutcome {
    let mut entries: Vec<WalkedEntry> = Vec::new();
    let mut skipped_count: u64 = 0;
    let mut warnings: Vec<ScanWarning> = Vec::new();
    let mut status = WalkStatus::Completed;

    let mut it = WalkDir::new(normalized_root)
        .follow_links(false)
        .into_iter();

    loop {
        // Safe cancellation boundary: BETWEEN entries only, so a cancel never
        // interrupts the recording of an entry mid-flight (AC-104.3).
        if ctx.is_cancelled() {
            status = WalkStatus::Cancelled;
            break;
        }

        let next = match it.next() {
            None => break,
            Some(item) => item,
        };
        match next {
            Ok(entry) => {
                let file_type = entry.file_type();

                // Excludes: drop a matched non-root entry (and prune its subtree
                // if it is a directory) before it is recorded. The root (depth 0)
                // is never excluded.
                if entry.depth() > 0 && excludes.is_excluded(normalized_root, entry.path()) {
                    if file_type.is_dir() {
                        it.skip_current_dir();
                    }
                    continue;
                }

                // walkdir metadata with follow_links(false) is symlink metadata,
                // so a junction reports its own attributes (reparse + directory),
                // not its target's. May fail under a permission deny; then we
                // still record the entry from its (readdir-provided) file type.
                let meta = entry.metadata().ok();

                #[cfg(windows)]
                let (is_reparse, dir_attr) = {
                    use std::os::windows::fs::MetadataExt;
                    match &meta {
                        Some(m) => {
                            let a = m.file_attributes();
                            (
                                a & FILE_ATTRIBUTE_REPARSE_POINT != 0,
                                a & FILE_ATTRIBUTE_DIRECTORY != 0,
                            )
                        }
                        None => (false, false),
                    }
                };
                #[cfg(not(windows))]
                let is_reparse = false;

                // A junction / symlink / reparse point: recorded, never followed.
                // On Windows `is_symlink()` is true for junctions and the reparse
                // attribute is the belt-and-suspenders; on other platforms a
                // symlink is the only reparse-like case.
                let is_link = file_type.is_symlink() || is_reparse;

                let kind = if file_type.is_file() {
                    EntryKind::File
                } else if file_type.is_dir() {
                    EntryKind::Dir
                } else {
                    // A symlink / reparse point (not followed). Classify by the
                    // directory attribute where known (junctions -> dir), else
                    // default to dir (directory junctions are the common case).
                    #[cfg(windows)]
                    let k = if dir_attr {
                        EntryKind::Dir
                    } else {
                        EntryKind::File
                    };
                    #[cfg(not(windows))]
                    let k = EntryKind::Dir;
                    k
                };

                let stored_path = strip_extended_length_prefix(entry.path());
                let name = stored_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| stored_path.to_string_lossy().into_owned());

                let (size, file_class) = match kind {
                    EntryKind::File => (
                        meta.as_ref().map(|m| m.len()).unwrap_or(0),
                        Some(classify_path(&stored_path)),
                    ),
                    EntryKind::Dir => (0, None),
                };

                let mtime = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok().map(system_time_to_iso8601));

                // A recorded junction/reparse point draws its warning (AC-101.2).
                if is_link {
                    warnings.push(ScanWarning::junction_skipped(&stored_path));
                }

                entries.push(WalkedEntry {
                    path: stored_path.clone(),
                    name,
                    kind,
                    file_class,
                    size,
                    mtime,
                    depth: entry.depth(),
                });

                // Progress at the entry boundary: done is the count recorded so
                // far (monotonically non-decreasing), total unknown on a first
                // walk, label is the current path (AC-104.1).
                ctx.report(ProgressUpdate {
                    done: entries.len() as u64,
                    total_estimate: None,
                    current_label: stored_path.to_string_lossy().into_owned(),
                });

                // Defensive descent guard for a directory reparse point that is
                // NOT a symlink (follow_links already stops symlinks/junctions).
                // Fires only for the exotic non-symlink reparse dir, so it never
                // interferes with a normal walk or the junction test (a junction
                // reports is_symlink, so file_type.is_dir() is false here).
                #[cfg(windows)]
                if is_reparse && file_type.is_dir() {
                    it.skip_current_dir();
                }
            }
            Err(err) => {
                // A subtree we could not enumerate (permission-denied read_dir,
                // an unstatable entry). walkdir has usually already yielded the
                // directory entry itself via its parent's listing, so the entry
                // is recorded; this Err just means its children are unreadable.
                // Record the skip AND a structured warning, then keep going - the
                // scan never aborts (AC-11, AC-101.3).
                skipped_count += 1;
                if let Some(path) = err.path() {
                    let stored = strip_extended_length_prefix(path);
                    warnings.push(ScanWarning::permission_denied(&stored));
                }
                tracing::warn!(
                    path = ?err.path(),
                    error = %err,
                    "scan: entry skipped (unreadable); continuing"
                );
            }
        }
    }

    // Determinism: stable, path-sorted order. PathBuf compares component-wise,
    // so a parent (a strict path prefix) always sorts before its descendants -
    // the property the persistence layer relies on for single-pass parent_id.
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    WalkOutcome {
        entries,
        skipped_count,
        warnings,
        status,
    }
}

/// Format a [`SystemTime`] as ISO-8601 UTC with whole-second precision, e.g.
/// `2026-07-04T12:34:56Z`. Times before the Unix epoch format with a negative
/// internal offset but still render a valid civil date.
pub(crate) fn system_time_to_iso8601(st: SystemTime) -> String {
    let unix_secs = match st.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    format_unix_secs(unix_secs)
}

/// The current time as an ISO-8601 UTC string (used for `started_at` /
/// `completed_at` on the `scans` row, and by the Phase 5 shell for the `jobs`
/// row so job timestamps share the scans' format). Public so the Tauri-free
/// shell can stamp its `jobs` rows without pulling a date crate; still
/// zero-tauri, zero-network.
pub fn now_iso8601_utc() -> String {
    system_time_to_iso8601(SystemTime::now())
}

/// Render Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
fn format_unix_secs(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs_of_day = unix_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3_600;
    let mm = (secs_of_day % 3_600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Howard Hinnant's `civil_from_days`: convert a count of days since the Unix
/// epoch (1970-01-01) into a `(year, month, day)` civil date. Pure integer
/// arithmetic, no lookup tables, valid across the proleptic Gregorian calendar -
/// the reason the core needs no date library.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_formats_known_instants() {
        // The Unix epoch and a well-known later instant, to lock the civil-date
        // math and the ISO-8601 rendering.
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
        // 1_700_000_000 = 2023-11-14T22:13:20Z.
        assert_eq!(format_unix_secs(1_700_000_000), "2023-11-14T22:13:20Z");
        // A leap-year day boundary: 2024-02-29T23:59:59Z = 1709251199.
        assert_eq!(format_unix_secs(1_709_251_199), "2024-02-29T23:59:59Z");
    }

    #[test]
    fn system_time_round_trips_through_epoch() {
        let st = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(system_time_to_iso8601(st), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn entry_kind_strings() {
        assert_eq!(EntryKind::File.as_str(), "file");
        assert_eq!(EntryKind::Dir.as_str(), "dir");
    }

    // ---- Job-model behavior (F-104), cross-platform ----

    use crate::job::{CancelFlag, JobContext, ProgressUpdate};
    use crate::scan::exclude::ExcludeSet;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    /// Build a small readable tree: `root/{dir0,dir1,dir2}/book.m4b`.
    fn small_tree() -> TempDir {
        let tree = TempDir::new().expect("tempdir");
        for i in 0..3 {
            let sub = tree.path().join(format!("dir{i}"));
            fs::create_dir(&sub).unwrap();
            fs::write(sub.join("book.m4b"), b"audio").unwrap();
        }
        tree
    }

    /// AC-104.1: progress `done` is monotonically non-decreasing across a walk,
    /// the final value equals the entry count, and the total estimate is unknown
    /// (None) on a first walk.
    #[test]
    fn progress_is_monotonic_with_unknown_total() {
        let tree = small_tree();
        let seen: Arc<Mutex<Vec<u64>>> = Arc::new(Mutex::new(Vec::new()));
        let totals_all_none = Arc::new(Mutex::new(true));

        let sink_seen = seen.clone();
        let sink_totals = totals_all_none.clone();
        let ctx = JobContext::new(
            CancelFlag::new(),
            Arc::new(move |u: ProgressUpdate| {
                sink_seen.lock().unwrap().push(u.done);
                if u.total_estimate.is_some() {
                    *sink_totals.lock().unwrap() = false;
                }
            }),
        );

        let normalized = crate::paths::to_extended_length(tree.path());
        let outcome = walk_with_job(&normalized, &ExcludeSet::empty(), &ctx);
        assert_eq!(outcome.status, WalkStatus::Completed);

        let reported = seen.lock().unwrap().clone();
        assert!(!reported.is_empty(), "progress must fire at least once");
        for pair in reported.windows(2) {
            assert!(
                pair[1] >= pair[0],
                "progress must be non-decreasing: {reported:?}"
            );
        }
        assert_eq!(
            *reported.last().unwrap(),
            outcome.entries.len() as u64,
            "final progress equals the entry count"
        );
        assert!(
            *totals_all_none.lock().unwrap(),
            "total_estimate must be None on a first walk"
        );
    }

    /// AC-104.2/104.3: a cancel observed at an entry boundary stops the walk with
    /// WalkStatus::Cancelled and fewer entries than a full walk would record.
    #[test]
    fn cancel_at_boundary_stops_the_walk() {
        let tree = small_tree();
        let normalized = crate::paths::to_extended_length(tree.path());

        // Full walk for the baseline count.
        let full = walk_with_job(&normalized, &ExcludeSet::empty(), &JobContext::inert());
        assert_eq!(full.status, WalkStatus::Completed);
        let full_count = full.entries.len();
        assert!(full_count >= 4, "tree should have several entries");

        // Cancel after the first reported entry.
        let cancel = CancelFlag::new();
        let sink_cancel = cancel.clone();
        let ctx = JobContext::new(
            cancel,
            Arc::new(move |_u: ProgressUpdate| sink_cancel.cancel()),
        );
        let cancelled = walk_with_job(&normalized, &ExcludeSet::empty(), &ctx);
        assert_eq!(cancelled.status, WalkStatus::Cancelled);
        assert!(
            cancelled.entries.len() < full_count,
            "a cancelled walk records fewer entries ({}) than a full walk ({full_count})",
            cancelled.entries.len()
        );
    }

    /// F-101 excludes: a matched directory is pruned with its whole subtree.
    #[test]
    fn excludes_prune_a_directory_subtree() {
        let tree = small_tree();
        let normalized = crate::paths::to_extended_length(tree.path());

        let excludes = ExcludeSet::compile(&["dir1".to_string()]).expect("compile");
        let outcome = walk_with_job(&normalized, &excludes, &JobContext::inert());

        assert!(
            !outcome.entries.iter().any(|e| e.name == "dir1"),
            "the excluded directory must not be recorded"
        );
        // Its child is pruned too; the other dirs' children remain.
        let books = outcome
            .entries
            .iter()
            .filter(|e| e.name == "book.m4b")
            .count();
        assert_eq!(books, 2, "only the two non-excluded dirs keep their book");
    }
}
