//! F-1110 (book-level duplicate comparison) end to end over purpose-built
//! trees: AC-51 to AC-55.
//!
//! # Why these trees are local rather than added to the standard fixture
//!
//! `standard_library_manifest` deliberately contains NO multi-file book copied
//! twice; measured before this was written, its only folder group is a
//! single-file `Dresden Files` book beside a series container of the same name.
//! Adding the missing case there would move goldens and snapshots across twelve
//! test files at once, which would bury a real regression in a wall of expected
//! diff. The cases live here, next to the assertions that read them, and moving
//! one into the standard fixture is its own reviewable change.
//!
//! Each tree is built as `PlanNode`s directly, the same view the production
//! pipeline hands the detector.

use abo_core::dupes::detect::dupe_entries_from_plan_nodes;
use abo_core::dupes::{book_folders_from_plan_nodes, detect_duplicates, BookMatch, DuplicateGroup};
use abo_core::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};
use abo_core::plan::builder::PlanNode;
use abo_core::scan::typing::classify_path;
use std::path::Path;

/// A tiny declarative tree, so each test reads as the library it describes.
enum T {
    D(&'static str, Vec<T>),
    F(&'static str, u64),
}

fn build(tree: Vec<T>) -> Vec<PlanNode> {
    let mut nodes = Vec::new();
    let mut next = 0usize;
    for t in &tree {
        walk(t, None, "library", &mut next, &mut nodes);
    }
    nodes
}

fn walk(
    t: &T,
    parent: Option<usize>,
    parent_path: &str,
    next: &mut usize,
    out: &mut Vec<PlanNode>,
) {
    let id = *next;
    *next += 1;
    match t {
        T::D(name, children) => {
            let path = format!("{parent_path}/{name}");
            out.push(PlanNode {
                id,
                parent,
                name: (*name).to_string(),
                path: path.clone(),
                kind: NodeKind::Folder,
                file_class: None,
                size: 0,
            });
            for c in children {
                walk(c, Some(id), &path, next, out);
            }
        }
        T::F(name, size) => {
            out.push(PlanNode {
                id,
                parent,
                name: (*name).to_string(),
                path: format!("{parent_path}/{name}"),
                kind: NodeKind::File,
                file_class: Some(classify_path(Path::new(name))),
                size: *size,
            });
        }
    }
}

fn merged_of(nodes: &[PlanNode]) -> Vec<MergedEntry> {
    let inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    extract(&inputs)
}

fn groups_of(nodes: &[PlanNode]) -> Vec<DuplicateGroup> {
    let merged = merged_of(nodes);
    let entries = dupe_entries_from_plan_nodes(nodes, &merged);
    let books = book_folders_from_plan_nodes(nodes, &merged);
    detect_duplicates(&entries, &books)
}

fn folder_group<'a>(groups: &'a [DuplicateGroup], key: &str) -> &'a DuplicateGroup {
    groups
        .iter()
        .find(|g| g.is_version_candidate() && g.group_key == key)
        .unwrap_or_else(|| panic!("no folder group keyed {key:?} in {groups:#?}"))
}

/// Twelve mp3 parts with distinct sizes, so a structural match means something.
fn twelve_parts() -> Vec<T> {
    (1..=12)
        .map(|i| {
            let name: &'static str = Box::leak(format!("Part {i:02}.mp3").into_boxed_str());
            T::F(name, 60_000 + i * 1_000)
        })
        .collect()
}

/// AC-51, AC-52, AC-53: two copies of a twelve-part book are ONE group, counted
/// as a duplicate candidate, and reach the structural tier. Before F-1110 this
/// pair contributed nothing countable: twelve mp3s are twelve unrelated files to
/// the exact detector, and the folder group it did form was filtered out of every
/// count as a mere version candidate.
#[test]
fn two_twelve_part_copies_are_one_candidate_group_at_structural_tier() {
    let nodes = build(vec![
        T::D("Genre - SciFI", vec![T::D("Dune", twelve_parts())]),
        T::D("Backups", vec![T::D("Dune", twelve_parts())]),
    ]);
    let groups = groups_of(&nodes);
    let g = folder_group(&groups, "dune");

    assert_eq!(g.copies(), 2, "one group of two copies, never two pairs");
    assert_eq!(g.book_match, Some(BookMatch::Structural));
    assert!(g.is_duplicate_candidate(), "AC-52: recorded and counted");
    assert_eq!(
        groups.iter().filter(|g| g.is_version_candidate()).count(),
        1,
        "F-1110 raises a tier on the group that exists; it emits no second group"
    );

    // FD-08: one duplicated book is ONE counted group. The twelve parts of the
    // two copies are identically named and sized, so the exact detector also
    // found twelve groups; every one of them is the same duplication restated,
    // and none is counted.
    assert_eq!(
        groups.iter().filter(|g| g.is_duplicate_candidate()).count(),
        1,
        "one duplicated book, one counted group: {groups:#?}"
    );
    assert_eq!(
        groups.iter().filter(|g| g.is_exact()).count(),
        12,
        "the part-level groups are still DETECTED, just not counted"
    );
    assert!(
        groups
            .iter()
            .filter(|g| g.is_exact())
            .all(|g| g.subsumed_by_book_group),
        "every part group is subsumed by the book group above it"
    );
}

/// A disc-split book copied twice: every disc carries a `track01.mp3`, so the
/// exact detector finds a FOUR-member group drawn from two books. That is still
/// the folder-level duplication restated one track at a time, so it is subsumed.
///
/// This is the case that made the rule "spans at least two copies" rather than
/// "exactly one member per copy". Disc-split books are ordinary: measured on the
/// standard fixture, `Verbal Advantage` alone yields a four-copy `track01.mp3`
/// group without any duplication at all.
#[test]
fn a_disc_split_book_copied_twice_counts_once_despite_repeated_track_names() {
    let discs = || {
        vec![
            T::D("Disc 1", vec![T::F("track01.mp3", 45_000)]),
            T::D("Disc 2", vec![T::F("track01.mp3", 45_000)]),
        ]
    };
    let nodes = build(vec![
        T::D(
            "Genre - Non-Fiction",
            vec![T::D("Verbal Advantage", discs())],
        ),
        T::D("Backups", vec![T::D("Verbal Advantage", discs())]),
    ]);
    let groups = groups_of(&nodes);

    let track = groups
        .iter()
        .find(|g| g.is_exact() && g.group_key.starts_with("track01.mp3|"))
        .expect("track01.mp3 repeats across discs AND copies");
    assert_eq!(track.copies(), 4, "two discs times two copies");
    assert!(track.subsumed_by_book_group);
    assert_eq!(
        groups.iter().filter(|g| g.is_duplicate_candidate()).count(),
        1,
        "one duplicated book, one counted group: {groups:#?}"
    );
}

/// A group confined to a SINGLE book is that book's own internal duplication,
/// never the folder-level one, so it keeps counting on its own account. Here the
/// duplicated book is genuinely duplicated AND one part repeats inside a copy.
#[test]
fn a_group_confined_to_one_book_is_never_subsumed() {
    let nodes = build(vec![
        // A lone disc-split book, not duplicated anywhere, whose two discs
        // happen to carry the same track name and size.
        T::D(
            "Genre - Non-Fiction",
            vec![T::D(
                "Verbal Advantage",
                vec![
                    T::D("Disc 1", vec![T::F("track01.mp3", 45_000)]),
                    T::D("Disc 2", vec![T::F("track01.mp3", 45_000)]),
                ],
            )],
        ),
    ]);
    let groups = groups_of(&nodes);

    assert!(
        !groups.iter().any(|g| g.is_version_candidate()),
        "nothing is duplicated at the book level here"
    );
    let track = groups
        .iter()
        .find(|g| g.is_exact() && g.group_key.starts_with("track01.mp3|"))
        .expect("the two same-named tracks still group");
    assert!(
        !track.subsumed_by_book_group,
        "with no covering book group, nothing may be suppressed"
    );
    assert!(track.is_duplicate_candidate());
}

/// An exact group with a member OUTSIDE any book folder is not covered by a book
/// group, so it keeps counting. The rule only suppresses what it can prove is a
/// restatement; anything it cannot prove stays visible.
#[test]
fn an_exact_group_reaching_outside_the_books_is_not_subsumed() {
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![
                T::D("Dune", twelve_parts()),
                // A loose copy of one part, sitting on the shelf itself.
                T::F("Part 01.mp3", 61_000),
            ],
        ),
        T::D("Backups", vec![T::D("Dune", twelve_parts())]),
    ]);
    let groups = groups_of(&nodes);

    let reaching = groups
        .iter()
        .find(|g| g.is_exact() && g.group_key.starts_with("Part 01.mp3|"))
        .expect("the Part 01 group must exist");
    assert_eq!(reaching.copies(), 3, "two in books, one loose");
    assert!(
        !reaching.subsumed_by_book_group,
        "a member outside every book cannot be covered by a book group"
    );
    assert!(reaching.is_duplicate_candidate());
}

/// Regression: an ordinary single-file duplicate, with no book group anywhere
/// near it, counts exactly as it always did. The subsumption rule must be
/// invisible when there is nothing to subsume.
#[test]
fn a_plain_single_file_duplicate_still_counts() {
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![T::F("Island by Aldous Huxley.m4b", 120_000)],
        ),
        T::D(
            "Backups",
            vec![T::F("Island by Aldous Huxley.m4b", 120_000)],
        ),
    ]);
    let groups = groups_of(&nodes);
    assert_eq!(
        groups.iter().filter(|g| g.is_duplicate_candidate()).count(),
        1
    );
    assert!(groups
        .iter()
        .filter(|g| g.is_exact())
        .all(|g| !g.subsumed_by_book_group));
}

/// A TITLE-ONLY folder group subsumes nothing: it is not a confirmed duplicate,
/// so the part-level evidence under it must stay countable. Suppressing there
/// would hide real file duplicates behind a pair that only shares a name.
#[test]
fn a_title_only_group_subsumes_nothing() {
    // Same title, different shapes (AC-55), but one part happens to match.
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![T::D(
                "Dune",
                vec![T::F("Part 01.mp3", 61_000), T::F("Part 02.mp3", 62_000)],
            )],
        ),
        T::D(
            "Backups",
            vec![T::D("Dune", vec![T::F("Part 01.mp3", 61_000)])],
        ),
    ]);
    let groups = groups_of(&nodes);
    let folder = folder_group(&groups, "dune");
    assert_eq!(folder.book_match, Some(BookMatch::TitleOnly));
    assert!(!folder.is_duplicate_candidate());

    let part1 = groups
        .iter()
        .find(|g| g.is_exact() && g.group_key.starts_with("Part 01.mp3|"))
        .expect("Part 01 matches across the two folders");
    assert!(
        !part1.subsumed_by_book_group,
        "an unconfirmed pair must not hide the file evidence beneath it"
    );
    assert!(part1.is_duplicate_candidate());
}

/// AC-53 as jp settled it on 2026-08-14: the sizes are SORTED and compared
/// canonically, because directory iteration order is not stable across two
/// copies of the same book. Here the two copies hold the same twelve parts under
/// the same twelve names, but the sizes are encountered in opposite order. A
/// positional comparison reports a false difference on a genuinely identical
/// pair; a canonical one does not.
///
/// The part names stay `Part NN`, deliberately. Names like `a-01.mp3` parse as
/// `Author - Title` under F-301, so twelve of them read as twelve DISTINCT
/// titles and the folder classifies `MultiBookSuspect` rather than `Book` - it
/// then has no book shape at all and the tier collapses to title-only for a
/// reason that has nothing to do with what this test is checking. Found by
/// writing it the other way first.
#[test]
fn structural_match_ignores_the_order_sizes_are_encountered_in_ac53() {
    let ascending: Vec<T> = (1..=12)
        .map(|i| {
            let name: &'static str = Box::leak(format!("Part {i:02}.mp3").into_boxed_str());
            T::F(name, 60_000 + i * 1_000)
        })
        .collect();
    let descending: Vec<T> = (1..=12)
        .map(|i| {
            let name: &'static str = Box::leak(format!("Part {i:02}.mp3").into_boxed_str());
            T::F(name, 60_000 + (13 - i) * 1_000)
        })
        .collect();

    let nodes = build(vec![
        T::D("Genre - SciFI", vec![T::D("Dune", ascending)]),
        T::D("Backups", vec![T::D("Dune", descending)]),
    ]);

    // The two copies really are listed in opposite size order.
    let merged = merged_of(&nodes);
    let books = book_folders_from_plan_nodes(&nodes, &merged);
    assert_eq!(books.len(), 2);
    assert_eq!(
        books[0].audio_sizes, books[1].audio_sizes,
        "sorted, so the two agree"
    );

    let groups = groups_of(&nodes);
    assert_eq!(
        folder_group(&groups, "dune").book_match,
        Some(BookMatch::Structural),
        "canonical comparison, not positional"
    );
}

/// AC-55: a single-file copy and a multi-file copy of the same title GROUP
/// TOGETHER but never rise above title-only, so nothing about them can
/// auto-resolve. Choosing between one file and twelve is a preference, not a
/// mechanical ranking.
#[test]
fn single_file_copy_beside_a_twelve_part_copy_groups_but_never_resolves_ac55() {
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![T::D("Dune", vec![T::F("Dune.m4b", 858_000)])],
        ),
        T::D("Backups", vec![T::D("Dune", twelve_parts())]),
    ]);
    let groups = groups_of(&nodes);
    let g = folder_group(&groups, "dune");

    assert_eq!(g.copies(), 2, "AC-55: they must still group together");
    assert_eq!(g.book_match, Some(BookMatch::TitleOnly));
    assert!(
        !g.is_duplicate_candidate(),
        "AC-55: never presented as a resolvable duplicate"
    );
}

/// The containment rule. A disc-split book classifies as `Book` AND so does each
/// of its disc folders, so without the rule one book is five candidates. Worse,
/// every book's `Disc 1` normalises to the title "disc 1", so two unrelated
/// books would fingerprint-match on their disc folders alone.
///
/// Measured on the standard fixture before the rule existed: `Verbal Advantage`
/// produced five `Book` folders, one real and four discs.
#[test]
fn a_disc_split_book_is_one_candidate_and_its_discs_never_match_across_books() {
    let discs = || {
        vec![
            T::D(
                "Disc 1",
                vec![T::F("track01.mp3", 45_000), T::F("track02.mp3", 45_000)],
            ),
            T::D("Disc 2", vec![T::F("track01.mp3", 45_000)]),
        ]
    };
    let nodes = build(vec![
        // Two DIFFERENT books, each disc-split with identically shaped discs.
        T::D(
            "Genre - Non-Fiction",
            vec![T::D("Verbal Advantage", discs())],
        ),
        T::D(
            "Genre - Business",
            vec![T::D("Executive Presence", discs())],
        ),
    ]);

    let merged = merged_of(&nodes);
    let books = book_folders_from_plan_nodes(&nodes, &merged);
    let paths: Vec<&str> = books.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "library/Genre - Business/Executive Presence",
            "library/Genre - Non-Fiction/Verbal Advantage",
        ],
        "one book folder per book: the disc folders are PARTS, not books"
    );
    assert_eq!(books[0].audio_count, 3, "the parts count toward the book");

    // And the identically shaped discs of two different books never group.
    let groups = groups_of(&nodes);
    assert!(
        !groups
            .iter()
            .any(|g| g.is_version_candidate() && g.is_duplicate_candidate()),
        "two unrelated books must not become duplicates via their disc folders: {groups:#?}"
    );
}

/// A genre shelf parses a title too, so "carries a title" was never the right
/// population. Two shelves that happen to share a name group as version
/// candidates, as they always did, but never become duplicate candidates: the
/// classifier calls them pack containers, not books.
#[test]
fn two_shelves_sharing_a_name_never_become_duplicate_candidates() {
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![
                T::D("Dune", vec![T::F("Dune.m4b", 200_000)]),
                T::D("Neuromancer", vec![T::F("Neuromancer.m4b", 150_000)]),
            ],
        ),
        T::D(
            "Backups",
            vec![T::D(
                "Genre - SciFI",
                vec![
                    T::D("Foundation", vec![T::F("Foundation.m4b", 200_000)]),
                    T::D("Hyperion", vec![T::F("Hyperion.m4b", 150_000)]),
                ],
            )],
        ),
    ]);
    let groups = groups_of(&nodes);
    let shelf = groups
        .iter()
        .find(|g| g.is_version_candidate() && g.group_key == "scifi");
    if let Some(g) = shelf {
        assert_eq!(
            g.book_match,
            Some(BookMatch::TitleOnly),
            "a shelf is not a book, whatever its name parses to"
        );
        assert!(!g.is_duplicate_candidate());
    }
    assert!(
        !groups.iter().any(|g| g.is_duplicate_candidate()),
        "nothing here is a duplicate: {groups:#?}"
    );
}

/// Same title, same file count, same total bytes, different individual parts:
/// the fingerprint agrees and the structural tier catches the difference. This
/// is the case AC-53 exists for; AC-51 alone cannot see it.
#[test]
fn same_fingerprint_but_different_parts_stops_at_the_fingerprint_tier() {
    let a = vec![
        T::F("Part 01.mp3", 100_000),
        T::F("Part 02.mp3", 200_000),
        T::F("Part 03.mp3", 300_000),
    ];
    let b = vec![
        T::F("Part 01.mp3", 150_000),
        T::F("Part 02.mp3", 150_000),
        T::F("Part 03.mp3", 300_000),
    ];
    let nodes = build(vec![
        T::D("Genre - SciFI", vec![T::D("Dune", a)]),
        T::D("Backups", vec![T::D("Dune", b)]),
    ]);
    let groups = groups_of(&nodes);
    let g = folder_group(&groups, "dune");
    assert_eq!(g.book_match, Some(BookMatch::Fingerprint));
    assert!(
        g.is_duplicate_candidate(),
        "still a candidate: AC-52 records it, AC-54 is what would settle it"
    );
}

/// A folder of zero-byte placeholders is not a book, so two of them do not
/// become duplicate candidates by agreeing on a fingerprint of (title, n, 0).
/// F-203 already drops zero-byte files for the same reason.
#[test]
fn zero_byte_placeholder_folders_are_not_book_candidates() {
    let nodes = build(vec![
        T::D(
            "Genre - SciFI",
            vec![T::D(
                "Dune",
                vec![T::F("Part 01.mp3", 0), T::F("Part 02.mp3", 0)],
            )],
        ),
        T::D(
            "Backups",
            vec![T::D(
                "Dune",
                vec![T::F("Part 01.mp3", 0), T::F("Part 02.mp3", 0)],
            )],
        ),
    ]);
    let merged = merged_of(&nodes);
    assert!(
        book_folders_from_plan_nodes(&nodes, &merged).is_empty(),
        "zero total audio bytes is not a book"
    );
    let groups = groups_of(&nodes);
    assert!(!folder_group(&groups, "dune").is_duplicate_candidate());
}

/// Detection stays deterministic with the tier attached: same tree, same groups,
/// same order, same tiers.
#[test]
fn detection_with_tiers_is_deterministic() {
    let nodes = build(vec![
        T::D("Genre - SciFI", vec![T::D("Dune", twelve_parts())]),
        T::D("Backups", vec![T::D("Dune", twelve_parts())]),
    ]);
    assert_eq!(groups_of(&nodes), groups_of(&nodes));
}
