//! Compositional table tests (AC-301.1, AC-301.3): every discovery example
//! from `docs/internal/product-requirements.md` F-301 / the feature-function
//! breakdown, and every F-301-family example in the P1 fixture manifest
//! (`crates/abo-core/src/fixtures/manifest.rs`), run end to end through
//! [`parse_preview`] (strip general noise, then match) with its expected
//! fields. This is deliberately at the RAW-string level (noise included),
//! not the pre-stripped level each matcher's own unit tests use, because
//! that is what AC-301.1's "real discovery examples" actually look like on
//! disk.

use super::*;
use crate::parse::matchers::MatcherId;

struct Case {
    raw: &'static str,
    expect: Expect,
}

enum Expect {
    Fields(MatcherId, ParsedFields),
    /// A degraded pattern-8 fallback (the AC-301.3 outlier's path): the
    /// whole string kept verbatim as the title, at the low-confidence
    /// score tier.
    DegradedFallback(&'static str),
}

fn fields(f: ParsedFields) -> ParsedFields {
    f
}

#[test]
fn discovery_and_fixture_examples_table() {
    let cases: Vec<Case> = vec![
        // ---- Pattern 1: Title by Author (loose root file) ----
        Case {
            raw: "Sapiens by Yuval Noah Harari.m4b",
            expect: Expect::Fields(
                MatcherId::Pattern1TitleByAuthorLooseRoot,
                fields(ParsedFields {
                    title: Some("Sapiens".into()),
                    author: Some("Yuval Noah Harari".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Atomic Habits by James Clear.m4b",
            expect: Expect::Fields(
                MatcherId::Pattern1TitleByAuthorLooseRoot,
                fields(ParsedFields {
                    title: Some("Atomic Habits".into()),
                    author: Some("James Clear".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "The Phoenix Project by Gene Kim.m4b",
            expect: Expect::Fields(
                MatcherId::Pattern1TitleByAuthorLooseRoot,
                fields(ParsedFields {
                    title: Some("The Phoenix Project".into()),
                    author: Some("Gene Kim".into()),
                    ..Default::default()
                }),
            ),
        },
        // AC-301.3: the 1-of-238 outlier. Not a wrong parse: the whole
        // string survives verbatim as the title, at the degraded-fallback
        // score, so it counts against coverage rather than passing clean.
        Case {
            raw: "FREE SAMPLE_ The Martian by Andy Weir.m4b",
            expect: Expect::DegradedFallback("FREE SAMPLE_ The Martian by Andy Weir"),
        },
        // ---- Pattern 2: Author - Title ----
        Case {
            raw: "Jim Butcher - Dresden Files 22GB",
            expect: Expect::Fields(
                MatcherId::Pattern2AuthorDashTitle,
                fields(ParsedFields {
                    author: Some("Jim Butcher".into()),
                    title: Some("Dresden Files".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Andy Weir - Project Hail Mary",
            expect: Expect::Fields(
                MatcherId::Pattern2AuthorDashTitle,
                fields(ParsedFields {
                    author: Some("Andy Weir".into()),
                    title: Some("Project Hail Mary".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Cixin Liu - The Three-Body Problem",
            expect: Expect::Fields(
                MatcherId::Pattern2AuthorDashTitle,
                fields(ParsedFields {
                    author: Some("Cixin Liu".into()),
                    title: Some("The Three-Body Problem".into()),
                    ..Default::default()
                }),
            ),
        },
        // Discovery lists this under the "Pattern 9" heading; this
        // implementation resolves it to Pattern 2. See
        // crate::parse::matchers module doc comment for why.
        Case {
            raw: "Roald Dahl - Charlie and the Chocolate Factory",
            expect: Expect::Fields(
                MatcherId::Pattern2AuthorDashTitle,
                fields(ParsedFields {
                    author: Some("Roald Dahl".into()),
                    title: Some("Charlie and the Chocolate Factory".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 3: Title by Author (folder variant) ----
        Case {
            raw: "How to Think About AI - A Guide for the Perplexe by Richard Susskind",
            expect: Expect::Fields(
                MatcherId::Pattern3TitleByAuthorFolder,
                fields(ParsedFields {
                    title: Some("How to Think About AI".into()),
                    subtitle: Some("A Guide for the Perplexe".into()),
                    author: Some("Richard Susskind".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Every Tool's a Hammer - Life Is What You Make It by Adam Savage",
            expect: Expect::Fields(
                MatcherId::Pattern3TitleByAuthorFolder,
                fields(ParsedFields {
                    title: Some("Every Tool's a Hammer".into()),
                    subtitle: Some("Life Is What You Make It".into()),
                    author: Some("Adam Savage".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 4: Year - Author - Title (Series #N) [noise] ----
        Case {
            raw: "2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]",
            expect: Expect::Fields(
                MatcherId::Pattern4YearAuthorTitleSeriesAward,
                fields(ParsedFields {
                    year: Some(2016),
                    author: Some("N.K. Jemisin".into()),
                    title: Some("The Fifth Season".into()),
                    series: Some("The Broken Earth".into()),
                    series_index: Some("1".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "2014 - Charles Stross - Neptune's Brood (Freyaverse #2) [64k 12;00;34 660MB]",
            expect: Expect::Fields(
                MatcherId::Pattern4YearAuthorTitleSeriesAward,
                fields(ParsedFields {
                    year: Some(2014),
                    author: Some("Charles Stross".into()),
                    title: Some("Neptune's Brood".into()),
                    series: Some("Freyaverse".into()),
                    series_index: Some("2".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 5: N - Title - Author - Year ----
        Case {
            raw: "1 - Ender's Game - Orson Scott Card - 1985",
            expect: Expect::Fields(
                MatcherId::Pattern5RankTitleAuthorYear,
                fields(ParsedFields {
                    title: Some("Ender's Game".into()),
                    author: Some("Orson Scott Card".into()),
                    year: Some(1985),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "16 - Brave New World - Aldous Huxley - 1932",
            expect: Expect::Fields(
                MatcherId::Pattern5RankTitleAuthorYear,
                fields(ParsedFields {
                    title: Some("Brave New World".into()),
                    author: Some("Aldous Huxley".into()),
                    year: Some(1932),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 6: NN.N - Title [noise] ----
        Case {
            raw: "14.0 - Cold Days [320k 18;48;03 2.52GB]",
            expect: Expect::Fields(
                MatcherId::Pattern6SeriesIndexTitle,
                fields(ParsedFields {
                    series_index: Some("14.0".into()),
                    title: Some("Cold Days".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "08.0 - Proven Guilty [320k 16;15;24 2.19GB]",
            expect: Expect::Fields(
                MatcherId::Pattern6SeriesIndexTitle,
                fields(ParsedFields {
                    series_index: Some("08.0".into()),
                    title: Some("Proven Guilty".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "(1) The Eye of the World [64k 29;57;14 826MB]",
            expect: Expect::Fields(
                MatcherId::Pattern6SeriesIndexTitle,
                fields(ParsedFields {
                    series_index: Some("1".into()),
                    title: Some("The Eye of the World".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 7: Author_Name_-_Title (underscored) ----
        Case {
            raw: "Chris_Dixon_-_Read_Write_Own",
            expect: Expect::Fields(
                MatcherId::Pattern7Underscored,
                fields(ParsedFields {
                    author: Some("Chris Dixon".into()),
                    title: Some("Read Write Own".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Michael_Easter_-_Scarcity_Brain",
            expect: Expect::Fields(
                MatcherId::Pattern7Underscored,
                fields(ParsedFields {
                    author: Some("Michael Easter".into()),
                    title: Some("Scarcity Brain".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 8: bare Title ----
        Case {
            raw: "Out of the Silent Planet",
            expect: Expect::Fields(
                MatcherId::Pattern8BareTitle,
                fields(ParsedFields {
                    title: Some("Out of the Silent Planet".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "The Soul of a New Machine",
            expect: Expect::Fields(
                MatcherId::Pattern8BareTitle,
                fields(ParsedFields {
                    title: Some("The Soul of a New Machine".into()),
                    ..Default::default()
                }),
            ),
        },
        Case {
            raw: "Rework",
            expect: Expect::Fields(
                MatcherId::Pattern8BareTitle,
                fields(ParsedFields {
                    title: Some("Rework".into()),
                    ..Default::default()
                }),
            ),
        },
        // ---- Pattern 9: Author - Series Name (irregular separators) ----
        Case {
            raw: "Frank Herbert-Dune-#1-Chronicles[1-8}",
            expect: Expect::Fields(
                MatcherId::Pattern9IrregularSeriesContainer,
                fields(ParsedFields {
                    author: Some("Frank Herbert".into()),
                    series: Some("Dune".into()),
                    series_index: Some("1".into()),
                    ..Default::default()
                }),
            ),
        },
    ];

    for case in cases {
        let outcome = parse_preview(case.raw);
        match (&outcome, &case.expect) {
            (MatchOutcome::Matched(m), Expect::Fields(expected_id, expected_fields)) => {
                assert_eq!(
                    m.id, *expected_id,
                    "wrong matcher id for {:?}: got {:?}, want {:?}",
                    case.raw, m.id, expected_id
                );
                assert_eq!(
                    m.fields, *expected_fields,
                    "wrong fields for {:?}",
                    case.raw
                );
            }
            (MatchOutcome::Matched(m), Expect::DegradedFallback(expected_title)) => {
                assert_eq!(m.id, MatcherId::Pattern8BareTitle, "for {:?}", case.raw);
                assert_eq!(
                    m.score,
                    matchers::DEGRADED_FALLBACK_SCORE,
                    "expected the degraded fallback tier for {:?}",
                    case.raw
                );
                assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
            }
            (other, _) => panic!("for {:?}: expected a match, got {other:?}", case.raw),
        }
    }
}

/// AC-301.4: the coverage metric itself, over a small synthetic set
/// (the real fixture-library measurement lives in
/// `crates/abo-core/tests/parser_coverage.rs`).
#[test]
fn coverage_counts_degraded_fallbacks_as_non_parsing() {
    let names = [
        "Sapiens by Yuval Noah Harari.m4b",
        "FREE SAMPLE_ The Martian by Andy Weir.m4b",
    ];
    let report = coverage(names);
    assert_eq!(report.total, 2);
    assert_eq!(report.clean, 1);
    assert_eq!(
        report.non_parsing,
        vec!["FREE SAMPLE_ The Martian by Andy Weir.m4b".to_string()]
    );
    assert_eq!(report.fraction(), 0.5);
}

#[test]
fn coverage_counts_a_genuine_bare_title_as_clean() {
    let names = ["Rework"];
    let report = coverage(names);
    assert_eq!(report.clean, 1);
    assert!(report.non_parsing.is_empty());
}

#[test]
fn coverage_of_an_empty_set_is_full_by_convention() {
    let report = coverage(Vec::new());
    assert_eq!(report.total, 0);
    assert_eq!(report.fraction(), 1.0);
}

#[test]
fn strip_known_extension_only_strips_recognized_audio_extensions() {
    assert_eq!(strip_known_extension("Sapiens.m4b"), "Sapiens");
    assert_eq!(strip_known_extension("Sapiens.mp3"), "Sapiens");
    // A folder name with a real internal period (initials) must not be
    // truncated: ".K" is not a recognized extension.
    assert_eq!(
        strip_known_extension("N.K. Jemisin - The Fifth Season"),
        "N.K. Jemisin - The Fifth Season"
    );
    assert_eq!(
        strip_known_extension("No Extension Here"),
        "No Extension Here"
    );
}
