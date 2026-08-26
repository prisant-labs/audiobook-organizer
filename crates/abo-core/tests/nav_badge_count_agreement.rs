//! Does the Duplicates nav badge count the same thing the Duplicates screen shows?
//!
//! It does now, and this file is the proof. It used to be the proof of the
//! opposite.
//!
//! # What was wrong
//!
//! `AC-29` (v0.6.0 hardening) puts a count on the Duplicates nav item. A person
//! reads that number, clicks it, and expects to find that many things. Two
//! separate implementations of "a duplicate group" fed the two halves of that
//! sentence:
//!
//!   * **The badge** read `health_metrics`'s duplicate metric
//!     (`classify/metrics.rs`), which buckets FILES by `(basename, size)`. It
//!     knows nothing about books.
//!   * **The screen** reads `dupes::review`, built on `detect_duplicates`, which
//!     ALSO groups duplicated BOOK FOLDERS and then marks the per-track groups
//!     those folders already cover as `subsumed_by_book_group`.
//!
//! So a twelve-file audiobook present twice was `12` to the badge and `1` to the
//! screen. On jp's real library that was 406 against 300, measured and
//! reconciled exactly in
//! `docs/internal/audits/2026-08-21_nav-badge-count-divergence.md`.
//!
//! # What the fix was, and what it deliberately was NOT
//!
//! The audit offered making `health_metrics` book-aware. That would have built a
//! SECOND book-aware duplicate counter, which is the disease and not the cure:
//! the divergence existed because one rule had two implementations checked
//! against each other rather than against the world.
//!
//! Instead there is still exactly ONE implementation of "a duplicate group is a
//! book", in `dupes::detect`, and `classify::health_metrics_for_scan` makes the
//! health metrics QUOTE it rather than paraphrase it. `health_metrics` itself is
//! unchanged and still counts tracks, which
//! `the_pure_metric_still_counts_tracks_which_is_what_makes_the_seam_the_fix`
//! below asserts on purpose: it proves the agreement comes from the seam, so
//! nobody can "simplify" a caller back onto the bare function and quietly
//! restore the defect.
//!
//! # Why these tests go through the real entry points
//!
//! Both sides are read the way the product reads them - the badge through
//! `health_metrics_for_scan` (what `classify_overview` serves to `navCountsFrom`)
//! and the screen through `review_for_scan(..).group_count()` (`AC-18`'s headline
//! number). A test that re-implemented either side would be a fifth counter, and
//! counting a thing a fifth way is how this started.

use abo_core::classify::{
    classify, health_metrics, health_metrics_for_scan, inputs_from_snapshot,
    DUPLICATE_CANDIDATE_GROUPS,
};
use abo_core::db::open_db;
use abo_core::dupes::{review_for_scan, ResolutionPolicy};
use abo_core::scan::{get_scan_entries, run_scan};
use sqlx::SqlitePool;
use tempfile::TempDir;

/// A library holding exactly ONE book, present twice, with `tracks` files each.
///
/// Both copies get identical track names AND identical track sizes, which is
/// what makes them duplicates under the `(basename, size)` key both pipelines
/// start from. Sizes stay small deliberately: detection keys on the size value,
/// never on it being large, so a kilobyte proves the same thing a megabyte would
/// and does not make the 40-track case write 80 MB.
fn library_with_one_duplicated_book(tracks: usize) -> TempDir {
    let lib = TempDir::new().expect("library tempdir");
    for shelf in ["Andy Weir", "_incoming"] {
        let dir = lib.path().join(shelf).join("Project Hail Mary");
        std::fs::create_dir_all(&dir).expect("create book folder");
        for t in 1..=tracks {
            // Deterministic per-track size, identical across both copies.
            let body = vec![b'x'; 1_024 + t * 16];
            std::fs::write(dir.join(format!("{t:02}.mp3")), &body).expect("write track");
        }
    }
    lib
}

async fn scan_of(lib: &TempDir) -> (TempDir, SqlitePool, i64) {
    let db = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db.path()).await.expect("open_db");
    let summary = run_scan(&pool, lib.path()).await.expect("scan the library");
    (db, pool, summary.scan_id)
}

/// What the nav badge renders: the duplicate metric out of the SAME payload
/// `classify_overview` hands `navCountsFrom` (and the Library home).
async fn badge_count(pool: &SqlitePool, scan_id: i64) -> u64 {
    let rows = get_scan_entries(pool, scan_id).await.expect("snapshot");
    let inputs = inputs_from_snapshot(&rows);
    let cs = classify(&inputs);
    let m = health_metrics_for_scan(pool, scan_id, &inputs, &cs)
        .await
        .expect("health metrics for the scan");
    m.problems
        .iter()
        .find(|p| p.problem == DUPLICATE_CANDIDATE_GROUPS)
        .expect("the duplicate metric is always emitted")
        .count
}

/// What the Duplicates screen lists: `AC-18`'s headline group count, read
/// through the same function the surface calls.
async fn screen_count(pool: &SqlitePool, scan_id: i64) -> usize {
    review_for_scan(
        pool,
        scan_id,
        std::path::MAIN_SEPARATOR,
        ResolutionPolicy::FlagOnly,
    )
    .await
    .expect("the duplicates review")
    .group_count()
}

/// The headline, inverted from what it used to assert. One duplicated
/// twelve-track book is ONE duplicated book on both surfaces.
#[tokio::test]
async fn the_badge_counts_books_just_as_the_screen_does() {
    let lib = library_with_one_duplicated_book(12);
    let (_db, pool, scan_id) = scan_of(&lib).await;

    let badge = badge_count(&pool, scan_id).await;
    let screen = screen_count(&pool, scan_id).await;

    assert_eq!(
        screen, 1,
        "the screen collapses twelve duplicated tracks into the one duplicated book"
    );
    assert_eq!(
        badge as usize, screen,
        "AC-29: the number beside Duplicates is a promise about the Duplicates screen"
    );
}

/// The old divergence grew linearly with track count, so it was worst on exactly
/// the books most likely to be duplicated: long, many-file, unabridged rips.
/// Checked at the same four widths the divergence was checked at.
#[tokio::test]
async fn the_count_no_longer_widens_with_every_extra_track() {
    for tracks in [2usize, 5, 12, 40] {
        let lib = library_with_one_duplicated_book(tracks);
        let (_db, pool, scan_id) = scan_of(&lib).await;

        let badge = badge_count(&pool, scan_id).await;
        let screen = screen_count(&pool, scan_id).await;

        assert_eq!(
            screen, 1,
            "still the single duplicated book at {tracks} tracks"
        );
        assert_eq!(
            badge as usize, screen,
            "the badge must not grow with track count ({tracks} tracks)"
        );
    }
}

/// The control, kept from the divergence file because it is what makes the
/// agreement attributable.
///
/// Two SINGLE-FILE books that duplicate each other have no book-level group to
/// subsume them, so both counters landed on the same number even before the fix.
/// It must still agree afterwards: a "fix" that moved this one would have
/// changed what a duplicate IS rather than how it is counted. Both sides are 1,
/// never 0, so this cannot pass by both counters finding nothing.
#[tokio::test]
async fn single_file_duplicates_still_agree_which_isolates_the_cause() {
    let lib = TempDir::new().expect("library tempdir");
    for shelf in ["Andy Weir", "_incoming"] {
        let dir = lib.path().join(shelf);
        std::fs::create_dir_all(&dir).expect("create shelf");
        std::fs::write(dir.join("Artemis.m4b"), vec![b'x'; 2_048]).expect("write book");
    }
    let (_db, pool, scan_id) = scan_of(&lib).await;

    let badge = badge_count(&pool, scan_id).await;
    let screen = screen_count(&pool, scan_id).await;

    assert_eq!(screen, 1, "one duplicated single-file book");
    assert_eq!(
        badge as usize, screen,
        "with one file per book there is nothing to subsume, so the counters agree"
    );
}

/// The seam is what does the work, and this is the assertion that says so.
///
/// `health_metrics` is UNCHANGED and still counts one group per duplicated
/// track. The badge agrees with the screen only because every user-facing caller
/// goes through `health_metrics_for_scan`. If someone "simplifies" a caller back
/// onto the bare function, the badge silently returns to counting tracks, and
/// the three tests above would catch it only because of what this one pins:
/// that the two functions genuinely differ, so choosing between them matters.
///
/// The day the pure function is itself made book-aware, this test fails and
/// should be read carefully rather than deleted - a second book-aware
/// implementation is the shape the whole fix exists to avoid.
#[tokio::test]
async fn the_pure_metric_still_counts_tracks_which_is_what_makes_the_seam_the_fix() {
    let lib = library_with_one_duplicated_book(12);
    let (_db, pool, scan_id) = scan_of(&lib).await;

    let rows = get_scan_entries(&pool, scan_id).await.expect("snapshot");
    let inputs = inputs_from_snapshot(&rows);
    let cs = classify(&inputs);

    let pure = health_metrics(&inputs, &cs)
        .problems
        .iter()
        .find(|p| p.problem == DUPLICATE_CANDIDATE_GROUPS)
        .expect("the duplicate metric is always emitted")
        .count;

    assert_eq!(
        pure, 12,
        "health_metrics still buckets by (basename, size): twelve tracks, twelve groups"
    );
    assert_eq!(
        badge_count(&pool, scan_id).await,
        1,
        "and the seam is what turns those twelve into the one book the screen shows"
    );
}
