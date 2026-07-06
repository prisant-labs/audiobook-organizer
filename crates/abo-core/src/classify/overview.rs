//! F-902 (library home): turn a classified snapshot into the [`LibraryOverview`]
//! the `classify_overview` IPC command returns. This is where the home
//! screen's prose numbers, the "Worth a look first" shelf, the series
//! clusters, and the good-news line all come from - every one derived from the
//! snapshot at call time, never a hardcoded sample (AC-7, FD-27).
//!
//! Like the rest of `crate::classify`, this module is pure logic: no I/O, no
//! filesystem, nothing `cfg`-gated (the CFG RULE). [`build_overview`] consumes
//! the SAME [`ClassifyInput`]/[`FolderClassification`] shapes [`classify`] and
//! [`health_metrics`] already operate on, plus the [`HealthMetrics`] those
//! produce, so it adds no new engine computation - it reshapes existing
//! results for the home screen.
//!
//! # What counts as "a book" (`total_books`)
//!
//! Three sources, summed, with no double counting:
//!   1. Every `book`-class folder, EXCLUDING a disc-split part (a `book`
//!      folder whose parent is also `book`; see `metrics::health_metrics`'s
//!      own fold) - this is exactly [`HealthMetrics`]'s `book` per-class
//!      `folder_count`.
//!   2. Every loose-root audio file (no book/container folder around it) -
//!      exactly the `"loose-root-books"` problem's `count`.
//!   3. Every DISTINCT title F-203 found inside a `multi-book-suspect`
//!      folder ([`Evidence::distinct_titles`]) - a box set of 7 books sitting
//!      as sibling files in one folder counts as 7 books, not 1.
//!
//! # What counts as "worth a look" (`needs_tidy_books`)
//!
//! A book NEEDS a tidy when it is: a loose-root file; a `book` folder whose
//! name carries strippable noise (the same noise definition `health_metrics`
//! uses for its `"noisy-names"` problem); a `multi-book-suspect` folder (every
//! book inside is trivially "not in its own folder yet"); or a `book` folder
//! that holds a member of a duplicate candidate group. These are tracked as a
//! set of entry ids (a book folder counted once even if it is BOTH noisy and
//! has a duplicate copy), so `needs_tidy_books` never double-counts a single
//! book. One documented approximation: a multi-book-suspect folder counts as
//! ONE "needs tidy" id here (its books all need the same fix at once), even
//! though `total_books` above counts its distinct titles individually - the
//! two numbers answer different questions ("how many books" vs "how many
//! books have a problem right now") and are not required to be title-for-title
//! reconciled.
//!
//! # Worth-a-look shelf: curated, not exhaustive
//!
//! [`LibraryOverview::worth_a_look`] is capped at [`WORTH_A_LOOK_CAP`] examples
//! (design-system Section 4.7: "a few examples of what the tidy-up would
//! fix", with a "See all N" link to the full review surface). The full,
//! responsive-over-800+-books list is Phase 5's virtualized review surface and
//! the exported report (D-16); the home shelf is deliberately bounded and
//! needs no virtualization.

use std::collections::{HashMap, HashSet};

use crate::classify::engine::{Evidence, FolderClass, FolderClassification};
use crate::classify::metrics::has_noise;
use crate::classify::{ClassifyInput, HealthMetrics};
use crate::ipc::{BookExample, BookReason, GoodNews, LibraryOverview, ReasonKind, SeriesCluster};
use crate::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};

/// The maximum number of examples on the "Worth a look first" shelf (see the
/// module doc). Chosen to comfortably fill a horizontally-scrolling shelf row
/// without approaching list-scale (the full list is Phase 5's concern).
const WORTH_A_LOOK_CAP: usize = 24;

/// Build the [`LibraryOverview`] for one classified snapshot.
///
/// `scan_id` is the snapshot's id (carried through so [`BookExample::entry_id`]
/// can be resolved back to a cover via `cover_get`); `entries` and
/// `classifications` are the same pair [`crate::classify::health_metrics`]
/// takes, and `metrics` is that call's own output (the caller computes it
/// once and hands it to both - see `commands::classify_overview` in
/// `src-tauri`).
pub fn build_overview(
    scan_id: i64,
    entries: &[ClassifyInput],
    classifications: &[FolderClassification],
    metrics: &HealthMetrics,
) -> LibraryOverview {
    let entry_by_id: HashMap<usize, &ClassifyInput> = entries.iter().map(|e| (e.id, e)).collect();
    let class_by_id: HashMap<usize, FolderClass> =
        classifications.iter().map(|c| (c.id, c.class)).collect();

    // The F-303 merge, so title/author come from the SAME parse the classifier
    // itself consulted (this mirrors `plan::builder`'s own re-parse of merged
    // fields; there is no cheaper way to recover title/author than re-running
    // the pure merge, since `Evidence` only carries the SUPPRESSED author,
    // never the title).
    let merged: Vec<MergedEntry> = extract(&to_entry_inputs(entries));
    let merged_by_id: HashMap<usize, &MergedEntry> = merged.iter().map(|m| (m.id, m)).collect();

    let per_class_book_folders = metrics
        .per_class
        .iter()
        .find(|c| c.class == FolderClass::Book)
        .map(|c| c.folder_count)
        .unwrap_or(0);
    let loose_root_count = problem_count(metrics, "loose-root-books");
    let multibook_title_sum: u64 = classifications
        .iter()
        .filter(|c| c.class == FolderClass::MultiBookSuspect)
        .map(|c| c.evidence.distinct_titles.len().max(1) as u64)
        .sum();
    let total_books = per_class_book_folders + loose_root_count + multibook_title_sum;

    let book_dup_copies = duplicate_copies_by_book_folder(entries, &class_by_id);

    let mut needs_tidy_ids: HashSet<usize> = HashSet::new();
    let mut worth_a_look: Vec<BookExample> = Vec::new();

    // Loose-root files: each one is, by definition, not yet in its own folder.
    for e in loose_root_entries(entries) {
        needs_tidy_ids.insert(e.id);
        let m = merged_by_id.get(&e.id).copied();
        worth_a_look.push(BookExample {
            entry_id: e.id as i64,
            title: title_of(m, &e.name),
            author: m
                .and_then(|m| m.fields.author.as_ref())
                .map(|f| f.value.clone()),
            reason: BookReason {
                kind: ReasonKind::Warn,
                text: "loose file".to_string(),
            },
        });
    }

    // Messy-named book folders and duplicate-holding book folders (a folder
    // can be both; `needs_tidy_ids` de-dupes, but each condition still gets
    // its own shelf card so a shopper sees both signals if both apply to
    // different books).
    for c in classifications {
        if c.class != FolderClass::Book || is_disc_part(c.id, entries, &class_by_id) {
            continue;
        }
        let Some(entry) = entry_by_id.get(&c.id) else {
            continue;
        };
        let m = merged_by_id.get(&c.id).copied();
        let title = title_of(m, &entry.name);
        let author = clean_or_merged_author(&c.evidence, m);

        if has_noise(&entry.name) {
            needs_tidy_ids.insert(c.id);
            worth_a_look.push(BookExample {
                entry_id: c.id as i64,
                title: title.clone(),
                author: author.clone(),
                reason: BookReason {
                    kind: ReasonKind::Warn,
                    text: "messy name".to_string(),
                },
            });
        }
        if let Some(&copies) = book_dup_copies.get(&c.id) {
            needs_tidy_ids.insert(c.id);
            worth_a_look.push(BookExample {
                entry_id: c.id as i64,
                title,
                author,
                reason: BookReason {
                    kind: ReasonKind::Alert,
                    text: copies_chip(copies),
                },
            });
        }
    }

    // Multi-book-suspect folders: a box set / bundle sitting as sibling files.
    for c in classifications {
        if c.class != FolderClass::MultiBookSuspect {
            continue;
        }
        needs_tidy_ids.insert(c.id);
        let Some(entry) = entry_by_id.get(&c.id) else {
            continue;
        };
        let m = merged_by_id.get(&c.id).copied();
        let book_count = c.evidence.distinct_titles.len().max(1);
        worth_a_look.push(BookExample {
            entry_id: c.id as i64,
            title: title_of(m, &entry.name),
            author: c.evidence.own_author.clone().or_else(|| {
                m.and_then(|m| m.fields.author.as_ref())
                    .map(|f| f.value.clone())
            }),
            reason: BookReason {
                kind: ReasonKind::Alert,
                text: format!(
                    "{book_count} book{}, 1 folder",
                    if book_count == 1 { "" } else { "s" }
                ),
            },
        });
    }

    worth_a_look.truncate(WORTH_A_LOOK_CAP);

    let needs_tidy_books = needs_tidy_ids.len() as u64;
    let already_tidy_books = total_books.saturating_sub(needs_tidy_books);

    let series: Vec<SeriesCluster> = classifications
        .iter()
        .filter(|c| c.class == FolderClass::SeriesContainer)
        .filter_map(|c| {
            let entry = entry_by_id.get(&c.id)?;
            Some(SeriesCluster {
                name: c
                    .evidence
                    .own_series
                    .clone()
                    .unwrap_or_else(|| entry.name.clone()),
                author: c.evidence.own_author.clone(),
                book_count: c.evidence.book_children,
            })
        })
        .collect();

    let good_news = GoodNews {
        already_tidy_books,
        series_shelved: series.len() as u64,
        empty_folders: problem_count(metrics, "empty-folders"),
        duplicate_groups: problem_count(metrics, "duplicate-candidate-groups"),
        duplicate_bytes: problem_bytes(metrics, "duplicate-candidate-groups"),
    };

    LibraryOverview {
        scan_id,
        total_books,
        total_bytes: metrics.total_bytes,
        needs_tidy_books,
        worth_a_look,
        series,
        good_news,
        metrics: metrics.clone(),
    }
}

fn to_entry_inputs(entries: &[ClassifyInput]) -> Vec<EntryInput> {
    entries
        .iter()
        .map(|e| EntryInput {
            id: e.id,
            parent: e.parent,
            name: e.name.clone(),
            kind: e.kind,
        })
        .collect()
}

/// A book folder's effective title: its own-name parse when present, else the
/// raw folder/file name verbatim (fabricate nothing beyond what F-303 already
/// read; a shelf card always shows SOME text).
fn title_of(merged: Option<&MergedEntry>, fallback_name: &str) -> String {
    merged
        .and_then(|m| m.fields.title.as_ref())
        .map(|f| f.value.clone())
        .unwrap_or_else(|| fallback_name.to_string())
}

/// A book folder's author: the classifier's shelf-suppressed clean value when
/// present (never the raw, possibly-spurious inherited one - see
/// `plan::builder::book_fields_from_folder`'s identical rule), else the raw
/// merged author as a last resort so the card is not blank when suppression
/// dropped a genuinely useful value.
fn clean_or_merged_author(evidence: &Evidence, merged: Option<&MergedEntry>) -> Option<String> {
    evidence.clean_author.clone().or_else(|| {
        merged
            .and_then(|m| m.fields.author.as_ref())
            .map(|f| f.value.clone())
    })
}

fn copies_chip(copies: u64) -> String {
    format!("{copies} cop{}", if copies == 1 { "y" } else { "ies" })
}

fn problem_count(metrics: &HealthMetrics, problem: &str) -> u64 {
    metrics
        .problems
        .iter()
        .find(|p| p.problem == problem)
        .map(|p| p.count)
        .unwrap_or(0)
}

fn problem_bytes(metrics: &HealthMetrics, problem: &str) -> u64 {
    metrics
        .problems
        .iter()
        .find(|p| p.problem == problem)
        .map(|p| p.byte_total)
        .unwrap_or(0)
}

/// Whether folder `id` is a disc-split part (a `book`-class folder whose
/// parent is ALSO `book`-class) - the same fold `health_metrics` applies so a
/// disc-split single book is not counted, or shelved, twice.
fn is_disc_part(
    id: usize,
    entries: &[ClassifyInput],
    class_by_id: &HashMap<usize, FolderClass>,
) -> bool {
    let Some(entry) = entries.iter().find(|e| e.id == id) else {
        return false;
    };
    entry
        .parent
        .and_then(|p| class_by_id.get(&p))
        .map(|pc| *pc == FolderClass::Book)
        .unwrap_or(false)
}

/// The loose-root audio files, under the exact same two-convention rule
/// [`crate::classify::metrics::health_metrics`] documents at its computation
/// site (fixture multi-root shape vs a real single-rooted scan).
fn loose_root_entries(entries: &[ClassifyInput]) -> Vec<&ClassifyInput> {
    let parentless: Vec<&ClassifyInput> = entries.iter().filter(|e| e.parent.is_none()).collect();
    let scan_root_id: Option<usize> = match parentless.as_slice() {
        [only] if only.kind == NodeKind::Folder => Some(only.id),
        _ => None,
    };
    entries
        .iter()
        .filter(|e| {
            e.kind == NodeKind::File
                && e.file_class == Some(crate::scan::typing::FileClass::Audio)
                && match scan_root_id {
                    Some(root_id) => e.parent == Some(root_id),
                    None => e.parent.is_none(),
                }
        })
        .collect()
}

/// For every duplicate candidate group (files sharing basename+size, >= 2
/// members - the SAME Codex basename+size method `health_metrics` uses,
/// candidates only, FD-08 GROUP unit), map each participating BOOK folder to
/// the group's member count, so a book folder that holds one copy of a
/// 2-member group shows "2 copies" (the group total), not "1". A book folder
/// touched by more than one group keeps the LARGEST group's count (a
/// deliberate, documented simplification - the exact multi-group case is rare
/// and the Phase 5 review surface shows the precise per-operation detail).
fn duplicate_copies_by_book_folder(
    entries: &[ClassifyInput],
    class_by_id: &HashMap<usize, FolderClass>,
) -> HashMap<usize, u64> {
    let mut groups: HashMap<(String, u64), Vec<usize>> = HashMap::new();
    for e in entries {
        if e.kind == NodeKind::File {
            if let Some(parent) = e.parent {
                if class_by_id.get(&parent) == Some(&FolderClass::Book) {
                    groups
                        .entry((e.name.clone(), e.size))
                        .or_default()
                        .push(parent);
                }
            }
        }
    }
    let mut by_folder: HashMap<usize, u64> = HashMap::new();
    for (_key, folder_ids) in groups.iter().filter(|(_, v)| v.len() >= 2) {
        let group_size = folder_ids.len() as u64;
        for &folder_id in folder_ids {
            let slot = by_folder.entry(folder_id).or_insert(0);
            *slot = (*slot).max(group_size);
        }
    }
    by_folder
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::classify::metrics::health_metrics;
    use crate::scan::typing::FileClass;

    fn folder(id: usize, parent: Option<usize>, name: &str) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }
    }

    fn audio(id: usize, parent: Option<usize>, name: &str, size: u64) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::File,
            file_class: Some(FileClass::Audio),
            size,
        }
    }

    fn overview_for(entries: &[ClassifyInput]) -> LibraryOverview {
        let cs = classify(entries);
        let m = health_metrics(entries, &cs);
        build_overview(1, entries, &cs, &m)
    }

    /// AC-202.3-style baseline: a tidy library with two clean author-first
    /// books has zero problems and both books read as already tidy.
    #[test]
    fn tidy_library_has_no_worth_a_look_examples() {
        let entries = vec![
            folder(0, None, "Andy Weir - Project Hail Mary"),
            audio(1, Some(0), "Project Hail Mary.m4b", 175_000),
            folder(2, None, "James Clear - Atomic Habits"),
            audio(3, Some(2), "Atomic Habits.m4b", 140_000),
        ];
        let ov = overview_for(&entries);
        assert_eq!(ov.total_books, 2);
        assert_eq!(ov.needs_tidy_books, 0);
        assert_eq!(ov.good_news.already_tidy_books, 2);
        assert!(ov.worth_a_look.is_empty());
        assert_eq!(ov.total_bytes, 315_000);
    }

    /// A loose-root audio file surfaces as a "loose file" example, its title
    /// and author read from its own name (pattern-1 `Title by Author`).
    #[test]
    fn loose_root_file_is_a_loose_file_example() {
        let entries = vec![audio(0, None, "Sapiens by Yuval Noah Harari.m4b", 180_000)];
        let ov = overview_for(&entries);
        assert_eq!(ov.total_books, 1);
        assert_eq!(ov.needs_tidy_books, 1);
        assert_eq!(ov.worth_a_look.len(), 1);
        let ex = &ov.worth_a_look[0];
        assert_eq!(ex.title, "Sapiens");
        assert_eq!(ex.author.as_deref(), Some("Yuval Noah Harari"));
        assert_eq!(ex.reason.kind, ReasonKind::Warn);
        assert_eq!(ex.reason.text, "loose file");
    }

    /// A noisy folder name (bracketed release tags) yields a "messy name"
    /// example whose author/title still parse cleanly underneath the noise.
    ///
    /// A second, unrelated top-level folder keeps this fixture in the
    /// "multi-root" convention `loose_root_entries` documents (more than one
    /// parentless entry): with only the noisy folder present, it would be the
    /// SOLE parentless Folder and read as the scan root itself, making its
    /// audio child "loose" too - a different, unintended condition.
    #[test]
    fn noisy_book_folder_is_a_messy_name_example() {
        let entries = vec![
            folder(0, None, "Jim Butcher - Cold Days [320k 18;48;03 2.52GB]"),
            audio(1, Some(0), "Cold Days.m4b", 100_000),
            folder(2, None, "Andy Weir - Project Hail Mary"),
            audio(3, Some(2), "Project Hail Mary.m4b", 175_000),
        ];
        let ov = overview_for(&entries);
        assert_eq!(ov.needs_tidy_books, 1);
        let ex = ov
            .worth_a_look
            .iter()
            .find(|b| b.reason.text == "messy name")
            .expect("a messy-name example");
        assert_eq!(ex.title, "Cold Days");
        assert_eq!(ex.author.as_deref(), Some("Jim Butcher"));
        assert_eq!(ex.reason.kind, ReasonKind::Warn);
    }

    /// Two book folders sharing a duplicate basename+size file each get a "2
    /// copies" example (the group total, not a per-folder count of 1).
    #[test]
    fn duplicate_copy_across_two_book_folders_yields_two_copies_chip() {
        let entries = vec![
            folder(0, None, "Author A - Island"),
            audio(1, Some(0), "Island.m4b", 120_000),
            folder(2, None, "Author A - Island Backup"),
            audio(3, Some(2), "Island.m4b", 120_000),
        ];
        let ov = overview_for(&entries);
        let copy_examples: Vec<_> = ov
            .worth_a_look
            .iter()
            .filter(|b| b.reason.text.contains("cop"))
            .collect();
        assert_eq!(
            copy_examples.len(),
            2,
            "one example per participating folder"
        );
        for ex in copy_examples {
            assert_eq!(ex.reason.text, "2 copies");
            assert_eq!(ex.reason.kind, ReasonKind::Alert);
        }
        assert_eq!(ov.needs_tidy_books, 2);
    }

    /// A multi-book-suspect folder (several sibling audio files with distinct
    /// titles) contributes its title count to `total_books` but counts as ONE
    /// "needs tidy" id, and its shelf chip reports the book count in words.
    #[test]
    fn multibook_suspect_folder_counts_titles_but_one_tidy_id() {
        let entries = vec![
            folder(0, None, "Assorted Box Set"),
            audio(1, Some(0), "First Book.m4b", 100_000),
            audio(2, Some(0), "Second Book.m4b", 100_000),
            audio(3, Some(0), "Third Book.m4b", 100_000),
            // A second top-level folder, for the same reason as the
            // messy-name test above: keeps this in the unambiguous
            // "multi-root" fixture convention rather than making folder 0
            // read as the scan root itself (which would make its audio
            // children "loose").
            folder(4, None, "Andy Weir - Project Hail Mary"),
            audio(5, Some(4), "Project Hail Mary.m4b", 175_000),
        ];
        let ov = overview_for(&entries);
        let ex = ov
            .worth_a_look
            .iter()
            .find(|b| b.reason.kind == ReasonKind::Alert && b.reason.text.contains("folder"))
            .expect("a box-set example");
        assert!(
            ex.reason.text.contains("books, 1 folder"),
            "{}",
            ex.reason.text
        );
        assert_eq!(ov.needs_tidy_books, 1);
    }

    /// A real series container surfaces as a `SeriesCluster` with its series
    /// name, author, and book count - never inflating `worth_a_look` (a
    /// well-organized series is good news, not a problem).
    #[test]
    fn series_container_becomes_a_series_cluster() {
        let entries = vec![
            folder(0, None, "Jim Butcher - The Dresden Files"),
            folder(1, Some(0), "Jim Butcher - Storm Front"),
            audio(2, Some(1), "Storm Front.m4b", 150_000),
            folder(3, Some(0), "Jim Butcher - Fool Moon"),
            audio(4, Some(3), "Fool Moon.m4b", 150_000),
        ];
        let ov = overview_for(&entries);
        assert_eq!(ov.series.len(), 1);
        assert_eq!(ov.good_news.series_shelved, 1);
        assert_eq!(ov.series[0].author.as_deref(), Some("Jim Butcher"));
        assert_eq!(ov.series[0].book_count, 2);
    }

    /// Empty folders and duplicate bytes flow straight from the same
    /// `HealthMetrics` the sidebar/badges read, so the good-news line and the
    /// Duplicates badge never disagree.
    #[test]
    fn good_news_reads_empties_and_duplicate_bytes_from_metrics() {
        let entries = vec![
            folder(0, None, "Empty Folder"),
            folder(1, None, "Author A - Island"),
            audio(2, Some(1), "Island.m4b", 120_000),
            folder(3, None, "Author A - Island Backup"),
            audio(4, Some(3), "Island.m4b", 120_000),
        ];
        let ov = overview_for(&entries);
        assert_eq!(ov.good_news.empty_folders, 1);
        assert_eq!(ov.good_news.duplicate_groups, 1);
        assert_eq!(ov.good_news.duplicate_bytes, 240_000);
        assert_eq!(ov.metrics.total_bytes, ov.total_bytes);
    }

    /// The worth-a-look shelf never exceeds its cap even against a library
    /// with far more problem books than the shelf shows (the full list lives
    /// in the Phase 5 review surface, D-16).
    #[test]
    fn worth_a_look_is_capped() {
        let mut entries = Vec::new();
        for i in 0..(WORTH_A_LOOK_CAP as u64 + 10) {
            entries.push(audio(
                i as usize,
                None,
                &format!("Loose Book {i} by Some Author.m4b"),
                50_000,
            ));
        }
        let ov = overview_for(&entries);
        assert_eq!(ov.worth_a_look.len(), WORTH_A_LOOK_CAP);
        assert_eq!(ov.needs_tidy_books, WORTH_A_LOOK_CAP as u64 + 10);
    }
}
