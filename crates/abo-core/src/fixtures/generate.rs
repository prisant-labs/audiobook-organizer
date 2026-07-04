//! The fixture generator: materializes a [`crate::fixtures::manifest::FixtureManifest`]
//! into a real directory tree under a caller-supplied root.
//!
//! Determinism (AC-F4) is the load-bearing property here, achieved with two
//! rules: the manifest is static data (no run-time randomness, no wall
//! clock), and every file's placeholder content is a pure function of a
//! content seed (the duplicate-group key when the node declares one,
//! otherwise the node's relative path joined into a separator-independent
//! `/`-delimited form via [`path_seed`], never the raw platform path string)
//! plus its declared size. Regenerating from the same manifest into a fresh
//! root therefore always writes the same structure and the same bytes; only
//! OS-assigned metadata this module never reads (inode numbers, mtimes) can
//! differ, and the determinism test excludes exactly those. The
//! separator-independent seed form additionally means the same manifest
//! produces the same bytes across platforms, not merely across runs on one
//! platform.
//!
//! Refusing to write outside the caller's root (test-strategy.md Section 3)
//! is enforced structurally: [`validate_component`] rejects any manifest name
//! containing a path separator, a `:` (a Windows drive-relative prefix
//! hazard, e.g. `C:evil.mp3`), or a `.`/`..` traversal segment before it is
//! ever joined onto a path, so a manifest bug cannot escape the root by
//! construction. [`FixtureError::EscapesRoot`] is the hard-failure signal for
//! that case; it aborts generation immediately (a manifest bug, not a runtime
//! condition).
//!
//! Runtime IO failures are handled differently, per test-strategy.md Section
//! 3's "produced only where the host OS permits, and skipped-with-note
//! otherwise": creating a near-limit-path or reserved-device-name fixture can
//! fail on some hosts even via extended-length semantics (FD-19); such a
//! failure is caught, recorded on [`GeneratedLibrary::skipped`], and
//! generation continues, so the suite stays green on every CI runner.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::fixtures::families::FixtureFamily;
use crate::fixtures::manifest::{FixtureManifest, FixtureNode, PackProvenance};
use crate::paths::{strip_extended_length_prefix, to_extended_length};

/// File or directory, for a materialized [`GeneratedEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedKind {
    File,
    Dir,
}

/// One materialized filesystem entry, carrying the manifest expectations
/// that produced it. Because this index is data (test-strategy.md Section
/// 3), a downstream test can filter by [`FixtureFamily`] instead of
/// enumerating paths by hand.
#[derive(Debug, Clone)]
pub struct GeneratedEntry {
    /// Path relative to [`GeneratedLibrary::root`].
    pub relative_path: PathBuf,
    /// Absolute path, stored form (no `\\?\` prefix).
    pub absolute_path: PathBuf,
    pub kind: GeneratedKind,
    /// Declared size in bytes (0 for directories).
    pub size: u64,
    pub families: Vec<FixtureFamily>,
    pub provenance: Option<PackProvenance>,
}

/// A manifest node the generator declined to materialize on this host,
/// recorded with the reason instead of failing the run.
#[derive(Debug, Clone)]
pub struct SkippedFixture {
    pub relative_path: PathBuf,
    pub families: Vec<FixtureFamily>,
    pub reason: String,
}

/// The result of [`generate`]: every materialized entry plus every
/// skip-with-note, and the sum of declared bytes actually written (which the
/// AC-F5 size-sum test compares against an independent on-disk re-scan).
#[derive(Debug, Clone)]
pub struct GeneratedLibrary {
    /// The root the tree was generated under (stored form, no `\\?\` prefix).
    pub root: PathBuf,
    pub entries: Vec<GeneratedEntry>,
    pub skipped: Vec<SkippedFixture>,
    /// Sum of declared sizes for every file that was actually materialized.
    pub declared_total_bytes: u64,
}

impl GeneratedLibrary {
    /// Whether at least one entry (materialized OR skipped-with-note) carries
    /// `family`. Per test-strategy.md Section 3, a family must never silently
    /// vanish; it is either produced or explicitly noted as skipped.
    pub fn family_recorded(&self, family: FixtureFamily) -> bool {
        self.entries.iter().any(|e| e.families.contains(&family))
            || self.skipped.iter().any(|s| s.families.contains(&family))
    }

    /// Whether `family` was actually materialized on this host (not merely
    /// skipped-with-note).
    pub fn family_materialized(&self, family: FixtureFamily) -> bool {
        self.entries.iter().any(|e| e.families.contains(&family))
    }

    /// Independent on-disk byte total: re-reads `metadata().len()` for every
    /// materialized file entry. Used to prove [`GeneratedLibrary::declared_total_bytes`]
    /// is not just bookkeeping but matches reality (AC-F5).
    pub fn on_disk_total_bytes(&self) -> io::Result<u64> {
        let mut total = 0u64;
        for entry in &self.entries {
            if entry.kind == GeneratedKind::File {
                let extended = literal_extended_length(&entry.absolute_path);
                total += fs::metadata(&extended)?.len();
            }
        }
        Ok(total)
    }
}

/// Build the extended-length form of an already-absolute, already-clean
/// stored path by pure string prefixing, with no filesystem or Win32
/// normalization pass in between.
///
/// [`crate::paths::to_extended_length`] is correct for arbitrary
/// caller-supplied roots, but on Windows its fallback path
/// (`std::path::absolute`, backed by `GetFullPathNameW`) applies the same
/// silent trimming of a trailing dot or space in the final component that
/// every other un-prefixed Win32 path API applies -- exactly the fixture
/// [`crate::fixtures::manifest::standard_library_manifest`] deliberately
/// creates ([`FixtureFamily::TrailingDotOrSpaceNearMiss`]). Round-tripping
/// such an entry's stored `absolute_path` back through that function would
/// therefore silently look up the wrong (nonexistent) name. Every path this
/// helper is handed already came from joining validated, already-absolute
/// path components during generation (see [`generate_node`]), so no
/// normalization pass is needed: only the verbatim prefix has to be restored.
#[cfg(windows)]
fn literal_extended_length(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    if let Some(rest) = s.strip_prefix(r"\\") {
        PathBuf::from(format!(r"\\?\UNC\{rest}"))
    } else {
        PathBuf::from(format!(r"\\?\{s}"))
    }
}

/// Non-Windows: no `\\?\` concept; the path is used as-is.
#[cfg(not(windows))]
fn literal_extended_length(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Fixture-generation errors. These signal a manifest bug (a name that would
/// escape the generator root), not a host-dependent runtime condition; those
/// are recorded as [`SkippedFixture`] instead. Test-only / dev-only surface,
/// not part of the IPC contract (no `specta::Type`, no wire code).
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("fixture manifest component escapes the generator root: {0:?}")]
    EscapesRoot(String),
}

/// Reject a manifest name that could escape the generator root: empty, `.`,
/// `..`, containing a path separator, or containing a `:` (a Windows
/// drive-relative prefix hazard: `C:evil.mp3` addresses a path relative to
/// the current directory on the `C:` drive, not a literal file named
/// `C:evil.mp3`, so joining it onto a path can silently retarget outside the
/// generator root on Windows). Every node name passes through this before
/// ever being joined onto a path, so escaping the root is structurally
/// impossible for a manifest built from validated names.
fn validate_component(name: &str) -> Result<(), FixtureError> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(FixtureError::EscapesRoot(name.to_string()));
    }
    if name.contains('/') || name.contains('\\') || name.contains(':') {
        return Err(FixtureError::EscapesRoot(name.to_string()));
    }
    Ok(())
}

/// Materialize `manifest` under `temp_root` (which must already exist; a
/// [`tempfile::TempDir`] is the intended caller). Returns the index of every
/// materialized and skipped node.
pub fn generate(
    manifest: &FixtureManifest,
    temp_root: &Path,
) -> Result<GeneratedLibrary, FixtureError> {
    let extended_root = to_extended_length(temp_root);
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut declared_total_bytes = 0u64;

    for node in &manifest.top_level {
        generate_node(
            node,
            &extended_root,
            Path::new(""),
            &mut entries,
            &mut skipped,
            &mut declared_total_bytes,
        )?;
    }

    demote_collapsed_same_dir_unicode_twins(&mut entries, &mut skipped, &mut declared_total_bytes);

    Ok(GeneratedLibrary {
        root: strip_extended_length_prefix(&extended_root),
        entries,
        skipped,
        declared_total_bytes,
    })
}

/// Verify that any [`FixtureFamily::UnicodeNfcNfdTwinSameDir`] group
/// actually landed as distinct directory entries on this host, and demote
/// the whole group from materialized to skipped-with-note if it did not.
///
/// `fs::File::create` for the second twin cannot itself detect a collision:
/// on a filesystem that normalizes Unicode forms together (folding two
/// different byte sequences for the "same" name onto one directory entry),
/// the second create call succeeds by silently overwriting the first twin's
/// file, so both writes report `Ok`. The only way to notice is a read-back:
/// list the parent directory afterwards and check that every twin's exact
/// byte-level name is actually present as its own entry.
fn demote_collapsed_same_dir_unicode_twins(
    entries: &mut Vec<GeneratedEntry>,
    skipped: &mut Vec<SkippedFixture>,
    declared_total_bytes: &mut u64,
) {
    use std::collections::{HashMap, HashSet};

    let mut groups: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (idx, e) in entries.iter().enumerate() {
        if e.families
            .contains(&FixtureFamily::UnicodeNfcNfdTwinSameDir)
        {
            if let Some(parent) = e.absolute_path.parent() {
                groups.entry(parent.to_path_buf()).or_default().push(idx);
            }
        }
    }

    let mut demote: Vec<usize> = Vec::new();
    for (parent, idxs) in &groups {
        if idxs.len() < 2 {
            continue;
        }
        let extended_parent = to_extended_length(parent);
        let actual_names: HashSet<_> = match fs::read_dir(&extended_parent) {
            Ok(read_dir) => read_dir
                .filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .collect(),
            Err(_) => HashSet::new(),
        };
        let distinct_present = idxs
            .iter()
            .filter(|&&i| {
                entries[i]
                    .absolute_path
                    .file_name()
                    .map(|n| actual_names.contains(n))
                    .unwrap_or(false)
            })
            .count();
        if distinct_present < idxs.len() {
            demote.extend(idxs.iter().copied());
        }
    }

    if demote.is_empty() {
        return;
    }
    // Remove from the back forward so earlier indices stay valid.
    demote.sort_unstable_by(|a, b| b.cmp(a));
    demote.dedup();
    for idx in demote {
        let e = entries.remove(idx);
        if e.kind == GeneratedKind::File {
            *declared_total_bytes -= e.size;
        }
        skipped.push(SkippedFixture {
            relative_path: e.relative_path,
            families: e.families,
            reason: "filesystem normalizes NFC/NFD forms together: same-directory twin collapsed onto one directory entry".to_string(),
        });
    }
}

fn generate_node(
    node: &FixtureNode,
    parent_abs: &Path,
    parent_rel: &Path,
    entries: &mut Vec<GeneratedEntry>,
    skipped: &mut Vec<SkippedFixture>,
    declared_total_bytes: &mut u64,
) -> Result<(), FixtureError> {
    match node {
        FixtureNode::Folder(folder) => {
            validate_component(&folder.name)?;
            let abs = parent_abs.join(&folder.name);
            let rel = parent_rel.join(&folder.name);
            match fs::create_dir(&abs) {
                Ok(()) => {
                    entries.push(GeneratedEntry {
                        relative_path: rel.clone(),
                        absolute_path: strip_extended_length_prefix(&abs),
                        kind: GeneratedKind::Dir,
                        size: 0,
                        families: folder.families.clone(),
                        provenance: folder.provenance,
                    });
                    for child in &folder.children {
                        generate_node(child, &abs, &rel, entries, skipped, declared_total_bytes)?;
                    }
                }
                Err(e) => {
                    skipped.push(SkippedFixture {
                        relative_path: rel,
                        families: folder.families.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }
        FixtureNode::File(file_node) => {
            validate_component(&file_node.name)?;
            let abs = parent_abs.join(&file_node.name);
            let rel = parent_rel.join(&file_node.name);
            let seed = file_node
                .dup_group
                .map(|g| g.to_string())
                .unwrap_or_else(|| path_seed(&rel));
            match write_placeholder_file(&abs, file_node.size, &seed) {
                Ok(()) => {
                    *declared_total_bytes += file_node.size;
                    entries.push(GeneratedEntry {
                        relative_path: rel,
                        absolute_path: strip_extended_length_prefix(&abs),
                        kind: GeneratedKind::File,
                        size: file_node.size,
                        families: file_node.families.clone(),
                        provenance: None,
                    });
                }
                Err(e) => {
                    skipped.push(SkippedFixture {
                        relative_path: rel,
                        families: file_node.families.clone(),
                        reason: e.to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// Write `size` bytes of deterministic placeholder content to `path`, seeded
/// by `seed`. Same `(seed, size)` always produces the same bytes (AC-F4);
/// different seeds produce different content, and duplicate-pair files pass
/// the same seed (their shared `dup_group`) so they end up byte-identical,
/// not merely same-size.
fn write_placeholder_file(path: &Path, size: u64, seed: &str) -> io::Result<()> {
    let mut f = fs::File::create(path)?;
    if size == 0 {
        return Ok(());
    }
    let mut state = seed_state(seed);
    let mut remaining = size;
    let mut buf = [0u8; 4096];
    while remaining > 0 {
        let chunk = remaining.min(buf.len() as u64) as usize;
        for b in buf[..chunk].iter_mut() {
            state = xorshift64(state);
            *b = (state & 0xFF) as u8;
        }
        f.write_all(&buf[..chunk])?;
        remaining -= chunk as u64;
    }
    Ok(())
}

/// Build a content seed from a relative path that does not depend on the
/// host's path separator: components joined with a literal `/`, regardless
/// of whether the platform running the generator uses `\` (Windows) or `/`
/// (everywhere else). Without this, `rel.to_string_lossy()` would embed the
/// platform separator directly into the seed, so the same manifest would
/// write different placeholder bytes for the same logical relative path on
/// Windows versus Linux/macOS, breaking AC-F4 determinism across platforms
/// even though it holds fine within a single platform.
fn path_seed(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// FNV-1a of `seed`, used as the xorshift64 starting state. Never returns 0
/// (an absorbing state for xorshift64) by substituting a fixed nonzero
/// fallback in that case.
fn seed_state(seed: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in seed.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    if hash == 0 {
        0x9E37_79B9_7F4A_7C15
    } else {
        hash
    }
}

/// A small, fast, non-cryptographic PRNG step. Determinism, not security, is
/// the point: same input state always yields the same next state.
fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::manifest::{standard_library_manifest, FileNode, FolderNode};
    use tempfile::TempDir;

    /// Recursively compare two directory trees for identical structure and
    /// byte-identical file content. Deliberately ignores metadata this
    /// module never reads or writes (mtimes, permissions), which is exactly
    /// what AC-F4 ("byte-identical", content and structure) calls for.
    ///
    /// Takes plain (non-extended-length) roots and normalizes them once: the
    /// manifest's reserved-name fixtures (a folder literally named `NUL`)
    /// would otherwise resolve to the Windows null device instead of the
    /// real directory entry the moment a plain path joins that component, so
    /// every comparison in the recursion must stay on the extended-length
    /// form from the top down.
    fn assert_trees_identical(a_root: &Path, b_root: &Path) {
        assert_trees_identical_extended(&to_extended_length(a_root), &to_extended_length(b_root));
    }

    fn assert_trees_identical_extended(a: &Path, b: &Path) {
        let mut a_names: Vec<_> = fs::read_dir(a)
            .unwrap_or_else(|e| panic!("read_dir {a:?}: {e}"))
            .map(|e| e.unwrap().file_name())
            .collect();
        let mut b_names: Vec<_> = fs::read_dir(b)
            .unwrap_or_else(|e| panic!("read_dir {b:?}: {e}"))
            .map(|e| e.unwrap().file_name())
            .collect();
        a_names.sort();
        b_names.sort();
        assert_eq!(
            a_names, b_names,
            "directory listings differ under {a:?} vs {b:?}"
        );

        for name in a_names {
            let a_child = a.join(&name);
            let b_child = b.join(&name);
            let a_meta = fs::symlink_metadata(&a_child).unwrap();
            let b_meta = fs::symlink_metadata(&b_child).unwrap();
            assert_eq!(
                a_meta.is_dir(),
                b_meta.is_dir(),
                "entry kind differs for {name:?}"
            );
            if a_meta.is_dir() {
                assert_trees_identical_extended(&a_child, &b_child);
            } else {
                let a_bytes = fs::read(&a_child).unwrap();
                let b_bytes = fs::read(&b_child).unwrap();
                assert_eq!(
                    a_bytes, b_bytes,
                    "file content differs for {name:?} ({a_child:?} vs {b_child:?})"
                );
            }
        }
    }

    /// AC-F4: regenerating from the same manifest into two fresh roots
    /// yields byte-identical trees (content and structure). Written before
    /// the generator implementation was trusted, per the phase's test-first
    /// posture.
    #[test]
    fn regenerate_twice_is_byte_identical() {
        let manifest = standard_library_manifest();
        let dir_a = TempDir::new().expect("tempdir a");
        let dir_b = TempDir::new().expect("tempdir b");

        let lib_a = generate(&manifest, dir_a.path()).expect("generate a");
        let lib_b = generate(&manifest, dir_b.path()).expect("generate b");

        assert_eq!(
            lib_a.entries.len(),
            lib_b.entries.len(),
            "entry counts must match across regeneration"
        );
        assert_eq!(
            lib_a.declared_total_bytes, lib_b.declared_total_bytes,
            "declared byte totals must match across regeneration"
        );
        assert_eq!(
            lib_a.skipped.len(),
            lib_b.skipped.len(),
            "skip counts must match across regeneration on the same host"
        );

        assert_trees_identical(dir_a.path(), dir_b.path());
    }

    /// AC-F5: the sum of declared sizes for materialized files equals the
    /// independently re-read on-disk byte total.
    #[test]
    fn on_disk_bytes_match_declared_total() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let on_disk = lib.on_disk_total_bytes().expect("on-disk byte scan");
        assert_eq!(
            on_disk, lib.declared_total_bytes,
            "on-disk byte total must equal the manifest-declared total (AC-F5)"
        );
        // Sanity: the manifest actually declares a nonzero, nontrivial
        // amount, so this assertion is not vacuously true.
        assert!(lib.declared_total_bytes > 1_000_000);
    }

    /// AC-F1: nothing escapes the caller-supplied root, and the generator
    /// refuses (rather than silently sanitizing) a manifest node whose name
    /// would traverse outside it.
    #[test]
    fn generation_refuses_to_escape_the_root() {
        let hostile = FixtureManifest {
            top_level: vec![FixtureNode::Folder(FolderNode {
                name: "../escape-attempt".to_string(),
                families: Vec::new(),
                provenance: None,
                children: Vec::new(),
            })],
        };
        let dir = TempDir::new().expect("tempdir");
        let result = generate(&hostile, dir.path());
        assert!(
            matches!(result, Err(FixtureError::EscapesRoot(_))),
            "expected EscapesRoot, got {result:?}"
        );

        // A component containing an embedded separator is refused the same
        // way, whichever separator the manifest author used.
        let hostile_sep = FixtureManifest {
            top_level: vec![FixtureNode::File(FileNode {
                name: "nested/escape.mp3".to_string(),
                size: 10,
                families: Vec::new(),
                dup_group: None,
            })],
        };
        let dir2 = TempDir::new().expect("tempdir");
        let result2 = generate(&hostile_sep, dir2.path());
        assert!(matches!(result2, Err(FixtureError::EscapesRoot(_))));
    }

    /// AC-F1 (nothing generated under the repo working tree) is a
    /// process-level guarantee established by every test in this suite
    /// calling `TempDir::new()` (OS temp dir) rather than a path under
    /// `CARGO_MANIFEST_DIR`; this test documents and locks that convention so
    /// a future edit cannot accidentally point the generator at the repo.
    #[test]
    fn temp_dir_root_is_outside_the_repo_working_tree() {
        let dir = TempDir::new().expect("tempdir");
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            !dir.path().starts_with(repo_root),
            "fixture tempdir {:?} must not be inside the repo tree {:?}",
            dir.path(),
            repo_root
        );
    }

    /// FD-01: every pack-member folder in the standard manifest carries the
    /// provenance annotation, and it names the same pack as the shell.
    #[test]
    fn pack_members_carry_their_shells_provenance() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let shell = lib
            .entries
            .iter()
            .find(|e| e.families.contains(&FixtureFamily::PackShell))
            .expect("a pack-shell entry must exist");
        let shell_provenance = shell
            .provenance
            .expect("pack shell must carry its own provenance for the member comparison");

        let members: Vec<_> = lib
            .entries
            .iter()
            .filter(|e| e.families.contains(&FixtureFamily::PackMember))
            .collect();
        assert!(!members.is_empty(), "expected at least one pack member");
        for member in members {
            assert_eq!(
                member.provenance,
                Some(shell_provenance),
                "pack member {:?} must carry the shell's provenance",
                member.relative_path
            );
        }
    }

    /// test-strategy.md Section 3's harness self-test: generate, scan with
    /// the real F-101 walker, and compare counts to the declared index,
    /// before any downstream suite is asked to trust the harness.
    #[test]
    fn round_trip_scan_matches_declared_counts() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let normalized_root = to_extended_length(dir.path());
        let outcome = crate::scan::walk::walk(&normalized_root);

        let scanned_files = outcome
            .entries
            .iter()
            .filter(|e| e.kind == crate::scan::walk::EntryKind::File)
            .count();
        let declared_files = lib
            .entries
            .iter()
            .filter(|e| e.kind == GeneratedKind::File)
            .count();
        assert_eq!(
            scanned_files, declared_files,
            "the F-101 walker's file count must match the generator's own index"
        );

        let scanned_bytes: u64 = outcome
            .entries
            .iter()
            .filter(|e| e.kind == crate::scan::walk::EntryKind::File)
            .map(|e| e.size)
            .sum();
        assert_eq!(
            scanned_bytes, lib.declared_total_bytes,
            "the F-101 walker's byte total must match the manifest-declared total"
        );
    }

    /// AC-F2/AC-F3, checked against what actually landed on disk this run
    /// (not just what the manifest declares): every required family is
    /// either materialized or explicitly recorded as skipped-with-note. A
    /// family silently absent from both would be a harness bug.
    #[test]
    fn every_required_family_is_materialized_or_explicitly_skipped() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let unrecorded: Vec<&str> = FixtureFamily::REQUIRED
            .iter()
            .filter(|f| !lib.family_recorded(**f))
            .map(|f| f.as_str())
            .collect();
        assert!(
            unrecorded.is_empty(),
            "families neither materialized nor skipped-with-note: {unrecorded:?}"
        );
    }

    /// On a normal, unrestricted Windows dev host, the hard cases (near-limit
    /// paths, reserved device names) should actually materialize via
    /// extended-length semantics, not merely skip. This documents the
    /// expected common case; CI hosts that refuse them still pass the
    /// weaker test above.
    #[cfg(windows)]
    #[test]
    fn hard_cases_materialize_on_a_normal_windows_host() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        for family in [
            FixtureFamily::NearLimitPath,
            FixtureFamily::ReservedNameNearMiss,
            FixtureFamily::TrailingDotOrSpaceNearMiss,
        ] {
            assert!(
                lib.family_materialized(family),
                "{:?} should materialize (not just skip) on a normal Windows host; skipped: {:?}",
                family,
                lib.skipped
                    .iter()
                    .filter(|s| s.families.contains(&family))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn zero_byte_files_materialize_as_empty() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let zero_byte_entries: Vec<_> = lib
            .entries
            .iter()
            .filter(|e| e.families.contains(&FixtureFamily::ZeroByteSample))
            .collect();
        assert!(!zero_byte_entries.is_empty());
        for e in zero_byte_entries {
            assert_eq!(e.size, 0);
            let extended = to_extended_length(&e.absolute_path);
            assert_eq!(fs::metadata(&extended).unwrap().len(), 0);
        }
    }

    /// FIX 2 regression guard: the content seed for a nested path must be
    /// separator-independent (joined with `/`), so the same manifest writes
    /// the same bytes whether the generator runs on Windows or on a
    /// platform whose native separator is `/`.
    #[test]
    fn path_seed_is_separator_independent() {
        let rel = Path::new("Genre - SciFI")
            .join("Nested Book")
            .join("Nested Book.m4b");
        let seed = path_seed(&rel);
        assert!(
            seed.contains('/'),
            "seed must join components with '/': {seed:?}"
        );
        assert!(
            !seed.contains('\\'),
            "seed must not contain a platform backslash: {seed:?}"
        );
        assert_eq!(seed, "Genre - SciFI/Nested Book/Nested Book.m4b");
    }

    /// FIX 5 regression guard: a component containing a `:` (a Windows
    /// drive-relative prefix hazard) is refused the same way an embedded
    /// separator is.
    #[test]
    fn generation_refuses_colon_in_component() {
        let hostile = FixtureManifest {
            top_level: vec![FixtureNode::File(FileNode {
                name: "C:evil.mp3".to_string(),
                size: 10,
                families: Vec::new(),
                dup_group: None,
            })],
        };
        let dir = TempDir::new().expect("tempdir");
        let result = generate(&hostile, dir.path());
        assert!(
            matches!(result, Err(FixtureError::EscapesRoot(_))),
            "expected EscapesRoot, got {result:?}"
        );
    }

    /// FIX 4: the same-directory NFC/NFD twins either both materialize as
    /// distinct directory entries, or both are recorded as
    /// skipped-with-note together (never a silent half-materialization
    /// where one twin's write clobbered the other without a trace).
    #[test]
    fn same_dir_unicode_twins_are_distinct_or_both_skipped() {
        let manifest = standard_library_manifest();
        let dir = TempDir::new().expect("tempdir");
        let lib = generate(&manifest, dir.path()).expect("generate");

        let materialized: Vec<_> = lib
            .entries
            .iter()
            .filter(|e| {
                e.families
                    .contains(&FixtureFamily::UnicodeNfcNfdTwinSameDir)
            })
            .collect();
        let skipped: Vec<_> = lib
            .skipped
            .iter()
            .filter(|s| {
                s.families
                    .contains(&FixtureFamily::UnicodeNfcNfdTwinSameDir)
            })
            .collect();

        if skipped.is_empty() {
            assert_eq!(
                materialized.len(),
                2,
                "expected both same-dir NFC/NFD twins to materialize distinctly, got {materialized:?}"
            );
            let parent = materialized[0]
                .absolute_path
                .parent()
                .expect("twin must have a parent directory");
            let extended_parent = to_extended_length(parent);
            let names: std::collections::HashSet<_> = fs::read_dir(&extended_parent)
                .unwrap()
                .filter_map(|e| e.ok())
                .map(|e| e.file_name())
                .collect();
            assert_eq!(
                names.len(),
                2,
                "directory must contain two distinct entries for the twins"
            );
        } else {
            assert_eq!(
                materialized.len(),
                0,
                "twins must not be half-materialized: {materialized:?}"
            );
            assert_eq!(skipped.len(), 2, "both twins must be skipped together");
        }
    }

    #[test]
    fn seed_state_is_deterministic_and_varies_by_seed() {
        assert_eq!(seed_state("a"), seed_state("a"));
        assert_ne!(seed_state("a"), seed_state("b"));
    }

    #[test]
    fn placeholder_content_is_deterministic_for_same_seed_and_size() {
        let dir = TempDir::new().expect("tempdir");
        let p1 = dir.path().join("one.bin");
        let p2 = dir.path().join("two.bin");
        write_placeholder_file(&p1, 10_000, "same-seed").unwrap();
        write_placeholder_file(&p2, 10_000, "same-seed").unwrap();
        assert_eq!(fs::read(&p1).unwrap(), fs::read(&p2).unwrap());

        let p3 = dir.path().join("three.bin");
        write_placeholder_file(&p3, 10_000, "different-seed").unwrap();
        assert_ne!(fs::read(&p1).unwrap(), fs::read(&p3).unwrap());
    }
}
