//! Does the Duplicates nav badge count the same thing the Duplicates screen shows?
//!
//! It does not, and this file is the proof.
//!
//! # Why anyone should care
//!
//! `AC-29` (v0.6.0 hardening) puts a count on the Duplicates nav item. A person
//! reads that number, clicks it, and expects to find that many things. Two
//! separate implementations of "a duplicate group" feed the two halves of that
//! sentence, and they are not the same implementation:
//!
//!   * **The badge** reads `health_metrics`'s `duplicate-candidate-groups`
//!     (`classify/metrics.rs`), which buckets FILES by `(basename, size)` and
//!     counts every bucket holding two or more. It knows nothing about books.
//!   * **The screen** reads `dupes::review`, built on `detect_duplicates`
//!     (`dupes/detect.rs`), which additionally groups duplicated BOOK FOLDERS
//!     and then marks the per-track file groups those folders already cover as
//!     `subsumed_by_book_group`. A subsumed group is not a candidate, and the
//!     review drops it.
//!
//! So the moment a whole book is duplicated, the badge counts one group per
//! TRACK while the screen shows one card per BOOK. A twelve-file audiobook that
//! exists twice is `12` to the badge and `1` to the screen.
//!
//! # Why this is not just a rounding difference
//!
//! `review.rs` already carries a test named
//! `a_subsumed_group_is_excluded_just_as_the_copies_card_excludes_it`, whose own
//! comment says: "The review must count the same population the Copies card
//! counts, or the export and the screen disagree, which is exactly what `AC-20`
//! forbids." That rule was enforced between the review, the export and the
//! Copies card. The nav badge is a FOURTH counter that arrived later, from a
//! different module, and nothing checked it against that rule.
//!
//! `useNavCounts.ts` documents a guarantee it really does provide: the badge and
//! the Library home cannot disagree, because `AppShell` makes one
//! `useHealthMetrics()` call and feeds both. That guarantee is about the badge
//! versus the HOME. It says nothing about the badge versus the DUPLICATES
//! SCREEN, and the two are computed from different code.
//!
//! # Status
//!
//! Recorded, NOT fixed. The fix is a product decision rather than a bug with one
//! obvious repair, because the two numbers are each defensible for their own
//! surface, and picking one changes what a user sees:
//!
//!   * Make the badge book-aware, so it matches the screen. Costs the badge the
//!     book-folder extraction that `detect_duplicates` needs, on a path whose
//!     comment advertises that it "re-derives on every call".
//!   * Leave the badge as a cheap file-level heuristic and change what it is
//!     called, so it stops implying "things you will find on that screen".
//!
//! These tests therefore assert the CURRENT behaviour and are written to FAIL
//! loudly if anyone changes it, so the divergence cannot be closed by accident
//! or widened in silence. See `docs/internal/audits/` for the write-up.

use abo_core::classify::classify;
use abo_core::classify::health_metrics;
use abo_core::dupes::book_folders_from_plan_nodes;
use abo_core::dupes::detect::{detect_duplicates, dupe_entries_from_plan_nodes};
use abo_core::parse::extract::{extract, EntryInput, NodeKind};
use abo_core::plan::builder::{classify_inputs_from_plan_nodes, PlanNode};
use abo_core::scan::typing::classify_path;

/// One audiobook folder holding `track_count` numbered mp3s, rooted at `path`.
///
/// Both copies of a duplicated book get identical track names AND identical
/// track sizes, which is what makes them duplicates under the `(basename, size)`
/// key both pipelines start from.
fn book(nodes: &mut Vec<PlanNode>, next_id: &mut usize, path: &str, name: &str, tracks: usize) {
    let folder_id = *next_id;
    *next_id += 1;
    nodes.push(PlanNode {
        id: folder_id,
        parent: None,
        name: name.to_string(),
        path: format!("{path}/{name}"),
        kind: NodeKind::Folder,
        file_class: None,
        size: 0,
    });

    for t in 1..=tracks {
        let track_name = format!("{t:02}.mp3");
        let id = *next_id;
        *next_id += 1;
        nodes.push(PlanNode {
            id,
            parent: Some(folder_id),
            name: track_name.clone(),
            // Deterministic per-track size, identical across both copies.
            size: 1_000_000 + (t as u64) * 1_000,
            path: format!("{path}/{name}/{track_name}"),
            file_class: Some(classify_path(std::path::Path::new(&track_name))),
            kind: NodeKind::File,
        });
    }
}

/// A library holding exactly ONE book, present twice, with `tracks` files each.
fn library_with_one_duplicated_book(tracks: usize) -> Vec<PlanNode> {
    let mut nodes = Vec::new();
    let mut next_id = 0usize;
    book(
        &mut nodes,
        &mut next_id,
        "library/Andy Weir",
        "Project Hail Mary",
        tracks,
    );
    book(
        &mut nodes,
        &mut next_id,
        "library/_incoming",
        "Project Hail Mary",
        tracks,
    );
    nodes
}

/// What the nav badge would render: `health_metrics`'s group count.
fn badge_count(nodes: &[PlanNode]) -> u64 {
    let inputs = classify_inputs_from_plan_nodes(nodes);
    let cs = classify(&inputs);
    let m = health_metrics(&inputs, &cs);
    m.problems
        .iter()
        .find(|p| p.problem == "duplicate-candidate-groups")
        .expect("the duplicate-candidate-groups metric is always emitted")
        .count
}

/// What the Duplicates screen would list: candidate groups after subsumption.
fn screen_count(nodes: &[PlanNode]) -> usize {
    let entry_inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    let merged = extract(&entry_inputs);
    let dupe_entries = dupe_entries_from_plan_nodes(nodes, &merged);
    let books = book_folders_from_plan_nodes(nodes, &merged);
    detect_duplicates(&dupe_entries, &books)
        .iter()
        .filter(|g| g.is_duplicate_candidate())
        .count()
}

/// The headline. One duplicated twelve-track book: the badge says twelve, the
/// screen shows one.
#[test]
fn the_badge_counts_tracks_where_the_screen_counts_books() {
    let nodes = library_with_one_duplicated_book(12);

    let badge = badge_count(&nodes);
    let screen = screen_count(&nodes);

    assert_eq!(
        badge, 12,
        "the badge buckets by (basename, size), so each duplicated track is its own group"
    );
    assert_eq!(
        screen, 1,
        "the screen collapses those tracks into the one duplicated book that covers them"
    );
    assert_ne!(
        badge as usize, screen,
        "RECORDED DIVERGENCE: if this now passes as equal, the two counters were \
         reconciled and this file should be deleted rather than repaired"
    );
}

/// The gap grows linearly with track count, so it is worst on exactly the books
/// most likely to be duplicated: long, many-file, unabridged rips.
#[test]
fn the_gap_widens_with_every_extra_track() {
    for tracks in [2usize, 5, 12, 40] {
        let nodes = library_with_one_duplicated_book(tracks);
        assert_eq!(
            badge_count(&nodes) as usize,
            tracks,
            "badge counts one group per duplicated track ({tracks} tracks)"
        );
        assert_eq!(
            screen_count(&nodes),
            1,
            "the screen still shows the single duplicated book ({tracks} tracks)"
        );
    }
}

/// The control case, which is what makes the divergence attributable to
/// subsumption rather than to the two pipelines simply disagreeing everywhere.
///
/// Two SINGLE-FILE books that duplicate each other have no book-level group to
/// subsume them, so both counters land on the same number. The badge is not
/// wrong in general; it is wrong exactly where a book spans more than one file.
#[test]
fn single_file_duplicates_agree_which_isolates_the_cause() {
    let mut nodes = Vec::new();
    let mut next_id = 0usize;
    book(&mut nodes, &mut next_id, "library/Andy Weir", "Artemis", 1);
    book(&mut nodes, &mut next_id, "library/_incoming", "Artemis", 1);

    assert_eq!(
        badge_count(&nodes) as usize,
        screen_count(&nodes),
        "with one file per book there is nothing to subsume, so the counters agree"
    );
}
