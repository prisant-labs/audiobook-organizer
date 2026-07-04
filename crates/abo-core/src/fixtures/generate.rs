//! The fixture generator: materializes a [`crate::fixtures::manifest::FixtureManifest`]
//! into a real directory tree under a caller-supplied root.
//!
//! Determinism (AC-F4) is the load-bearing property here, achieved with two
//! rules: the manifest is static data (no run-time randomness, no wall
//! clock), and every file's placeholder content is a pure function of a
//! content seed (the duplicate-group key when the node declares one,
//! otherwise the node's relative path) plus its declared size. Regenerating
//! from the same manifest into a fresh root therefore always writes the same
//! structure and the same bytes; only OS-assigned metadata this module never
//! reads (inode numbers, mtimes) can differ, and the determinism test
//! excludes exactly those.
//!
//! Refusing to write outside the caller's root (test-strategy.md Section 3)
//! is enforced structurally: [`validate_component`] rejects any manifest name
//! containing a path separator or a `.`/`..` traversal segment before it is
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
                let extended = to_extended_length(&entry.absolute_path);
                total += fs::metadata(&extended)?.len();
            }
        }
        Ok(total)
    }
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
/// `..`, or containing a path separator. Every node name passes through this
/// before ever being joined onto a path, so escaping the root is structurally
/// impossible for a manifest built from validated names.
fn validate_component(name: &str) -> Result<(), FixtureError> {
    if name.is_empty() || name == "." || name == ".." {
        return Err(FixtureError::EscapesRoot(name.to_string()));
    }
    if name.contains('/') || name.contains('\\') {
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

    Ok(GeneratedLibrary {
        root: strip_extended_length_prefix(&extended_root),
        entries,
        skipped,
        declared_total_bytes,
    })
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
                .unwrap_or_else(|| rel.to_string_lossy().into_owned());
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
