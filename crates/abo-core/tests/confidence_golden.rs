//! F-303 confidence golden (AC-303.1, AC-303.2, AC-303.3): runs the
//! field-extraction confidence merge (`abo_core::parse::extract`) over the
//! real P1 fixture library (`crates/abo-core/src/fixtures/manifest.rs`) and
//! asserts, per representative entry, the merged fields, their confidence
//! tier and source, and the conflicts the merge refused to silently resolve.
//!
//! Table-driven rather than an insta snapshot, per docs/internal/test-strategy.md
//! Section 2.1 (the parse/extract layer is table-tested with rows of
//! `(input, expected fields + confidence)`; Section 2.2 reserves insta for
//! the F-201/F-202 classification goldens). Each row here is an entry keyed
//! by its `/`-joined relative path, so a manifest edit that shifts a book
//! cannot silently escape its expectation.
//!
//! The expectations deliberately LOCK the folder-first structural behavior
//! documented in `abo_core::parse::extract`, including the artifacts a pure
//! name pass cannot avoid without classification: a shelf folder named
//! `Genre - Children` (or `Genre - SciFI`) parses (F-301 pattern 2) as author
//! `Genre`, so a bare book under it inherits `Genre` at MEDIUM and a book with
//! its own author FLAGS a conflict against `Genre`. That is correct for this
//! layer (nothing is marked high that was not explicit; nothing is
//! fabricated); F-201 classification (P5) is what later distinguishes shelves
//! from real containers, using the ancestor provenance recorded on each
//! inherited field.

use std::collections::HashMap;
use std::path::Path;

use abo_core::fixtures::{generate, standard_library_manifest, GeneratedKind};
use abo_core::parse::extract::{
    extract, Confidence, EntryInput, Field, FieldName, FieldSource, MergedEntry, NodeKind,
};
use tempfile::TempDir;

/// `/`-join a relative path's components into a stable, platform-independent
/// key (the generated `relative_path` uses the host separator).
fn key(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// A single expected merged field: value, confidence tier, and source.
type ExpectStr = Option<(&'static str, Confidence, FieldSource)>;
type ExpectU16 = Option<(u16, Confidence, FieldSource)>;

/// One expected entry. Fields left `None` assert absence (fabricate-nothing).
struct Expect {
    key: &'static str,
    title: ExpectStr,
    author: ExpectStr,
    series: ExpectStr,
    series_index: ExpectStr,
    year: ExpectU16,
    subtitle: ExpectStr,
    /// Expected conflicts as `(field, own_value, ancestor_value)`, order-
    /// independent; `ancestor_id` correctness is checked separately.
    conflicts: &'static [(FieldName, &'static str, &'static str)],
}

fn as_u16(f: &Option<Field<u16>>) -> ExpectU16 {
    f.as_ref().map(|v| (v.value, v.confidence, v.source))
}

/// Compare an actual `Option<Field<String>>` against an expected tuple. The
/// actual value is borrowed as `&str` and compared against the table's
/// `&'static str`; `&str: PartialEq` makes the tuple comparison work without
/// any allocation or leaking.
fn eq_str(actual: &Option<Field<String>>, expected: ExpectStr, field: &str, key: &str) {
    let actual_tuple = actual
        .as_ref()
        .map(|v| (v.value.as_str(), v.confidence, v.source));
    assert_eq!(actual_tuple, expected, "field {field} mismatch for {key:?}");
}

fn check(entry: &MergedEntry, expect: &Expect, id_to_key: &HashMap<usize, String>) {
    let key = expect.key;
    eq_str(&entry.fields.title, expect.title, "title", key);
    eq_str(&entry.fields.author, expect.author, "author", key);
    eq_str(&entry.fields.series, expect.series, "series", key);
    eq_str(
        &entry.fields.series_index,
        expect.series_index,
        "series_index",
        key,
    );
    eq_str(&entry.fields.subtitle, expect.subtitle, "subtitle", key);
    let year_tuple = as_u16(&entry.fields.year);
    assert_eq!(year_tuple, expect.year, "field year mismatch for {key:?}");
    // Narrator is never present in the fixture library (no pattern reads it).
    assert!(
        entry.fields.narrator.is_none(),
        "unexpected narrator for {key:?}"
    );

    // Conflicts: compare the (field, own, ancestor) triples order-independently.
    let mut actual: Vec<(FieldName, String, String)> = entry
        .conflicts
        .iter()
        .map(|c| (c.field, c.own_value.clone(), c.ancestor_value.clone()))
        .collect();
    let mut expected: Vec<(FieldName, String, String)> = expect
        .conflicts
        .iter()
        .map(|(f, o, a)| (*f, o.to_string(), a.to_string()))
        .collect();
    actual.sort_by(|a, b| a.1.cmp(&b.1));
    expected.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(actual, expected, "conflicts mismatch for {key:?}");

    // Every conflict's ancestor_id must resolve to a real generated entry.
    for c in &entry.conflicts {
        assert!(
            id_to_key.contains_key(&c.ancestor_id),
            "conflict ancestor_id {} does not resolve to a generated entry (for {key:?})",
            c.ancestor_id
        );
    }
}

#[test]
fn confidence_golden_over_the_fixture_library() {
    use Confidence::{High, Low, Medium};
    use FieldSource::{Derived, Inherited, OwnName};

    let manifest = standard_library_manifest();
    let dir = TempDir::new().expect("tempdir");
    let lib = generate(&manifest, dir.path()).expect("generate fixture library");

    // Build the EntryInput tree: id = index into lib.entries, parent resolved
    // from the relative-path prefix.
    let key_to_id: HashMap<String, usize> = lib
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (key(&e.relative_path), i))
        .collect();
    let inputs: Vec<EntryInput> = lib
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let parent = e
                .relative_path
                .parent()
                .map(key)
                .filter(|s| !s.is_empty())
                .and_then(|pk| key_to_id.get(&pk).copied());
            EntryInput {
                id: i,
                parent,
                name: e
                    .relative_path
                    .file_name()
                    .expect("entry has a file name")
                    .to_string_lossy()
                    .into_owned(),
                kind: match e.kind {
                    GeneratedKind::File => NodeKind::File,
                    GeneratedKind::Dir => NodeKind::Folder,
                },
            }
        })
        .collect();

    let merged = extract(&inputs);
    let id_to_key: HashMap<usize, String> = lib
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| (i, key(&e.relative_path)))
        .collect();
    let by_key: HashMap<&str, &MergedEntry> = merged
        .iter()
        .map(|m| (id_to_key[&m.id].as_str(), m))
        .collect();

    let table: &[Expect] = &[
        // ---- high: explicit own-name title + author, no ancestor context ----
        Expect {
            key: "Atomic Habits by James Clear.m4b",
            title: Some(("Atomic Habits", High, OwnName)),
            author: Some(("James Clear", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- low: the AC-301.3 outlier, kept verbatim as a derived title;
        //      nothing fabricated ----
        Expect {
            key: "FREE SAMPLE_ The Martian by Andy Weir.m4b",
            title: Some(("FREE SAMPLE_ The Martian by Andy Weir", Low, Derived)),
            author: None,
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- the `SciFI` shelf: `SciFI` is a legitimate mixed-case title
        //      word, NOT a ripper tag, so F-302's evidence-based release-group
        //      stripper (D1 fix) leaves it in place. The shelf therefore
        //      parses via pattern 2 as author `Genre`, title `SciFI`, exactly
        //      like the other `Genre - X` shelves - books under it inherit
        //      `Genre` at MEDIUM and books with their own author flag a
        //      conflict against `Genre`. Locks the post-fix behavior. ----
        Expect {
            key: "Genre - SciFI",
            title: Some(("SciFI", High, OwnName)),
            author: Some(("Genre", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- a shelf whose second token is NOT a ripper-tag smell DOES
        //      parse as author=Genre (pattern 2) ----
        Expect {
            key: "Genre - Non-Fiction",
            title: Some(("Non-Fiction", High, OwnName)),
            author: Some(("Genre", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- pattern 4 folder under the SciFI shelf: full field set at
        //      high; its own author disagrees with the nearest author ancestor
        //      (the `Genre` shelf, now that SciFI parses as author=Genre), so
        //      that disagreement is FLAGGED (D1 fix changed this from no
        //      conflict) ----
        Expect {
            key: "Genre - SciFI/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]",
            title: Some(("The Fifth Season", High, OwnName)),
            author: Some(("N.K. Jemisin", High, OwnName)),
            series: Some(("The Broken Earth", High, OwnName)),
            series_index: Some(("1", High, OwnName)),
            year: Some((2016, High, OwnName)),
            subtitle: None,
            conflicts: &[(FieldName::Author, "N.K. Jemisin", "Genre")],
        },
        // ---- the canonical AC-303.2 case: a book file inherits series AND
        //      author from its series-container folder at MEDIUM, keeps its
        //      own title at HIGH, inherits neither index nor year ----
        Expect {
            key: "Genre - SciFI/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]/The Fifth Season.m4b",
            title: Some(("The Fifth Season", High, OwnName)),
            author: Some(("N.K. Jemisin", Medium, Inherited)),
            series: Some(("The Broken Earth", Medium, Inherited)),
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- pattern 6 folder deep under a Dresden series container:
        //      own index high, author inherited from the nearest ancestor
        //      that has one (Jim Butcher), medium ----
        Expect {
            key: "Genre - SciFI/Hugo Collection/0 Series Books/Jim Butcher - Dresden Files 22GB/08.0 - Proven Guilty [320k 16;15;24 2.19GB]",
            title: Some(("Proven Guilty", High, OwnName)),
            author: Some(("Jim Butcher", Medium, Inherited)),
            series: None,
            series_index: Some(("08.0", High, OwnName)),
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- deep nesting (depth >= 5): the file inherits author from the
        //      nearest author ancestor (Jim Butcher), keeps its own index ----
        Expect {
            key: "Genre - SciFI/Hugo Collection/0 Series Books/Jim Butcher - Dresden Files 22GB/14.0 - Cold Days [320k 18;48;03 2.52GB]/14.0 - Cold Days.m4b",
            title: Some(("Cold Days", High, OwnName)),
            author: Some(("Jim Butcher", Medium, Inherited)),
            series: None,
            series_index: Some(("14.0", High, OwnName)),
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- a file whose own author matches its nearest ancestor: no
        //      conflict (contrast the shelf conflicts below) ----
        Expect {
            key: "Genre - SciFI/Jim Butcher - Dresden Files 22GB/Jim Butcher - Dresden Files 22GB.m4b",
            title: Some(("Dresden Files", High, OwnName)),
            author: Some(("Jim Butcher", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- pattern 5 folder: title/author/year high; its own author
        //      disagrees with the nearest author ancestor (the `Genre` shelf,
        //      now that SciFI parses as author=Genre), so it is FLAGGED (D1
        //      fix changed this from no conflict) ----
        Expect {
            key: "Genre - SciFI/Top 100 Sci-Fi Books/1 - Ender's Game - Orson Scott Card - 1985",
            title: Some(("Ender's Game", High, OwnName)),
            author: Some(("Orson Scott Card", High, OwnName)),
            series: None,
            series_index: None,
            year: Some((1985, High, OwnName)),
            subtitle: None,
            conflicts: &[(FieldName::Author, "Orson Scott Card", "Genre")],
        },
        // ---- pattern 9 container: author + series + index high, no title;
        //      its own author disagrees with the nearest author ancestor (the
        //      `Genre` shelf, now that SciFI parses as author=Genre), so it is
        //      FLAGGED (D1 fix changed this from no conflict) ----
        Expect {
            key: "Genre - SciFI/Frank Herbert-Dune-#1-Chronicles[1-8}",
            title: None,
            author: Some(("Frank Herbert", High, OwnName)),
            series: Some(("Dune", High, OwnName)),
            series_index: Some(("1", High, OwnName)),
            year: None,
            subtitle: None,
            conflicts: &[(FieldName::Author, "Frank Herbert", "Genre")],
        },
        // ---- a pattern 9 member: own title high, own (mis-parsed) author
        //      high, series inherited medium, and the disagreement between
        //      the mis-parsed `Book 1` and the container's `Frank Herbert`
        //      is FLAGGED, not resolved. The load-bearing conflict case. ----
        Expect {
            key: "Genre - SciFI/Frank Herbert-Dune-#1-Chronicles[1-8}/Book 1 - Dune.m4b",
            title: Some(("Dune", High, OwnName)),
            author: Some(("Book 1", High, OwnName)),
            series: Some(("Dune", Medium, Inherited)),
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[(FieldName::Author, "Book 1", "Frank Herbert")],
        },
        // ---- pattern 3 folder under a real `author=Genre` shelf: title +
        //      subtitle + author high, author conflict flagged vs Genre ----
        Expect {
            key: "Genre - Non-Fiction/How to Think About AI - A Guide for the Perplexe by Richard Susskind",
            title: Some(("How to Think About AI", High, OwnName)),
            author: Some(("Richard Susskind", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: Some(("A Guide for the Perplexe", High, OwnName)),
            conflicts: &[(FieldName::Author, "Richard Susskind", "Genre")],
        },
        // ---- simple single-level inheritance: a bare book under an
        //      `author=Genre` shelf inherits author at medium ----
        Expect {
            key: "Genre - Non-Fiction/Rework",
            title: Some(("Rework", High, OwnName)),
            author: Some(("Genre", Medium, Inherited)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- an underscored name that degrades: `Disk_04`'s underscore
        //      trips pattern 1's leftover-noise guard, so the whole token is
        //      kept as a LOW derived title (a second low-tier example,
        //      distinct from the FREE SAMPLE outlier) ----
        Expect {
            key: "Genre - Non-Fiction/Verbal Advantage/Disk_04",
            title: Some(("Disk_04", Low, Derived)),
            author: Some(("Genre", Medium, Inherited)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- pattern 7 folder (underscored), conflict vs Genre shelf ----
        Expect {
            key: "Genre - Business/Chris_Dixon_-_Read_Write_Own",
            title: Some(("Read Write Own", High, OwnName)),
            author: Some(("Chris Dixon", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[(FieldName::Author, "Chris Dixon", "Genre")],
        },
        // ---- multi-book members: each sibling keeps its OWN distinct title
        //      (titles never inherit), all share the shelf-inherited author.
        //      Exactly what F-203 multi-book detection consumes ----
        Expect {
            key: "Genre - Children/Chronicles of Narnia/Prince Caspian.m4b",
            title: Some(("Prince Caspian", High, OwnName)),
            author: Some(("Genre", Medium, Inherited)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        Expect {
            key: "Genre - Children/Chronicles of Narnia/The Silver Chair.m4b",
            title: Some(("The Silver Chair", High, OwnName)),
            author: Some(("Genre", Medium, Inherited)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[],
        },
        // ---- the Harry Potter hard case (AC-203.2 territory): a
        //      `Title - Part N` file mis-parses author out of the title via
        //      pattern 2; the merge keeps the mis-parse explicit but FLAGS
        //      its disagreement with the shelf, never hiding it ----
        Expect {
            key: "Genre - Children/Harry Potter/Harry Potter and the Goblet of Fire - Part 1.mp3",
            title: Some(("Part 1", High, OwnName)),
            author: Some(("Harry Potter and the Goblet of Fire", High, OwnName)),
            series: None,
            series_index: None,
            year: None,
            subtitle: None,
            conflicts: &[(
                FieldName::Author,
                "Harry Potter and the Goblet of Fire",
                "Genre",
            )],
        },
    ];

    for expect in table {
        let entry = by_key
            .get(expect.key)
            .unwrap_or_else(|| panic!("fixture entry not found for key {:?}", expect.key));
        check(entry, expect, &id_to_key);
    }

    // FIX 2 (D3): each INHERITED field records the id of the ancestor it came
    // from, so P5 can suppress a shelf-derived inheritance (drop the spurious
    // `Genre` author) without re-walking the tree. Assert the recorded
    // ancestor resolves back to the expected genre shelf for the shelf-derived
    // inheritance rows, and that own-name fields carry no such provenance.
    let provenance_rows: &[(&str, &str)] = &[
        // (entry key, expected ancestor-shelf key it inherited author from)
        ("Genre - Non-Fiction/Rework", "Genre - Non-Fiction"),
        (
            "Genre - Children/Chronicles of Narnia/Prince Caspian.m4b",
            "Genre - Children",
        ),
        (
            "Genre - Children/Chronicles of Narnia/The Silver Chair.m4b",
            "Genre - Children",
        ),
    ];
    for (entry_key, shelf_key) in provenance_rows {
        let entry = by_key
            .get(entry_key)
            .unwrap_or_else(|| panic!("fixture entry not found for key {entry_key:?}"));
        let author = entry
            .fields
            .author
            .as_ref()
            .unwrap_or_else(|| panic!("expected an inherited author for {entry_key:?}"));
        assert_eq!(
            author.source,
            FieldSource::Inherited,
            "expected inherited author for {entry_key:?}"
        );
        let from = author
            .inherited_from
            .unwrap_or_else(|| panic!("inherited author for {entry_key:?} records no ancestor"));
        assert_eq!(
            id_to_key.get(&from).map(String::as_str),
            Some(*shelf_key),
            "inherited author for {entry_key:?} came from the wrong ancestor"
        );
        // The entry's own-name title must NOT carry ancestor provenance.
        assert_eq!(
            entry.fields.title.as_ref().and_then(|f| f.inherited_from),
            None,
            "own-name title for {entry_key:?} must not record an ancestor"
        );
    }
    // An own-name (high) author never records provenance.
    let own_author = by_key
        .get("Atomic Habits by James Clear.m4b")
        .expect("Atomic Habits entry present");
    assert_eq!(
        own_author
            .fields
            .author
            .as_ref()
            .and_then(|f| f.inherited_from),
        None,
        "own-name author must not record an ancestor"
    );

    // Global invariant across EVERY merged entry (not just the table rows):
    // fabricate-nothing means no field is ever present with an empty value,
    // and every conflict's ancestor id resolves to a real entry.
    for m in &merged {
        for (name, val) in [
            ("title", m.fields.title.as_ref().map(|f| &f.value)),
            ("author", m.fields.author.as_ref().map(|f| &f.value)),
            ("series", m.fields.series.as_ref().map(|f| &f.value)),
            (
                "series_index",
                m.fields.series_index.as_ref().map(|f| &f.value),
            ),
            ("subtitle", m.fields.subtitle.as_ref().map(|f| &f.value)),
            ("narrator", m.fields.narrator.as_ref().map(|f| &f.value)),
        ] {
            if let Some(v) = val {
                assert!(
                    !v.trim().is_empty(),
                    "empty {name} value present for {:?} (fabricate-nothing violation)",
                    id_to_key[&m.id]
                );
            }
        }
        for c in &m.conflicts {
            assert!(id_to_key.contains_key(&c.ancestor_id));
        }
    }
}
