//! F-1110 (book-level duplicate comparison): the tiered comparison that tells
//! a real multi-file duplicate apart from two folders that merely share a name.
//!
//! # The gap this closes
//!
//! Every criterion in `F-702` to `F-905` assumes one book is one file. A book
//! split across twelve mp3s is twelve unrelated files to
//! [`detect_exact_duplicates`](super::detect::detect_exact_duplicates), so two
//! copies of it never form an exact group, and the Copies card counts only
//! exact groups. A user with two copies of a twelve-part book is therefore told
//! nothing at all. That silent under-reporting is the worst property of shipping
//! without this (`F-1110` descope note).
//!
//! # Why this ADDS TO existing groups rather than emitting new ones
//!
//! Folders that share a normalised title already group, as
//! [`METHOD_VERSION`](super::detect::METHOD_VERSION) version candidates. A
//! fingerprint match requires a title match by construction, so every
//! book-level duplicate is ALREADY inside one of those groups. Emitting a
//! second group for it would count the same book twice, and `FD-08` makes the
//! GROUP the counted unit. So `F-1110` raises the MATCH TIER of a group that
//! already exists; it never creates one.
//!
//! That choice is also what makes `AC-55` work. A one-file copy and a
//! twelve-file copy of the same book must group together and never auto-resolve.
//! They share a title, so they are one group; they differ in audio count, so the
//! group never reaches [`BookMatch::Fingerprint`]. Partitioning by fingerprint
//! instead would have split them into a group of one each, and a group of one is
//! dropped, so they would have stopped grouping at all.
//!
//! # What counts as a book folder
//!
//! The [`FolderClass::Book`] verdict the classifier already produces, plus one
//! containment rule: a `Book` folder nested inside another `Book` folder is a
//! PART of that book, not a book. Without the rule a disc-split title is five
//! candidate books rather than one, and worse, every book's `Disc 1` normalises
//! to the title "disc 1", so unrelated books would fingerprint-match on their
//! disc folders. Measured on the standard fixture before the rule existed:
//! `Verbal Advantage` produced five `Book` folders, one real and four discs.
//!
//! # Purity
//!
//! Like the rest of the planning core this module is pure logic: no I/O, nothing
//! `cfg`-gated (the CFG RULE). The content tier (`AC-54`) is deliberately NOT
//! here, because it needs hashes that only exist after a verification pass; it
//! lives beside the other database-backed questions.

use std::collections::{HashMap, HashSet};

use crate::classify::engine::{classify, FolderClass};
use crate::parse::extract::{MergedEntry, NodeKind};
use crate::plan::builder::{classify_inputs_from_plan_nodes, PlanNode};
use crate::scan::typing::FileClass;

/// How closely the members of a folder duplicate group actually match.
///
/// Ordered weakest to strongest, and each tier is an equivalence relation over
/// a canonical value, so a group's tier is simply the strongest tier at which
/// EVERY member agrees. That is one pass over the members, not a pairwise loop,
/// even though `AC-54` describes content matching pairwise: "all N agree" and
/// "every pair agrees" are the same statement for an equivalence relation.
///
/// The tiers are cumulative by construction rather than by convention. Equal
/// sorted size vectors imply an equal count and an equal sum, so
/// [`Structural`](Self::Structural) cannot be reached without
/// [`Fingerprint`](Self::Fingerprint) also holding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BookMatch {
    /// The members share a normalised title and nothing more. Possibly
    /// different editions, possibly a book beside a container that happens to
    /// carry its name, possibly one copy as a single file and another split
    /// across twelve. Never auto-resolved (`AC-55`).
    TitleOnly,
    /// Every member is a book folder and all of them agree on title, audio-file
    /// count, and total audio bytes (`AC-51`, `AC-52`). Duplicate CANDIDATES in
    /// exactly the sense single-file candidates already are: recorded, counted,
    /// never acted on.
    Fingerprint,
    /// The members additionally agree on the sorted multiset of their audio file
    /// sizes (`AC-53`). Reads no file contents.
    Structural,
}

impl BookMatch {
    /// Stable machine string, for evidence and any surface that names the tier.
    pub fn as_str(self) -> &'static str {
        match self {
            BookMatch::TitleOnly => "title-only",
            BookMatch::Fingerprint => "fingerprint",
            BookMatch::Structural => "structural",
        }
    }

    /// Whether this tier makes the group a duplicate CANDIDATE (`AC-52`) rather
    /// than a looser version candidate.
    ///
    /// This is the predicate that ends the silent under-reporting: a group at
    /// this tier or better is counted on the Copies card and in the report,
    /// where before only exact single-file groups were.
    pub fn is_duplicate_candidate(self) -> bool {
        self >= BookMatch::Fingerprint
    }
}

/// One book folder, described only from data the scan already holds (`AC-51`):
/// no new scan pass, no filesystem read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookFolder {
    /// The `entries.id` of the folder itself.
    pub id: usize,
    pub path: String,
    /// The normalised parsed title. Computed with the SAME normaliser the
    /// version-candidate detector uses, which is what guarantees a fingerprint
    /// match is always also a title match, which in turn is what makes the
    /// no-double-counting argument in the module doc hold.
    pub title_norm: String,
    /// Count of audio files anywhere beneath this folder.
    pub audio_count: usize,
    /// Total bytes across those audio files.
    pub audio_bytes: u64,
    /// The `entries.id` of every audio file beneath this folder, sorted.
    ///
    /// Two things need it. The subsumption rule in
    /// [`detect_duplicates`](super::detect::detect_duplicates) asks which book
    /// owns a given file, and `AC-54`'s content tier will ask which files to
    /// hash for a given book. Book folders never nest (see the containment rule
    /// above), so these sets partition the library's audio: every file belongs
    /// to at most one book.
    pub audio_entry_ids: Vec<usize>,
    /// Their sizes, SORTED ascending.
    ///
    /// Sorted rather than in directory order, settled by jp on 2026-08-14. The
    /// spec's original "ordered multiset" is self-contradictory, and the
    /// ordering it implied is not stable across two copies of the same book:
    /// a positional comparison reports false differences on genuinely identical
    /// folders. Sorting makes the comparison canonical.
    pub audio_sizes: Vec<u64>,
}

impl BookFolder {
    /// The `AC-51` fingerprint: title, audio-file count, total audio bytes.
    pub fn fingerprint(&self) -> (&str, usize, u64) {
        (&self.title_norm, self.audio_count, self.audio_bytes)
    }
}

/// Normalise a title for matching: collapse whitespace, lowercase.
///
/// Deliberately identical to `detect::normalize_title`. Kept as one function
/// used by both rather than two that agree today: if they ever diverged, a
/// fingerprint match would stop implying a title match and book groups would
/// silently start double-counting against the version candidates they live in.
pub(super) fn normalize_title(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Derive every book folder in the snapshot (`AC-51`).
///
/// A book folder is a folder the classifier calls [`FolderClass::Book`], with no
/// `Book` ancestor, holding at least one audio file of non-zero total size, and
/// carrying a parsed title that normalises to something non-empty.
///
/// Each of those four conditions drops a real case that would otherwise become a
/// false duplicate candidate:
///
/// - **Not `Book`**: a genre shelf parses a title too (`Genre - SciFI` yields
///   "SciFI"), and so do series containers and staging folders. Matching on
///   "carries a title" would fingerprint all of them.
/// - **A `Book` ancestor**: the folder is a disc or part of the book above it.
/// - **No audio, or zero bytes**: a folder of zero-byte placeholders is not a
///   book, and two of them would match each other on a fingerprint of
///   (title, n, 0). `F-203` already drops zero-byte files for the same reason.
/// - **No title**: `AC-51`'s fingerprint has a title component, so a book whose
///   name parses to nothing cannot have one. Such a book is still findable by
///   the exact single-file detector when it is a single file.
pub fn book_folders_from_plan_nodes(nodes: &[PlanNode], merged: &[MergedEntry]) -> Vec<BookFolder> {
    let index_of: HashMap<usize, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();

    let classified = classify(&classify_inputs_from_plan_nodes(nodes));
    let book_ids: HashSet<usize> = classified
        .iter()
        .filter(|c| c.class == FolderClass::Book)
        .map(|c| c.id)
        .collect();

    let title_of: HashMap<usize, &str> = merged
        .iter()
        .filter_map(|m| m.fields.title.as_ref().map(|t| (m.id, t.value.as_str())))
        .collect();

    // Audio sizes per ancestor folder, walking each audio file up to the root.
    // The step guard mirrors `dupe_entries_from_plan_nodes`: a malformed parent
    // chain must not spin, and a snapshot is untrusted input.
    let mut audio_files: HashMap<usize, Vec<(usize, u64)>> = HashMap::new();
    for n in nodes {
        if n.file_class != Some(FileClass::Audio) {
            continue;
        }
        let mut cur = n.parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > nodes.len() {
                break;
            }
            steps += 1;
            audio_files.entry(p).or_default().push((n.id, n.size));
            cur = nodes[index_of[&p]].parent;
        }
    }

    let mut out = Vec::new();
    for n in nodes {
        if n.kind != NodeKind::Folder || !book_ids.contains(&n.id) {
            continue;
        }
        if has_book_ancestor(n, nodes, &index_of, &book_ids) {
            continue;
        }
        let Some(title) = title_of.get(&n.id) else {
            continue;
        };
        let title_norm = normalize_title(title);
        if title_norm.is_empty() {
            continue;
        }
        let files = audio_files.get(&n.id).cloned().unwrap_or_default();
        if files.is_empty() {
            continue;
        }
        let mut sizes: Vec<u64> = files.iter().map(|(_, s)| *s).collect();
        sizes.sort_unstable();
        let audio_bytes: u64 = sizes.iter().sum();
        if audio_bytes == 0 {
            continue;
        }
        let mut audio_entry_ids: Vec<usize> = files.iter().map(|(i, _)| *i).collect();
        audio_entry_ids.sort_unstable();
        out.push(BookFolder {
            id: n.id,
            path: n.path.clone(),
            title_norm,
            audio_count: sizes.len(),
            audio_bytes,
            audio_entry_ids,
            audio_sizes: sizes,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Whether `node` sits anywhere beneath a folder the classifier calls `Book`.
fn has_book_ancestor(
    node: &PlanNode,
    nodes: &[PlanNode],
    index_of: &HashMap<usize, usize>,
    book_ids: &HashSet<usize>,
) -> bool {
    let mut cur = node.parent;
    let mut steps = 0;
    while let Some(p) = cur {
        if steps > nodes.len() {
            break;
        }
        steps += 1;
        if book_ids.contains(&p) {
            return true;
        }
        cur = nodes[index_of[&p]].parent;
    }
    false
}

/// The match tier for one folder group, given its member entry ids and the book
/// folders of the snapshot.
///
/// Returns the strongest tier at which every member agrees. A member that is not
/// a book folder has no fingerprint, so the group cannot rise above
/// [`BookMatch::TitleOnly`]: that is the case where a book sits beside a series
/// container carrying the same name, which is exactly a human decision.
pub fn match_tier(member_ids: &[usize], books: &[BookFolder]) -> BookMatch {
    let by_id: HashMap<usize, &BookFolder> = books.iter().map(|b| (b.id, b)).collect();

    let mut shapes = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        match by_id.get(id) {
            Some(b) => shapes.push(*b),
            // Not a book folder, so there is nothing to compare. Never guess.
            None => return BookMatch::TitleOnly,
        }
    }
    // A group of one is not a group; detection drops those before this is
    // called, but the guard keeps the function honest if called directly.
    let Some(first) = shapes.first() else {
        return BookMatch::TitleOnly;
    };
    if shapes.len() < 2 {
        return BookMatch::TitleOnly;
    }

    if !shapes
        .iter()
        .all(|b| b.fingerprint() == first.fingerprint())
    {
        return BookMatch::TitleOnly;
    }
    if !shapes.iter().all(|b| b.audio_sizes == first.audio_sizes) {
        return BookMatch::Fingerprint;
    }
    BookMatch::Structural
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: usize, path: &str, title: &str, sizes: &[u64]) -> BookFolder {
        let mut sizes = sizes.to_vec();
        sizes.sort_unstable();
        BookFolder {
            id,
            path: path.to_string(),
            title_norm: normalize_title(title),
            audio_count: sizes.len(),
            audio_bytes: sizes.iter().sum(),
            // Synthetic ids, distinct per book so a containment test over these
            // fixtures behaves the way it does over a real snapshot.
            audio_entry_ids: (0..sizes.len()).map(|i| id * 1_000 + i).collect(),
            audio_sizes: sizes,
        }
    }

    /// AC-51 + AC-53: two copies of a twelve-part book match all the way to
    /// structural, even though the two folders list their parts in different
    /// orders. This is the case jp settled on 2026-08-14: directory iteration
    /// order is not stable across two copies, so the comparison must be
    /// canonical rather than positional.
    #[test]
    fn twelve_part_copies_match_structurally_despite_listing_order_ac53() {
        let forward: Vec<u64> = (1..=12).map(|i| i * 1000).collect();
        let reversed: Vec<u64> = forward.iter().rev().copied().collect();
        let a = book(1, "lib/a/Dune", "Dune", &forward);
        let b = book(2, "lib/b/Dune", "Dune", &reversed);
        assert_eq!(match_tier(&[1, 2], &[a, b]), BookMatch::Structural);
    }

    /// AC-53: same count and same total bytes, different individual sizes. The
    /// fingerprint cannot tell these apart; the structural tier can, and stops
    /// there rather than claiming a structural match.
    #[test]
    fn same_count_and_total_but_different_parts_stops_at_fingerprint() {
        let a = book(1, "lib/a/Dune", "Dune", &[100, 200, 300]);
        let b = book(2, "lib/b/Dune", "Dune", &[150, 150, 300]);
        assert_eq!(a.audio_bytes, b.audio_bytes, "the fingerprints agree");
        assert_eq!(match_tier(&[1, 2], &[a, b]), BookMatch::Fingerprint);
    }

    /// AC-55: a single-file copy and a multi-file copy of the same title never
    /// rise above title-only, so nothing about them can auto-resolve. Choosing
    /// between one file and twelve is a preference, not a mechanical ranking.
    #[test]
    fn single_file_copy_beside_multi_file_copy_stays_title_only_ac55() {
        let one = book(1, "lib/a/Dune", "Dune", &[780_000]);
        let twelve = book(2, "lib/b/Dune", "Dune", &[65_000; 12]);
        assert_eq!(
            one.audio_bytes, twelve.audio_bytes,
            "even with identical total bytes, the counts differ"
        );
        let tier = match_tier(&[1, 2], &[one, twelve]);
        assert_eq!(tier, BookMatch::TitleOnly);
        assert!(
            !tier.is_duplicate_candidate(),
            "AC-55: this pair must never be treated as a resolvable duplicate"
        );
    }

    /// A member that is not a book folder (a series container carrying the same
    /// name as a book, which the standard fixture actually contains) keeps the
    /// group at title-only. Nothing is guessed about the missing side.
    #[test]
    fn a_member_that_is_not_a_book_folder_keeps_the_group_at_title_only() {
        let a = book(1, "lib/a/Dresden Files", "Dresden Files", &[210_000]);
        // Member 2 is absent from the book list entirely.
        assert_eq!(match_tier(&[1, 2], &[a]), BookMatch::TitleOnly);
    }

    /// Three copies agree as one group, not as three pairs. The tier is a
    /// property of the whole group, so one odd copy holds all of it back.
    #[test]
    fn one_odd_copy_holds_the_whole_group_back() {
        let a = book(1, "lib/a/Dune", "Dune", &[100, 200]);
        let b = book(2, "lib/b/Dune", "Dune", &[100, 200]);
        let odd = book(3, "lib/c/Dune", "Dune", &[100, 201]);
        assert_eq!(
            match_tier(&[1, 2], &[a.clone(), b.clone()]),
            BookMatch::Structural
        );
        assert_eq!(match_tier(&[1, 2, 3], &[a, b, odd]), BookMatch::TitleOnly);
    }

    /// The tiers are ordered, and `is_duplicate_candidate` draws the line in
    /// one place rather than at every call site.
    #[test]
    fn tier_ordering_and_the_candidate_line() {
        assert!(BookMatch::TitleOnly < BookMatch::Fingerprint);
        assert!(BookMatch::Fingerprint < BookMatch::Structural);
        assert!(!BookMatch::TitleOnly.is_duplicate_candidate());
        assert!(BookMatch::Fingerprint.is_duplicate_candidate());
        assert!(BookMatch::Structural.is_duplicate_candidate());
    }
}
