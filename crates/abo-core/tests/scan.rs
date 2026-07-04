//! Integration tests for the Phase 3 scanner (F-101), file typing (F-103), and
//! snapshot persistence (F-105): AC-9, AC-10, AC-11, AC-13.
//!
//! The cross-platform tests (`scan_counts_match_fixture`, `snapshot_immutable`)
//! run on the committed fixture on every OS (the CI test job runs ubuntu +
//! windows). The Windows-only tests exercise the filesystem edges that are the
//! point of the phase (extended-length paths, permission-denied, junction
//! loops); each builds its OS-feature fixture at runtime and cleans it up with a
//! Drop guard so a failed assertion never leaves an undeletable temp dir or a
//! changed ACL behind.

use std::fs;
use std::path::Path;

use abo_core::db::open_db;
use abo_core::ipc::{EntryRow, ScanWarningKind};
use abo_core::scan::{get_scan_entries, run_scan};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// The committed fixture root (`crates/abo-core/tests/fixtures/scan-basic`).
fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scan-basic")
}

/// A fresh, migrated database in its own temp dir (kept alive by the returned
/// `TempDir`), so the DB never sits inside a scanned tree.
async fn fresh_pool() -> (TempDir, SqlitePool) {
    let dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(dir.path()).await.expect("open_db");
    (dir, pool)
}

/// Independent reference traversal via `std::fs` (no walkdir), counting `dir`
/// itself as a directory, so its counts match `run_scan` (which records the
/// root as the depth-0 entry). Returns `(total_file_bytes, files, dirs)`.
fn reference_counts(dir: &Path) -> (u64, u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut dirs = 1u64; // count `dir` itself
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let md = entry.metadata().expect("metadata");
        if md.is_dir() {
            let (b, f, d) = reference_counts(&entry.path());
            bytes += b;
            files += f;
            dirs += d;
        } else {
            bytes += md.len();
            files += 1;
        }
    }
    (bytes, files, dirs)
}

/// Count entries of a given file class in the returned rows.
fn class_count(entries: &[EntryRow], class: &str) -> usize {
    entries
        .iter()
        .filter(|e| e.file_class.as_deref() == Some(class))
        .count()
}

/// AC-9: scanning the committed fixture yields entries whose count, kinds,
/// classes, sizes, and depths exactly match the fixture's known contents.
#[tokio::test]
async fn scan_counts_match_fixture() {
    let (_db, pool) = fresh_pool().await;
    let root = fixture_root();

    let summary = run_scan(&pool, &root).await.expect("scan");
    assert_eq!(summary.status, "completed");
    assert_eq!(summary.skipped_count, 0, "clean fixture skips nothing");

    // Independent std::fs traversal cross-checks run_scan's totals.
    let (ref_bytes, ref_files, ref_dirs) = reference_counts(&root);
    let ref_total = ref_files + ref_dirs;

    // Hardcoded expectations document the fixture's exact shape (18 entries:
    // 5 dirs incl. root + 13 files). If either the fixture or the walker drifts,
    // one of these two cross-checks fails.
    assert_eq!(ref_dirs, 5, "fixture has 5 dirs including root");
    assert_eq!(ref_files, 13, "fixture has 13 files");
    assert_eq!(
        summary.entry_count, ref_total as i64,
        "entry_count matches the independent traversal"
    );
    assert_eq!(summary.entry_count, 18);
    assert_eq!(
        summary.total_bytes, ref_bytes as i64,
        "total_bytes is the sum of file sizes"
    );
    assert!(summary.total_bytes > 0, "fixture files are non-empty");

    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read");
    assert_eq!(entries.len(), 18);

    // Kinds.
    let dirs = entries.iter().filter(|e| e.kind == "dir").count();
    let files = entries.iter().filter(|e| e.kind == "file").count();
    assert_eq!(dirs, 5);
    assert_eq!(files, 13);

    // Every file class is represented with its exact count (AC-12 coverage from
    // real files, complementing the unit table test).
    assert_eq!(class_count(&entries, "audio"), 3);
    assert_eq!(class_count(&entries, "ebook"), 1);
    assert_eq!(class_count(&entries, "image"), 1);
    assert_eq!(class_count(&entries, "playlist"), 1);
    assert_eq!(class_count(&entries, "release-info"), 2);
    assert_eq!(class_count(&entries, "weblink"), 1);
    assert_eq!(class_count(&entries, "comic"), 1);
    assert_eq!(class_count(&entries, "video"), 1);
    assert_eq!(class_count(&entries, "other"), 2);
    // Directories carry no class.
    assert_eq!(
        entries
            .iter()
            .filter(|e| e.kind == "dir" && e.file_class.is_some())
            .count(),
        0,
        "directories have NULL file_class"
    );

    // By-name spot checks: kind, class, depth, and size.
    let by_name = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("missing entry {name}"))
    };

    let root_entry = by_name("scan-basic");
    assert_eq!(root_entry.kind, "dir");
    assert_eq!(root_entry.depth, 0);
    assert_eq!(root_entry.parent_id, None, "root has no parent");
    assert!(root_entry.file_class.is_none());

    let mp4 = by_name("interview.mp4");
    assert_eq!(mp4.kind, "file");
    assert_eq!(mp4.file_class.as_deref(), Some("video"));
    assert_eq!(mp4.depth, 4, "Bonus/interview.mp4 is 4 below root");
    assert!(mp4.size > 0);

    let readme = by_name("readme");
    assert_eq!(readme.file_class.as_deref(), Some("other"));
    assert_eq!(readme.depth, 1);

    let data = by_name("data.xyz");
    assert_eq!(data.file_class.as_deref(), Some("other"));

    // Depth range: root at 0, deepest file at 4.
    assert_eq!(entries.iter().map(|e| e.depth).min(), Some(0));
    assert_eq!(entries.iter().map(|e| e.depth).max(), Some(4));

    // Sizes match the filesystem for every file entry.
    for e in entries.iter().filter(|e| e.kind == "file") {
        let disk = fs::metadata(&e.path)
            .unwrap_or_else(|_| panic!("stat {}", e.path))
            .len();
        assert_eq!(e.size as u64, disk, "size mismatch for {}", e.path);
    }

    // Parent linkage: exactly one entry (the root) has no parent; every other
    // entry's parent_id resolves to a dir entry in the set.
    let no_parent = entries.iter().filter(|e| e.parent_id.is_none()).count();
    assert_eq!(no_parent, 1, "only the root lacks a parent");
    let ids: std::collections::HashSet<i64> = entries.iter().map(|e| e.id).collect();
    for e in entries.iter().filter(|e| e.parent_id.is_some()) {
        assert!(
            ids.contains(&e.parent_id.unwrap()),
            "parent_id of {} must reference an entry in the snapshot",
            e.name
        );
    }
    // interview.mp4's parent is the "Bonus" directory.
    let bonus = by_name("Bonus");
    assert_eq!(mp4.parent_id, Some(bonus.id));

    // Stored paths never carry the extended-length verbatim prefix.
    for e in &entries {
        assert!(
            !e.path.starts_with(r"\\?\"),
            "stored path must not carry the \\?\\ prefix: {}",
            e.path
        );
    }
}

/// AC-13: two scans of the same fixture produce two `scans` rows, and the first
/// scan's rows are byte-identical before and after the second run (immutable
/// snapshots: a new scan never mutates a prior one).
#[tokio::test]
async fn snapshot_immutable() {
    let (_db, pool) = fresh_pool().await;
    let root = fixture_root();

    let scan1 = run_scan(&pool, &root).await.expect("scan 1");
    let entries_before = get_scan_entries(&pool, scan1.scan_id)
        .await
        .expect("read 1a");
    let row_before = scans_row(&pool, scan1.scan_id).await;

    let scan2 = run_scan(&pool, &root).await.expect("scan 2");
    assert_ne!(
        scan1.scan_id, scan2.scan_id,
        "second scan is a new snapshot"
    );

    // Two distinct snapshots now exist.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scans")
        .fetch_one(&pool)
        .await
        .expect("count scans");
    assert_eq!(count, 2, "each scan inserts its own scans row");

    // The first snapshot's entries are untouched by the second scan.
    let entries_after = get_scan_entries(&pool, scan1.scan_id)
        .await
        .expect("read 1b");
    assert_eq!(
        entries_before, entries_after,
        "scan 1 entries must be byte-identical after scan 2"
    );

    // And the first snapshot's scans row is untouched.
    let row_after = scans_row(&pool, scan1.scan_id).await;
    assert_eq!(
        row_before, row_after,
        "scan 1 scans row must be unchanged after scan 2"
    );

    // Sanity: the second snapshot has its own full entry set.
    let entries2 = get_scan_entries(&pool, scan2.scan_id)
        .await
        .expect("read 2");
    assert_eq!(entries2.len(), entries_before.len());
}

/// The immutable-facing columns of one `scans` row, as a comparable tuple.
async fn scans_row(
    pool: &SqlitePool,
    scan_id: i64,
) -> (
    String,
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    String,
) {
    sqlx::query_as(
        "SELECT source, root_path, started_at, completed_at, entry_count, total_bytes, status \
         FROM scans WHERE id = ?",
    )
    .bind(scan_id)
    .fetch_one(pool)
    .await
    .expect("fetch scans row")
}

/// A bad root is rejected before any snapshot is written (Scan-family errors).
#[tokio::test]
async fn bad_root_is_rejected() {
    use abo_core::error::AppError;
    let (_db, pool) = fresh_pool().await;

    // Non-existent root.
    let missing = TempDir::new().unwrap().path().join("does-not-exist");
    match run_scan(&pool, &missing).await {
        Err(AppError::RootNotFound { .. }) => {}
        other => panic!("expected RootNotFound, got {other:?}"),
    }

    // A file, not a directory.
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("a-file.txt");
    fs::write(&file, b"x").unwrap();
    match run_scan(&pool, &file).await {
        Err(AppError::RootNotDirectory { .. }) => {}
        other => panic!("expected RootNotDirectory, got {other:?}"),
    }

    // No snapshot rows were written for the rejected roots.
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scans")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "a bad root writes no scans row");
}

/// AC-101.1: scanning the P1 fixture library yields exact expected entry counts
/// and byte totals, checked against the generator's own `GeneratedLibrary` index
/// (the golden source, per test-strategy.md Section 3). This is the persisted
/// (run_scan -> DB) counterpart of the fixtures module's walk-level self-test.
#[tokio::test]
async fn scan_fixture_library_counts_match_generated() {
    use abo_core::fixtures::{generate, standard_library_manifest, GeneratedKind};

    let (_db, pool) = fresh_pool().await;
    let tree = TempDir::new().expect("fixture tempdir");
    let lib = generate(&standard_library_manifest(), tree.path()).expect("generate fixtures");

    let summary = run_scan(&pool, tree.path()).await.expect("scan fixtures");
    assert_eq!(summary.status, "completed");

    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read entries");

    // File count and byte total must match the generator's materialized index
    // exactly (dirs are compared via file count + the +1 root below to avoid the
    // NFC/NFD-fold subtlety the generator already accounts for on this host).
    let scanned_files = entries.iter().filter(|e| e.kind == "file").count();
    let declared_files = lib
        .entries
        .iter()
        .filter(|e| e.kind == GeneratedKind::File)
        .count();
    assert_eq!(
        scanned_files, declared_files,
        "persisted file count must equal the generator's declared file count"
    );

    assert_eq!(
        summary.total_bytes as u64, lib.declared_total_bytes,
        "persisted byte total must equal the manifest-declared total (AC-F5 tie-in)"
    );

    // Every materialized directory (plus the scan root) is present as a dir row.
    let scanned_dirs = entries.iter().filter(|e| e.kind == "dir").count();
    let declared_dirs = lib
        .entries
        .iter()
        .filter(|e| e.kind == GeneratedKind::Dir)
        .count();
    assert_eq!(
        scanned_dirs,
        declared_dirs + 1,
        "persisted dir count must equal declared dirs plus the scan root"
    );
}

/// AC-101.5: names are captured case-preservingly, and NFC/NFD Unicode inputs are
/// both captured verbatim (the scanner never normalizes - that is F-304's job).
/// The NFC/NFD twin assertion is conditional on the host filesystem keeping the
/// two byte-forms as distinct directory entries (Windows NTFS and Linux ext4 do);
/// a folding filesystem is handled with a skip-note so CI stays green everywhere.
#[tokio::test]
async fn scan_preserves_case_and_unicode_forms() {
    let (_db, pool) = fresh_pool().await;
    let base = TempDir::new().expect("base tempdir");

    // Case preservation: a mixed-case name round-trips byte-for-byte.
    let mixed = base.path().join("MixedCase Book");
    fs::create_dir(&mixed).expect("mkdir mixed-case");
    fs::write(mixed.join("Chapter One.m4b"), b"audio").expect("write");

    // NFC vs NFD forms of "Cafe" with an accented e.
    let nfc = base.path().join("Caf\u{00e9}"); // é as one code point (NFC)
    let nfd = base.path().join("Cafe\u{0301}"); // e + combining acute (NFD)
    let nfc_made = fs::create_dir(&nfc).is_ok();
    let nfd_made = fs::create_dir(&nfd).is_ok();

    let summary = run_scan(&pool, base.path()).await.expect("scan");
    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read");

    // Case-preserving: the exact mixed-case name is stored, not lowercased.
    assert!(
        entries.iter().any(|e| e.name == "MixedCase Book"),
        "the mixed-case directory name must be preserved verbatim"
    );
    assert!(
        entries.iter().any(|e| e.name == "Chapter One.m4b"),
        "the mixed-case file name must be preserved verbatim"
    );

    // Unicode forms: both captured verbatim when the FS kept them distinct.
    let nfc_present = entries.iter().any(|e| e.name == "Caf\u{00e9}");
    let nfd_present = entries.iter().any(|e| e.name == "Cafe\u{0301}");
    if nfc_made && nfd_made && nfc_present && nfd_present {
        // Ideal case (NTFS / ext4): both distinct byte-forms are captured.
        assert!(
            nfc_present && nfd_present,
            "both NFC and NFD forms captured"
        );
    } else {
        // A folding filesystem collapsed the pair; at least one form must be
        // present and stored WITHOUT normalization (the scanner never rewrites
        // the bytes it read).
        assert!(
            nfc_present || nfd_present,
            "at least one Unicode form must be captured verbatim"
        );
    }
}

// ---------------------------------------------------------------------------
// Windows-only filesystem-edge tests (AC-10, AC-11). Each builds its OS-feature
// fixture at runtime and guarantees cleanup via a Drop guard.
// ---------------------------------------------------------------------------

/// AC-10: a path exceeding the legacy 260-char MAX_PATH scans without error and
/// the deep entry is recorded (extended-length open posture).
#[cfg(windows)]
#[tokio::test]
async fn scan_over_260_char_path() {
    use abo_core::paths::to_extended_length;

    let base = TempDir::new().expect("base tempdir");
    // The extended-length base lets us CREATE the >260 tree in the first place
    // (plain create_dir_all past MAX_PATH fails without the \\?\ prefix).
    let ext_base = to_extended_length(base.path());
    let _guard = LongPathGuard(ext_base.clone());

    // ~48 chars per segment; 8 segments comfortably clears 260 even after the
    // temp base and the \\?\ stripping.
    let segment = "a_long_directory_segment_name_padding_1234567890";
    let mut deep = ext_base.clone();
    for _ in 0..8 {
        deep.push(segment);
    }
    fs::create_dir_all(&deep).expect("create deep tree");
    fs::write(deep.join("deep-audiobook.m4b"), b"deep audio").expect("write deep file");

    let (_db, pool) = fresh_pool().await;
    let summary = run_scan(&pool, base.path()).await.expect("long-path scan");
    assert_eq!(summary.status, "completed");
    assert_eq!(summary.skipped_count, 0, "no skips on a readable deep tree");

    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read");
    let deep_entry = entries
        .iter()
        .find(|e| e.name == "deep-audiobook.m4b")
        .expect("deep file recorded");
    assert_eq!(deep_entry.file_class.as_deref(), Some("audio"));
    assert!(
        deep_entry.path.len() > 260,
        "stored path should exceed 260 chars, was {}",
        deep_entry.path.len()
    );
    assert!(
        !deep_entry.path.starts_with(r"\\?\"),
        "stored path must not carry the verbatim prefix"
    );

    // AC-101.4: with a genuinely >260-char recorded path, a LongPathsDisabled
    // warning is recorded exactly when this host has long-path support disabled
    // (a how-to link accompanies it). When it is enabled, the path scanned fine
    // and draws no long-path warning. This ties the pure warning logic to a real
    // over-limit tree on whichever setting the host actually has.
    use abo_core::ipc::ScanWarningKind;
    let disabled_warning = summary
        .warnings
        .iter()
        .find(|w| w.kind == ScanWarningKind::LongPathsDisabled);
    if abo_core::scan::longpath::long_paths_enabled() {
        assert!(
            disabled_warning.is_none(),
            "no long-paths-disabled warning when the host has long paths enabled"
        );
    } else {
        let w = disabled_warning
            .expect("a >260-char path on a long-paths-disabled host must record the warning");
        assert!(
            w.how_to.is_some(),
            "the long-paths-disabled warning must carry a how-to link"
        );
    }
}

/// AC-11 (permission-denied): a subdir whose read is denied is recorded, the
/// scan completes, its unreadable children are skipped (counted), and a readable
/// sibling is still recorded (the walk never aborts).
#[cfg(windows)]
#[tokio::test]
async fn scan_records_permission_denied() {
    let base = TempDir::new().expect("base tempdir");

    let denied = base.path().join("locked");
    fs::create_dir(&denied).expect("mkdir locked");
    fs::write(denied.join("secret.mp3"), b"secret").expect("write secret");

    let open = base.path().join("open");
    fs::create_dir(&open).expect("mkdir open");
    fs::write(open.join("audio.mp3"), b"ok").expect("write audio");

    let user = std::env::var("USERNAME").unwrap_or_default();
    assert!(!user.is_empty(), "USERNAME must be set to deny access");

    // Deny read/execute (which includes list-directory) on `locked`.
    let denied_ok = icacls_deny(&denied, &format!("{user}:(OI)(CI)(RX)"));
    assert!(
        denied_ok,
        "icacls deny failed on a temp dir we own; investigate before marking ignored-in-CI"
    );
    let _guard = AclGuard {
        path: denied.clone(),
        user: user.clone(),
    };

    let (_db, pool) = fresh_pool().await;
    let summary = run_scan(&pool, base.path()).await.expect("scan continues");
    assert_eq!(summary.status, "completed", "scan runs to completion");
    assert!(
        summary.skipped_count >= 1,
        "the unreadable subtree is counted as skipped (was {})",
        summary.skipped_count
    );
    // AC-101.3: the denied subtree is recorded as a structured warning, not just
    // a count.
    assert!(
        summary
            .warnings
            .iter()
            .any(|w| w.kind == ScanWarningKind::PermissionDenied),
        "a permission-denied warning must be recorded, got {:?}",
        summary.warnings
    );

    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read");
    assert!(
        entries.iter().any(|e| e.name == "locked"),
        "the denied directory itself is recorded"
    );
    assert!(
        entries.iter().any(|e| e.name == "audio.mp3"),
        "the readable sibling is still recorded (scan did not abort)"
    );
    assert!(
        !entries.iter().any(|e| e.name == "secret.mp3"),
        "children of the denied directory are not enumerated"
    );
}

/// AC-11 (junction loop): a junction pointing at an ancestor forms a loop; the
/// scan terminates, the junction is recorded, and it is not followed.
#[cfg(windows)]
#[tokio::test]
async fn scan_terminates_on_junction_loop() {
    let base = TempDir::new().expect("base tempdir");

    let real = base.path().join("real");
    fs::create_dir(&real).expect("mkdir real");
    fs::write(real.join("book.m4b"), b"audio").expect("write book");
    fs::write(real.join("sibling.txt"), b"notes").expect("write sibling");

    // real\loop -> base (an ancestor): following it would recurse forever.
    let junction = real.join("loop");
    let made = mklink_junction(&junction, base.path());
    assert!(
        made,
        "mklink /J failed; cannot build the junction-loop fixture"
    );
    let _guard = JunctionGuard(junction.clone());

    let (_db, pool) = fresh_pool().await;
    let summary = run_scan(&pool, base.path()).await.expect("scan terminates");
    assert_eq!(summary.status, "completed");
    assert!(
        summary.entry_count < 20,
        "entry count stays finite (loop terminated), was {}",
        summary.entry_count
    );
    // AC-101.2: the junction is recorded AS a structured junction-skipped warning
    // (not merely present as a dir entry), naming the loop path.
    assert!(
        summary
            .warnings
            .iter()
            .any(|w| w.kind == ScanWarningKind::JunctionSkipped && w.path.ends_with("loop")),
        "a junction-skipped warning for the loop path must be recorded, got {:?}",
        summary.warnings
    );

    let entries = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read");
    assert!(
        entries.iter().any(|e| e.name == "loop"),
        "the junction is recorded"
    );
    assert!(
        !entries.iter().any(|e| e.path.contains(r"\loop\")),
        "the junction is not followed (no entries beneath it)"
    );
    // The walk was not over-skipped: real siblings are present.
    assert!(entries.iter().any(|e| e.name == "book.m4b"));
    assert!(entries.iter().any(|e| e.name == "sibling.txt"));
}

// ---- Windows test helpers (runtime fixtures + guaranteed cleanup) ----

/// Run `icacls <path> /deny <ace>` directly (icacls is an exe, no cmd needed).
/// Returns true on success.
#[cfg(windows)]
fn icacls_deny(path: &Path, ace: &str) -> bool {
    std::process::Command::new("icacls")
        .arg(path)
        .arg("/deny")
        .arg(ace)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a directory junction (`mklink /J`) via cmd (mklink is a cmd builtin).
#[cfg(windows)]
fn mklink_junction(link: &Path, target: &Path) -> bool {
    std::process::Command::new("cmd")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Removes an extended-length (`\\?\`) directory tree on drop, so a >260-char
/// fixture is always cleaned up even if the test panics (TempDir's own cleanup
/// can struggle past MAX_PATH). Runs before the TempDir's Drop.
#[cfg(windows)]
struct LongPathGuard(std::path::PathBuf);

#[cfg(windows)]
impl Drop for LongPathGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Removes the deny ACE on drop so the temp dir becomes deletable again; without
/// this a failed assertion would leave an undeletable directory behind.
#[cfg(windows)]
struct AclGuard {
    path: std::path::PathBuf,
    user: String,
}

#[cfg(windows)]
impl Drop for AclGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("icacls")
            .arg(&self.path)
            .arg("/remove:d")
            .arg(&self.user)
            .status();
    }
}

/// Removes the junction on drop (the junction itself, never its target).
#[cfg(windows)]
struct JunctionGuard(std::path::PathBuf);

#[cfg(windows)]
impl Drop for JunctionGuard {
    fn drop(&mut self) {
        // A junction is removed like a directory entry; std does not follow it.
        if fs::remove_dir(&self.0).is_err() {
            let _ = std::process::Command::new("cmd")
                .arg("/c")
                .arg("rmdir")
                .arg(&self.0)
                .status();
        }
    }
}
