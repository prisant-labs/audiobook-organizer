//! F-701 (duplicate candidate detection): name+size exact grouping across the
//! snapshot, with the GROUP as the canonical unit (FD-08).
//!
//! Two detection methods, kept distinct so nothing exact is ever confused with a
//! looser candidate:
//!   - **exact basename+size** ([`METHOD_EXACT`]): files sharing a basename AND a
//!     declared size are one duplicate group. This is the method that found the
//!     ~403 groups / ~10.08 GB candidate estimate at the 2026-03-25 baseline
//!     (FD-18, pending fresh scan); the GB figure is the total bytes across all
//!     members of the candidate groups, an estimate, NOT a measured duplicate
//!     volume (the true volume stays unknown until content hashing lands in
//!     v0.6.0 per F-702). No hashing here.
//!   - **normalized-title** ([`METHOD_VERSION`]): book folders whose normalized
//!     titles match are VERSION candidates (possibly different editions/bytes),
//!     labeled distinctly and NEVER auto-resolved. A version candidate is never
//!     folded into an exact-duplicate group.
//!
//! The canonical unit is the group (one book, N identical copies); member files
//! are "copies". Counts everywhere count groups; any byte figure states what it
//! measures (FD-08). This release is candidate-only: detection writes the
//! `duplicate_groups`/`duplicate_members` tables (see [`crate::db::dupes`]) but
//! emits no plan quarantine op (flag-only; a loser is quarantined only via a
//! later release's confirm flow after hash verification or explicit override).
//!
//! Like the rest of the planning core, this module is pure logic: no I/O,
//! nothing `cfg`-gated (the CFG RULE). Output is deterministic: groups are sorted
//! by key, members by path.

use std::collections::BTreeMap;

use crate::dupes::books::{match_tier, normalize_title, BookFolder, BookMatch};
use crate::parse::extract::{MergedEntry, NodeKind};
use crate::plan::builder::PlanNode;

/// The exact basename+size detection method label (also the `method` value
/// persisted in `duplicate_groups`).
pub const METHOD_EXACT: &str = "exact-basename-size";

/// The normalized-title version-candidate method label.
pub const METHOD_VERSION: &str = "normalized-title";

/// One entry duplicate detection considers: a stable id (the `entries.id` a
/// member row references), its full path, its basename, its size in bytes,
/// whether it is a directory, and its parsed title (folders only; [`None`] for
/// files and untitled folders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupeEntry {
    pub id: usize,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub title: Option<String>,
}

/// One member ("copy") of a duplicate group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMember {
    pub entry_id: usize,
    pub path: String,
    pub size: u64,
}

/// A duplicate candidate group: its detection method, a stable key, the total
/// bytes across all its members (the FD-08 candidate estimate, not a measured
/// duplicate volume), and its members ("copies"), sorted by path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    pub method: &'static str,
    pub group_key: String,
    pub total_bytes: u64,
    pub members: Vec<DuplicateMember>,
    /// How closely the members actually match, for FOLDER groups (`F-1110`).
    ///
    /// `None` for exact basename+size groups: those are file-level, and a file
    /// has no book shape to compare. An `Option` rather than an extra
    /// [`BookMatch`] variant, so a caller cannot read "not applicable" as a
    /// weak match and count it.
    pub book_match: Option<BookMatch>,
}

impl DuplicateGroup {
    /// The number of copies in this group (>= 2 for any real group).
    pub fn copies(&self) -> usize {
        self.members.len()
    }

    /// Whether this is an exact basename+size group (vs a version candidate).
    pub fn is_exact(&self) -> bool {
        self.method == METHOD_EXACT
    }

    /// Whether this is a normalized-title version candidate.
    pub fn is_version_candidate(&self) -> bool {
        self.method == METHOD_VERSION
    }

    /// Whether this group counts as a duplicate CANDIDATE (`AC-52`), the unit
    /// the Copies card and the report count.
    ///
    /// True for every exact basename+size group, as it always was, and now also
    /// for a folder group whose members agree at least on the `AC-51`
    /// fingerprint. Before `F-1110` the second half did not exist, so two copies
    /// of a twelve-part book contributed nothing to any count a user could see.
    pub fn is_duplicate_candidate(&self) -> bool {
        self.is_exact()
            || self
                .book_match
                .is_some_and(BookMatch::is_duplicate_candidate)
    }
}

/// Detect exact basename+size duplicate groups among FILE entries (AC-27). Two
/// files are one group when they share a basename AND a size; a group of three
/// identical copies is one group with three copies, not three pairs. Groups with
/// a single member are not groups and are dropped. Deterministic: groups sorted
/// by key, members by path.
pub fn detect_exact_duplicates(entries: &[DupeEntry]) -> Vec<DuplicateGroup> {
    // Key by (basename, size); BTreeMap keeps the output group order stable.
    let mut buckets: BTreeMap<(String, u64), Vec<&DupeEntry>> = BTreeMap::new();
    for e in entries {
        if e.is_dir {
            continue;
        }
        buckets.entry((e.name.clone(), e.size)).or_default().push(e);
    }

    let mut groups = Vec::new();
    for ((name, size), mut members) in buckets {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| a.path.cmp(&b.path));
        let total_bytes: u64 = members.iter().map(|m| m.size).sum();
        groups.push(DuplicateGroup {
            method: METHOD_EXACT,
            group_key: format!("{name}|{size}"),
            total_bytes,
            members: members
                .iter()
                .map(|m| DuplicateMember {
                    entry_id: m.id,
                    path: m.path.clone(),
                    size: m.size,
                })
                .collect(),
            // File-level: there is no book shape to compare.
            book_match: None,
        });
    }
    groups
}

/// Detect normalized-title version-candidate groups among FOLDER entries with a
/// parsed title (AC-28). Book folders whose titles normalize to the same string
/// are candidate versions of one book, labeled distinctly from exact duplicates
/// and never auto-resolved. Groups with a single member are dropped.
/// Deterministic: groups sorted by normalized title, members by path.
///
/// Each group additionally carries its `F-1110` match tier, computed from
/// `books`. The tier is what separates a real multi-file duplicate from two
/// folders that merely share a name, and it is attached to THIS group rather
/// than emitted as a second group: a fingerprint match implies a title match,
/// so a separate group would count the same book twice against `FD-08`. Pass an
/// empty `books` slice to get title-only tiers throughout, which is what a
/// caller with no classification available should expect to see.
pub fn detect_version_candidates(
    entries: &[DupeEntry],
    books: &[BookFolder],
) -> Vec<DuplicateGroup> {
    let mut buckets: BTreeMap<String, Vec<&DupeEntry>> = BTreeMap::new();
    for e in entries {
        if !e.is_dir {
            continue;
        }
        let Some(title) = e.title.as_deref() else {
            continue;
        };
        let norm = normalize_title(title);
        if norm.is_empty() {
            continue;
        }
        buckets.entry(norm).or_default().push(e);
    }

    let mut groups = Vec::new();
    for (norm, mut members) in buckets {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| a.path.cmp(&b.path));
        let total_bytes: u64 = members.iter().map(|m| m.size).sum();
        let member_ids: Vec<usize> = members.iter().map(|m| m.id).collect();
        groups.push(DuplicateGroup {
            method: METHOD_VERSION,
            group_key: norm,
            total_bytes,
            members: members
                .iter()
                .map(|m| DuplicateMember {
                    entry_id: m.id,
                    path: m.path.clone(),
                    size: m.size,
                })
                .collect(),
            book_match: Some(match_tier(&member_ids, books)),
        });
    }
    groups
}

/// Detect all duplicate candidates across the snapshot: exact basename+size
/// groups first, then normalized-title version candidates. The two method
/// families never merge (an exact-duplicate file pair and a version-candidate
/// folder pair are separate groups with separate methods).
pub fn detect_duplicates(entries: &[DupeEntry], books: &[BookFolder]) -> Vec<DuplicateGroup> {
    let mut groups = detect_exact_duplicates(entries);
    groups.extend(detect_version_candidates(entries, books));
    groups
}

/// Build [`DupeEntry`]s from the plan builder's node view plus the F-303 merged
/// fields (the real path from a snapshot to duplicate detection). Files carry no
/// title; folders carry their parsed title (for version-candidate matching) and
/// a size equal to their total descendant file bytes.
pub fn dupe_entries_from_plan_nodes(nodes: &[PlanNode], merged: &[MergedEntry]) -> Vec<DupeEntry> {
    use std::collections::HashMap;

    let index_of: HashMap<usize, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let title_of: HashMap<usize, &str> = merged
        .iter()
        .filter_map(|m| m.fields.title.as_ref().map(|t| (m.id, t.value.as_str())))
        .collect();

    // Total descendant file bytes per folder id.
    let mut folder_bytes: HashMap<usize, u64> = HashMap::new();
    for n in nodes {
        if n.kind != NodeKind::File {
            continue;
        }
        let mut cur = n.parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > nodes.len() {
                break;
            }
            steps += 1;
            *folder_bytes.entry(p).or_default() += n.size;
            cur = nodes[index_of[&p]].parent;
        }
    }

    nodes
        .iter()
        .map(|n| {
            let is_dir = n.kind == NodeKind::Folder;
            DupeEntry {
                id: n.id,
                path: n.path.clone(),
                name: n.name.clone(),
                size: if is_dir {
                    folder_bytes.get(&n.id).copied().unwrap_or(0)
                } else {
                    n.size
                },
                is_dir,
                title: if is_dir {
                    title_of.get(&n.id).map(|s| s.to_string())
                } else {
                    None
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: usize, path: &str, name: &str, size: u64) -> DupeEntry {
        DupeEntry {
            id,
            path: path.to_string(),
            name: name.to_string(),
            size,
            is_dir: false,
            title: None,
        }
    }

    fn dir(id: usize, path: &str, title: Option<&str>, size: u64) -> DupeEntry {
        DupeEntry {
            id,
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap().to_string(),
            size,
            is_dir: true,
            title: title.map(str::to_string),
        }
    }

    /// AC-27: a three-copy fixture yields ONE group of three (not three pairs),
    /// members labeled copies.
    #[test]
    fn three_identical_copies_are_one_group_of_three() {
        let entries = [
            file(1, "lib/a/Book.m4b", "Book.m4b", 100),
            file(2, "lib/b/Book.m4b", "Book.m4b", 100),
            file(3, "lib/c/Book.m4b", "Book.m4b", 100),
        ];
        let groups = detect_exact_duplicates(&entries);
        assert_eq!(groups.len(), 1, "one group, not three pairs");
        assert_eq!(groups[0].copies(), 3);
        assert!(groups[0].is_exact());
        assert_eq!(groups[0].total_bytes, 300, "sum of all member bytes");
        // Members sorted by path.
        let paths: Vec<&str> = groups[0].members.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["lib/a/Book.m4b", "lib/b/Book.m4b", "lib/c/Book.m4b"]
        );
    }

    /// Same basename but different size is NOT an exact duplicate (size must
    /// match too).
    #[test]
    fn same_name_different_size_is_not_a_group() {
        let entries = [
            file(1, "lib/a/Book.m4b", "Book.m4b", 100),
            file(2, "lib/b/Book.m4b", "Book.m4b", 200),
        ];
        assert!(detect_exact_duplicates(&entries).is_empty());
    }

    /// A suffixed OS-style copy (`Book (1).m4b`) has a DIFFERENT basename than
    /// its source, so it is a near-miss, never grouped into the exact pair.
    #[test]
    fn suffixed_copy_is_not_grouped_with_the_exact_pair() {
        let entries = [
            file(1, "lib/Book.m4b", "Book.m4b", 100),
            file(2, "lib/Book (1).m4b", "Book (1).m4b", 100),
            file(3, "lib/other/Book.m4b", "Book.m4b", 100),
        ];
        let groups = detect_exact_duplicates(&entries);
        assert_eq!(groups.len(), 1, "only the two exact-basename copies group");
        assert_eq!(groups[0].copies(), 2);
        assert!(
            groups[0]
                .members
                .iter()
                .all(|m| m.path.ends_with("/Book.m4b")),
            "the suffixed copy must be excluded"
        );
    }

    /// AC-28: normalized-title folder matches are version candidates, labeled
    /// distinctly from exact duplicates and never merged into them.
    #[test]
    fn folders_with_matching_titles_are_version_candidates() {
        let entries = [
            dir(1, "lib/Genre/Dresden Files", Some("Dresden Files"), 210),
            dir(2, "lib/Packs/dresden   files", Some("dresden   files"), 400),
        ];
        let groups = detect_version_candidates(&entries, &[]);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_version_candidate());
        assert_eq!(groups[0].method, METHOD_VERSION);
        assert_eq!(groups[0].group_key, "dresden files");
        assert_eq!(groups[0].copies(), 2);
        assert_eq!(groups[0].total_bytes, 610);
        // With no book folders supplied, nothing is known about their shape, so
        // the tier is title-only and the group is NOT a duplicate candidate.
        assert_eq!(groups[0].book_match, Some(BookMatch::TitleOnly));
        assert!(!groups[0].is_duplicate_candidate());
    }

    /// F-1110: two folders whose book shapes agree lift the group to a
    /// duplicate CANDIDATE, which is what ends the silent under-reporting. The
    /// group is the SAME group, with a stronger tier: no second group is
    /// emitted, so the FD-08 count does not move.
    #[test]
    fn matching_book_shapes_lift_a_version_group_to_a_duplicate_candidate() {
        use crate::dupes::books::BookFolder;

        let entries = [
            dir(1, "lib/a/Dune", Some("Dune"), 600),
            dir(2, "lib/b/Dune", Some("Dune"), 600),
        ];
        let books = vec![
            BookFolder {
                id: 1,
                path: "lib/a/Dune".to_string(),
                title_norm: "dune".to_string(),
                audio_count: 3,
                audio_bytes: 600,
                audio_sizes: vec![100, 200, 300],
            },
            BookFolder {
                id: 2,
                path: "lib/b/Dune".to_string(),
                title_norm: "dune".to_string(),
                audio_count: 3,
                audio_bytes: 600,
                audio_sizes: vec![100, 200, 300],
            },
        ];

        let without = detect_version_candidates(&entries, &[]);
        let with = detect_version_candidates(&entries, &books);
        assert_eq!(
            without.len(),
            with.len(),
            "F-1110 raises a tier; it never adds a group"
        );
        assert_eq!(with[0].book_match, Some(BookMatch::Structural));
        assert!(with[0].is_duplicate_candidate());
        assert!(
            !without[0].is_duplicate_candidate(),
            "and without the book shapes it would still be under-reported"
        );
    }

    /// Files are never version candidates and folders are never exact
    /// duplicates: the two methods do not cross.
    #[test]
    fn methods_do_not_cross() {
        let entries = [
            file(1, "lib/a/Book.m4b", "Book.m4b", 100),
            file(2, "lib/b/Book.m4b", "Book.m4b", 100),
            dir(3, "lib/Title", Some("Title"), 100),
            dir(4, "lib/x/Title", Some("Title"), 100),
        ];
        let all = detect_duplicates(&entries, &[]);
        assert_eq!(all.len(), 2, "one exact group + one version group");
        assert_eq!(all.iter().filter(|g| g.is_exact()).count(), 1);
        assert_eq!(all.iter().filter(|g| g.is_version_candidate()).count(), 1);
        // An exact file group has no book shape to compare, and says so rather
        // than reporting a weak match a caller could mistake for one.
        let exact = all.iter().find(|g| g.is_exact()).unwrap();
        assert_eq!(exact.book_match, None);
        assert!(
            exact.is_duplicate_candidate(),
            "exact groups always counted"
        );
    }
}
