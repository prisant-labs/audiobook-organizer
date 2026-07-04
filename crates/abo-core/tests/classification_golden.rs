//! F-201 / F-202 / F-203 goldens (AC-201.1, AC-201.2, AC-201.3, AC-201.5,
//! AC-201.6, AC-202.1, AC-202.2, AC-203.1, AC-203.2, AC-203.3): runs the
//! classification engine, health metrics, and multi-book detection over the
//! real P1 fixture library (`crates/abo-core/src/fixtures/manifest.rs`) and
//! locks the results with committed insta snapshots, plus targeted assertions
//! for the acceptance criteria that must hold regardless of snapshot churn.
//!
//! # Why a STABLE SUBSET for the committed snapshots
//!
//! Four fixture top-level subtrees are host-variable by design (they exercise
//! extended-length / reserved-name / Unicode-collision edges and may
//! skip-with-note on a given host): `Reserved Names`, `Trailing Dot Or Space`,
//! `Near Limit`, and `Unicode Twins Same Dir`. Their presence changes the entry
//! set, so a committed snapshot over the FULL library would differ between the
//! ubuntu and windows CI legs. The committed classification and metrics
//! snapshots are therefore taken over the stable subset (these four roots
//! excluded); because each is a separate top-level subtree, excluding them does
//! not change any other folder's class. The determinism guarantee (AC-201.5) is
//! verified over the FULL library instead (two runs, byte-identical), where a
//! single host is trivially self-consistent.

use std::collections::HashMap;
use std::path::Path;

use abo_core::classify::engine::ClassifyInput;
use abo_core::classify::{classify, health_metrics, FolderClass};
use abo_core::fixtures::{generate, standard_library_manifest, GeneratedKind};
use abo_core::parse::extract::NodeKind;
use abo_core::scan::typing::classify_path;
use tempfile::TempDir;

/// Top-level fixture subtrees that are host-variable and thus excluded from the
/// committed snapshots (see module doc).
const HOST_VARIABLE_ROOTS: &[&str] = &[
    "Reserved Names",
    "Trailing Dot Or Space",
    "Near Limit",
    "Unicode Twins Same Dir",
];

/// `/`-join a relative path's components into a stable, platform-independent key.
fn key(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The generated entries as `(key, kind, size)`, optionally dropping the
/// host-variable subtrees.
struct Row {
    key: String,
    kind: GeneratedKind,
    size: u64,
}

fn rows(stable_only: bool) -> Vec<Row> {
    let manifest = standard_library_manifest();
    // A fresh tempdir per call; leaked via the returned owned data (paths are
    // not needed past this point, only the key/kind/size).
    let dir = TempDir::new().expect("tempdir");
    let lib = generate(&manifest, dir.path()).expect("generate fixture library");
    lib.entries
        .iter()
        .map(|e| Row {
            key: key(&e.relative_path),
            kind: e.kind,
            size: e.size,
        })
        .filter(|r| {
            if !stable_only {
                return true;
            }
            let top = r.key.split('/').next().unwrap_or("");
            !HOST_VARIABLE_ROOTS.contains(&top)
        })
        .collect()
}

/// Build the classifier inputs from a row set: id = index, parent resolved by
/// path prefix, file class from the F-103 extension table.
fn inputs(rows: &[Row]) -> Vec<ClassifyInput> {
    let key_to_id: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.key.as_str(), i))
        .collect();
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let parent = {
                let p = Path::new(&r.key)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .filter(|s| !s.is_empty());
                p.and_then(|pk| key_to_id.get(pk.as_str()).copied())
            };
            let (kind, file_class) = match r.kind {
                GeneratedKind::Dir => (NodeKind::Folder, None),
                GeneratedKind::File => (NodeKind::File, Some(classify_path(Path::new(&r.key)))),
            };
            ClassifyInput {
                id: i,
                parent,
                name: Path::new(&r.key)
                    .file_name()
                    .expect("entry has a name")
                    .to_string_lossy()
                    .into_owned(),
                kind,
                file_class,
                size: r.size,
            }
        })
        .collect()
}

fn id_to_key(rows: &[Row]) -> HashMap<usize, String> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| (i, r.key.clone()))
        .collect()
}

fn class_of<'a>(
    cs: &'a [abo_core::classify::FolderClassification],
    id_key: &HashMap<usize, String>,
    key: &str,
) -> &'a abo_core::classify::FolderClassification {
    cs.iter()
        .find(|c| id_key.get(&c.id).map(String::as_str) == Some(key))
        .unwrap_or_else(|| panic!("no classification for {key:?}"))
}

// ---------------------------------------------------------------------------
// AC-201.1 / AC-201.2 / AC-201.6 / AC-202.1 / AC-202.2 / AC-203.1 / AC-203.2:
// committed snapshots over the stable subset.
// ---------------------------------------------------------------------------

#[test]
fn classification_snapshot_over_the_fixture_library() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let mut lines: Vec<String> = cs
        .iter()
        .map(|c| {
            let key = &id_key[&c.id];
            let evidence = serde_json::to_string(&c.evidence).expect("evidence serializes");
            format!("{key} => {} [{}] {evidence}", c.class.as_str(), c.rule_id)
        })
        .collect();
    lines.sort();
    insta::assert_snapshot!("classification", lines.join("\n"));
}

#[test]
fn health_metrics_snapshot_over_the_fixture_library() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let m = health_metrics(&inputs, &cs);

    let mut out = String::new();
    out.push_str("per-class (folders, bytes):\n");
    for cm in &m.per_class {
        out.push_str(&format!(
            "  {:<18} folders={:<3} bytes={}\n",
            cm.class.as_str(),
            cm.folder_count,
            cm.byte_total
        ));
    }
    out.push_str("problems:\n");
    for p in &m.problems {
        out.push_str(&format!(
            "  {:<28} count={:<3} unit={:<8} bytes={}\n",
            p.problem,
            p.count,
            p.unit.as_str(),
            p.byte_total
        ));
    }
    out.push_str(&format!("total-bytes={}\n", m.total_bytes));
    insta::assert_snapshot!("health_metrics", out);
}

#[test]
fn multibook_flags_snapshot_over_the_fixture_library() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let mut lines: Vec<String> = cs
        .iter()
        .filter(|c| c.class == FolderClass::MultiBookSuspect)
        .map(|c| {
            let key = &id_key[&c.id];
            format!("{key} => titles={:?}", c.evidence.distinct_titles)
        })
        .collect();
    lines.sort();
    insta::assert_snapshot!("multibook_flags", lines.join("\n"));
}

// ---------------------------------------------------------------------------
// Targeted acceptance-criteria assertions (must hold regardless of snapshots).
// ---------------------------------------------------------------------------

/// AC-201.3 / gate G-06: the FD-17 video/course cluster and the radio play
/// classify as manual-review, not book; the cluster folder itself is
/// manual-review.
#[test]
fn fd17_video_course_and_radio_play_are_manual_review() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    for key in [
        "Video Course Cluster/52 Sales Lessons",
        "Video Course Cluster/Comics",
        "Video Course Cluster/Radio Play",
        "Video Course Cluster",
    ] {
        assert_eq!(
            class_of(&cs, &id_key, key).class,
            FolderClass::ManualReview,
            "{key} must be manual-review (FD-17)"
        );
    }
}

/// AC-201.2: every classification records a rule id (never empty) and every
/// folder is classified exactly once (AC-201.1).
#[test]
fn every_folder_has_one_class_and_a_rule_id() {
    let rows = rows(false); // full library: still every folder placed
    let inputs = inputs(&rows);
    let cs = classify(&inputs);

    let folder_count = inputs.iter().filter(|e| e.kind == NodeKind::Folder).count();
    assert_eq!(cs.len(), folder_count, "one classification per folder");
    for c in &cs {
        assert!(
            !c.rule_id.is_empty(),
            "classification {} has no rule id",
            c.id
        );
    }
    // Exactly one entry per id.
    let mut ids: Vec<usize> = cs.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), cs.len(), "each folder classified exactly once");
}

/// AC-203.1 (Narnia) and AC-203.2 (Harry Potter): both flag as
/// multi-book-suspect with the expected distinct-title counts.
#[test]
fn narnia_and_harry_potter_are_multibook() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let narnia = class_of(&cs, &id_key, "Genre - Children/Chronicles of Narnia");
    assert_eq!(narnia.class, FolderClass::MultiBookSuspect);
    assert_eq!(narnia.evidence.distinct_titles.len(), 7, "Narnia: 7 titles");

    let hp = class_of(&cs, &id_key, "Genre - Children/Harry Potter");
    assert_eq!(hp.class, FolderClass::MultiBookSuspect);
    assert_eq!(
        hp.evidence.distinct_titles.len(),
        7,
        "Harry Potter: 7 titles across 11 files: {:?}",
        hp.evidence.distinct_titles
    );
}

/// F-203, the Wings of Fire fixture (`FixtureFamily::MultiBookWingsOfFire`): a
/// series distinguished PRIMARILY by a leading series index still flags as
/// multi-book-suspect, counted by its five real distinct per-book titles (the
/// series name itself, "Wings of Fire", never appears in the count).
#[test]
fn wings_of_fire_is_multibook() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let wof = class_of(&cs, &id_key, "Genre - Children/Wings of Fire");
    assert_eq!(wof.class, FolderClass::MultiBookSuspect);
    assert_eq!(
        wof.evidence.distinct_titles.len(),
        5,
        "Wings of Fire: 5 titles across 5 files: {:?}",
        wof.evidence.distinct_titles
    );
    assert!(
        !wof.evidence
            .distinct_titles
            .iter()
            .any(|t| t == "wings of fire"),
        "the shared series name must not itself be counted as a title: {:?}",
        wof.evidence.distinct_titles
    );
}

/// AC-203.3 (no false positives): a disc-based single book (Verbal Advantage)
/// and a single-book-plus-bonus shape are NOT flagged multi-book.
#[test]
fn disc_split_and_single_plus_bonus_are_not_multibook() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let verbal = class_of(&cs, &id_key, "Genre - Non-Fiction/Verbal Advantage");
    assert_eq!(
        verbal.class,
        FolderClass::Book,
        "a disc-split single book is one book, never multi-book"
    );
    // Its disc children are books (audio leaves), not multi-book suspects.
    for disc in ["Disc 1", "Disc 2", "CD3", "Disk_04"] {
        let k = format!("Genre - Non-Fiction/Verbal Advantage/{disc}");
        assert_eq!(
            class_of(&cs, &id_key, &k).class,
            FolderClass::Book,
            "{k} is a disc part (book), never multi-book"
        );
    }
}

/// Brief note 2 (pattern-2 vs pattern-9 by content): a pattern-9-NAMED folder
/// with two distinct-title files inside is a multi-book-suspect; one with a
/// single title inside is a book. Neither is a container.
#[test]
fn pattern9_named_folders_resolve_by_content() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    let dune = class_of(
        &cs,
        &id_key,
        "Genre - SciFI/Frank Herbert-Dune-#1-Chronicles[1-8}",
    );
    assert_eq!(
        dune.class,
        FolderClass::MultiBookSuspect,
        "two distinct titles inside -> multi-book, not a container"
    );

    let charlie = class_of(
        &cs,
        &id_key,
        "Genre - Children/Roald Dahl - Charlie and the Chocolate Factory",
    );
    assert_eq!(
        charlie.class,
        FolderClass::Book,
        "one title inside -> book, not a container"
    );
}

/// Genre shelves and real series containers are distinguished, and a real
/// series container's book keeps its inherited author while a shelf's book has
/// its shelf-derived author suppressed (P4 consumption note 1).
#[test]
fn shelves_are_packs_and_real_series_are_series_containers() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let id_key = id_to_key(&rows);

    for shelf in [
        "Genre - SciFI",
        "Genre - Non-Fiction",
        "Genre - Business",
        "Genre - Children",
        "Genre - SciFI/Hugo Collection",
        "Genre - SciFI/Top 100 Sci-Fi Books",
    ] {
        assert_eq!(
            class_of(&cs, &id_key, shelf).class,
            FolderClass::PackContainer,
            "{shelf} is a collection (pack-container)"
        );
    }

    let dresden = class_of(
        &cs,
        &id_key,
        "Genre - SciFI/Hugo Collection/0 Series Books/Jim Butcher - Dresden Files 22GB",
    );
    assert_eq!(
        dresden.class,
        FolderClass::SeriesContainer,
        "indexed children under one author -> series-container"
    );

    // A book under the real series container keeps its inherited author.
    let cold = class_of(
        &cs,
        &id_key,
        "Genre - SciFI/Hugo Collection/0 Series Books/Jim Butcher - Dresden Files 22GB/14.0 - Cold Days [320k 18;48;03 2.52GB]",
    );
    assert_eq!(
        cold.evidence.clean_author.as_deref(),
        Some("Jim Butcher"),
        "a book under a real series container keeps its inherited author"
    );

    // A bare book under a genre shelf has its shelf-derived author suppressed.
    let rework = class_of(&cs, &id_key, "Genre - Non-Fiction/Rework");
    assert_eq!(
        rework.evidence.clean_author, None,
        "a bare book under a shelf drops the spurious shelf author"
    );
}

/// AC-201.5: classification is deterministic - the full library classified
/// twice is byte-identical.
#[test]
fn classification_is_deterministic_over_full_library() {
    let rows = rows(false);
    let inputs = inputs(&rows);
    let first = classify(&inputs);
    let second = classify(&inputs);
    assert_eq!(first, second, "same snapshot must classify identically");
}

/// AC-202.1 with AC-F5: byte attribution single-counts - per-class bytes plus
/// loose-root bytes equal the library total, which equals the summed file
/// sizes.
#[test]
fn metric_bytes_tie_out_to_the_library_total() {
    let rows = rows(true);
    let inputs = inputs(&rows);
    let cs = classify(&inputs);
    let m = health_metrics(&inputs, &cs);

    let declared: u64 = rows
        .iter()
        .filter(|r| matches!(r.kind, GeneratedKind::File))
        .map(|r| r.size)
        .sum();
    assert_eq!(
        m.total_bytes, declared,
        "total bytes equals summed file sizes"
    );

    let class_bytes: u64 = m.per_class.iter().map(|c| c.byte_total).sum();
    let loose = m
        .problems
        .iter()
        .find(|p| p.problem == "loose-root-books")
        .unwrap()
        .byte_total;
    assert_eq!(
        class_bytes + loose,
        m.total_bytes,
        "per-class bytes plus loose-root bytes single-count the library total"
    );
}
