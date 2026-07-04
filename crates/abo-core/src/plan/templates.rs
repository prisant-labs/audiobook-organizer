//! F-401 (naming templates): render a book's target library path from its
//! parsed fields and a chosen preset.
//!
//! Three shipped presets (spec F-401): [`Preset::AbsAuthorFirst`] (the
//! default per D-02), [`Preset::TitleFirst`], and [`Preset::HybridGenre`].
//! Each renders an ordered list of path COMPONENTS (not a joined string with
//! platform separators baked in - the caller joins them however its target
//! platform wants); every returned component has already been passed
//! through [`crate::parse::normalize::normalize_component`], so this module
//! both consumes parsed fields and PRODUCES filesystem-safe, normalized
//! output components in one step. Callers never need to re-normalize a
//! rendered segment.
//!
//! # Variables
//!
//! The full F-401 variable set: `{Author}`, `{Title}`, `{Series}`,
//! `{SeriesIndex}` (width-configurable via [`render_path`]'s
//! `series_index_width` parameter: `1` -> `01` -> `001`), `{Year}`,
//! `{Narrator}`, `{Subtitle}`. [`TemplateFields`] carries all seven (plus
//! `genre`, see below). **Narrator and Subtitle are accepted but not used by
//! any of the three shipped presets' documented shapes** (spec F-401's Key
//! Behaviors literally spell out each preset's pattern, and neither variable
//! appears in any of them): they exist on [`TemplateFields`] because they
//! are part of the variable set a future custom/ruleset-defined template
//! (F-801) can use, not because a v0.3.0 preset renders them. This has a
//! useful consequence for AC-2 (missing-field fallbacks never emit an empty
//! segment): since neither variable can appear in v0.3.0 output at all,
//! neither can ever contribute an empty segment either - a stronger
//! guarantee than "handled gracefully", proven directly by
//! [`tests::narrator_and_subtitle_never_affect_rendered_output`].
//!
//! # Genre is not a parsed field (yet)
//!
//! `crate::parse::ParsedFields` has no `genre` field: v1 extraction is
//! folder/file-NAME driven (FD-14) and no matcher in this codebase ever
//! recognizes a genre. [`Preset::HybridGenre`] is still one of the three
//! shipped presets (spec F-401), so [`TemplateFields::genre`] exists as a
//! plain `Option<&str>` the CALLER may set directly (not derived from
//! [`crate::parse::ParsedFields`] via [`TemplateFields::from_parsed`], which
//! always leaves it `None`). Until a later release adds real genre data,
//! [`Preset::HybridGenre`] therefore renders identically to
//! [`Preset::AbsAuthorFirst`] whenever the caller does not separately supply
//! a genre (the same "missing organizational-shelf segment collapses away"
//! rule that applies to a missing `{Series}` - see below - applies to a
//! missing genre too), which is the documented target shape this module
//! commits to for now. See [`tests::hybrid_genre_collapses_to_abs_author_first_shape_without_a_genre_value`].
//!
//! # Missing-field fallbacks (AC-2)
//!
//! Per spec F-401 edge cases, never emit an empty segment (`()`, a bare
//! trailing separator, or an empty folder):
//!   - **Year** is always an optional DECORATION inside another segment
//!     (` - {Year}` or ` ({Year})`): if absent, that decoration is omitted
//!     entirely, never leaving behind stray punctuation.
//!   - **Series** (and, for [`Preset::HybridGenre`], **Genre**) are
//!     organizational-shelf segments: if absent, the WHOLE segment
//!     collapses away rather than emitting an empty folder level (spec:
//!     "missing series collapses the series segment rather than emitting an
//!     empty folder").
//!   - **Author** and **Title** are load-bearing identifiers with no
//!     shipped preset that renders sensibly without them. Their absence is
//!     therefore not a silent collapse: [`render_path`] returns
//!     [`RenderOutcome::ManualReview`] rather than a guessed or degraded
//!     path (the same posture the spec mandates for the next bullet).
//!   - **SeriesIndex** absent on a book that DOES have a series is the
//!     spec's one explicit manual-review trigger ("`{SeriesIndex}` absent
//!     on a series book routes the book to manual-review rather than
//!     guessing an index"): [`render_path`] returns
//!     [`RenderOutcome::ManualReview`] rather than fabricating an index.
//!     Note this is distinct from Series being absent altogether, which
//!     collapses gracefully (previous bullet) rather than escalating.
//!
//! # No award/genre folder under the default preset (AC-3, D-02)
//!
//! [`TemplateFields`] has no `award` field at all: nothing in
//! `crate::parse` extracts an award/rank marker into a structured field (the
//! `^` caret in `pattern4`'s "year-author-title-award" shape is consumed by
//! that matcher's regex and never copied into
//! [`crate::parse::ParsedFields`] - see `parse::matchers::pattern4`'s
//! doc comment). There is therefore no data path by which an award marker
//! could reach a rendered segment under ANY preset, and
//! [`Preset::AbsAuthorFirst`] additionally never even looks at `genre`. See
//! [`tests::award_marked_fixture_produces_no_award_folder`].

use crate::parse::normalize::normalize_component;
use crate::parse::ParsedFields;

/// The default [`SeriesIndex`](TemplateFields::series_index) zero-pad width
/// when a caller does not choose one explicitly. `1` (no zero-padding) is the
/// width the ratified GUI prototypes render: both `_local/gui/06-dryrun-report.html`
/// and `_local/gui/05-review.html` show the `after` book folders as `Book 1`,
/// `Book 5`, `Book 14`, `Book 17` (never `Book 01`/`Book 05`), so a width of
/// `1` is the shipped default that matches what the prototypes committed to.
/// F-401 ships `1`/`2`/`3` as configurable widths (spec: "width-configurable
/// 1/01/001"); this is simply which one applies when nothing overrides it, and
/// the F-801 default ruleset ([`crate::ruleset::default_ruleset`]) carries the
/// same value so the two never drift.
pub const DEFAULT_SERIES_INDEX_WIDTH: usize = 1;

/// One of the three shipped F-401 presets. [`Preset::AbsAuthorFirst`] is the
/// default (D-02, closing the strategy brief's open question 1).
///
/// Serializes as its spec-native kebab-case name (`abs-author-first`,
/// `title-first`, `hybrid-genre`) so an F-801 ruleset body (see
/// [`crate::ruleset`]) reads exactly as the spec's F-401 preset strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// `{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/` (default).
    #[default]
    AbsAuthorFirst,
    /// `{Title} - {Author} ({Year})/` - a single flat per-book folder,
    /// independent of series membership.
    TitleFirst,
    /// Genre kept as a top-level shelf, ABS-native below: the same shape as
    /// [`Preset::AbsAuthorFirst`] with an optional leading `{Genre}/`
    /// segment (see the module doc's "Genre is not a parsed field" section
    /// for what happens when no genre value is supplied).
    HybridGenre,
}

/// The template variables one book can supply, borrowed rather than owned
/// (a render call never needs to allocate to describe its input). Mirrors
/// `crate::parse::ParsedFields` field-for-field, plus `genre` (see the
/// module doc's "Genre is not a parsed field" section for why that one is
/// not part of [`ParsedFields`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TemplateFields<'a> {
    pub author: Option<&'a str>,
    pub title: Option<&'a str>,
    pub series: Option<&'a str>,
    pub series_index: Option<&'a str>,
    pub year: Option<u16>,
    pub narrator: Option<&'a str>,
    pub subtitle: Option<&'a str>,
    pub genre: Option<&'a str>,
}

impl<'a> TemplateFields<'a> {
    /// Borrow a [`TemplateFields`] view of an already-parsed
    /// [`ParsedFields`]. `genre` is always `None` (see the module doc); a
    /// caller with an out-of-band genre value sets it afterward.
    pub fn from_parsed(fields: &'a ParsedFields) -> Self {
        TemplateFields {
            author: fields.author.as_deref(),
            title: fields.title.as_deref(),
            series: fields.series.as_deref(),
            series_index: fields.series_index.as_deref(),
            year: fields.year,
            narrator: fields.narrator.as_deref(),
            subtitle: fields.subtitle.as_deref(),
            genre: None,
        }
    }
}

/// Why [`render_path`] could not produce a target path at all. The plan
/// builder (F-403, v0.3.0 Phase 4) maps every variant to a
/// `no-op(manual-review)` operation rather than guessing (spec F-401/F-403
/// edge cases).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualReviewReason {
    /// No usable author value; no shipped preset positions a book without
    /// one.
    MissingAuthor,
    /// No usable title value; no shipped preset positions a book without
    /// one.
    MissingTitle,
    /// The book has a series but no series index (spec's explicit
    /// "never guess an index" rule).
    MissingSeriesIndex,
}

/// The result of rendering one book's target path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderOutcome {
    /// The ordered, normalized path components (the caller joins them with
    /// its platform's separator; the last element is the book's own leaf
    /// folder).
    Rendered(Vec<String>),
    /// Rendering is not possible without guessing; route to manual review.
    ManualReview(ManualReviewReason),
}

impl RenderOutcome {
    /// Convenience for tests/callers that expect a successful render:
    /// panics with the reason if rendering instead needed manual review.
    #[cfg(test)]
    fn expect_rendered(self) -> Vec<String> {
        match self {
            RenderOutcome::Rendered(segments) => segments,
            RenderOutcome::ManualReview(reason) => {
                panic!("expected a rendered path, got ManualReview({reason:?})")
            }
        }
    }
}

/// Render `fields` under `preset` into an ordered list of normalized path
/// components, or a [`ManualReviewReason`] when a required value is
/// missing. `series_index_width` zero-pads [`TemplateFields::series_index`]
/// to at least that many digits (a width of `0` is treated as `1`, the same
/// "never a no-op cap" guard `parse::normalize` uses for its own
/// zero-length option). See the module doc for the exact fallback rules.
pub fn render_path(
    preset: Preset,
    fields: &TemplateFields<'_>,
    series_index_width: usize,
) -> RenderOutcome {
    let width = series_index_width.max(1);

    let Some(author) = non_empty(fields.author) else {
        return RenderOutcome::ManualReview(ManualReviewReason::MissingAuthor);
    };
    let Some(title) = non_empty(fields.title) else {
        return RenderOutcome::ManualReview(ManualReviewReason::MissingTitle);
    };

    let mut segments: Vec<String> = Vec::new();

    if matches!(preset, Preset::HybridGenre) {
        if let Some(genre) = non_empty(fields.genre) {
            segments.push(normalize(genre));
        }
    }

    match preset {
        Preset::AbsAuthorFirst | Preset::HybridGenre => {
            segments.push(normalize(author));
            match (non_empty(fields.series), non_empty(fields.series_index)) {
                (Some(series), Some(index)) => {
                    segments.push(normalize(series));
                    let padded = format_series_index(index, width);
                    let book = match fields.year {
                        Some(year) => format!("Book {padded} - {year} - {title}"),
                        None => format!("Book {padded} - {title}"),
                    };
                    segments.push(normalize(&book));
                }
                (Some(_series_with_no_index), None) => {
                    return RenderOutcome::ManualReview(ManualReviewReason::MissingSeriesIndex);
                }
                (None, _) => {
                    // No series at all: the series/"Book N" structure
                    // collapses away entirely (not just the series
                    // segment), leaving a plain per-book folder under the
                    // author, matching the spec's "missing series collapses
                    // the series segment rather than emitting an empty
                    // folder" - here that collapse also removes the
                    // series-relative "Book N -" prefix, since there is no
                    // series for a book number to be relative to.
                    let book = match fields.year {
                        Some(year) => format!("{title} ({year})"),
                        None => title.to_string(),
                    };
                    segments.push(normalize(&book));
                }
            }
        }
        Preset::TitleFirst => {
            let book = match fields.year {
                Some(year) => format!("{title} - {author} ({year})"),
                None => format!("{title} - {author}"),
            };
            segments.push(normalize(&book));
        }
    }

    RenderOutcome::Rendered(segments)
}

/// Trim `value` and treat an all-whitespace or empty result as absent, so a
/// caller-supplied `Some("")` or `Some("   ")` is handled exactly like
/// `None` rather than producing a blank path component.
fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Normalize one composed path component via
/// [`crate::parse::normalize::normalize_component`], discarding the flag
/// list (a plan-builder-level explainability concern, not this module's).
fn normalize(component: &str) -> String {
    normalize_component(component).component
}

/// Zero-pad the leading integer portion of a series index to at least
/// `width` digits, preserving any decimal suffix exactly
/// (`crate::parse::ParsedFields::series_index` deliberately keeps
/// `"14.0"`/`"08.0"` as their literal captured text - see that struct's doc
/// comment - so this only pads, never truncates or reinterprets, the
/// integer part). An index whose leading portion is not a plain
/// non-negative integer (a lettered sub-index like `"1a"`, or any other
/// shape a future matcher might capture) is returned unchanged rather than
/// guessed at.
fn format_series_index(raw: &str, width: usize) -> String {
    let (int_part, rest) = match raw.split_once('.') {
        Some((int_part, frac)) => (int_part, format!(".{frac}")),
        None => (raw, String::new()),
    };
    match int_part.parse::<u64>() {
        Ok(n) => format!("{n:0width$}{rest}"),
        Err(_) => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A convenience builder so table-driven tests can name only the fields
    /// they care about.
    fn fields<'a>(
        author: Option<&'a str>,
        title: Option<&'a str>,
        series: Option<&'a str>,
        series_index: Option<&'a str>,
        year: Option<u16>,
    ) -> TemplateFields<'a> {
        TemplateFields {
            author,
            title,
            series,
            series_index,
            year,
            narrator: None,
            subtitle: None,
            genre: None,
        }
    }

    // ---- AC-1: the discovery example set renders to each preset's -------
    // ---- documented target shape; abs-author-first is the default. ------

    /// The discovery example set (pattern-4's real fixtures: an award-marker
    /// series book, a `#N` series book, and a standalone book with no
    /// series at all) rendered under `abs-author-first`, the documented
    /// `{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/` shape.
    #[test]
    fn abs_author_first_renders_the_discovery_examples_to_the_documented_shape() {
        let cases: &[(TemplateFields, &[&str])] = &[
            (
                fields(
                    Some("N.K. Jemisin"),
                    Some("The Fifth Season"),
                    Some("The Broken Earth"),
                    Some("1"),
                    Some(2016),
                ),
                &[
                    "N.K. Jemisin",
                    "The Broken Earth",
                    "Book 1 - 2016 - The Fifth Season",
                ],
            ),
            (
                fields(
                    Some("Charles Stross"),
                    Some("Neptune's Brood"),
                    Some("Freyaverse"),
                    Some("2"),
                    Some(2014),
                ),
                &[
                    "Charles Stross",
                    "Freyaverse",
                    "Book 2 - 2014 - Neptune's Brood",
                ],
            ),
            (
                // "2014 - Ann Leckie - Ancillary Justice": no series at all.
                fields(
                    Some("Ann Leckie"),
                    Some("Ancillary Justice"),
                    None,
                    None,
                    Some(2014),
                ),
                &["Ann Leckie", "Ancillary Justice (2014)"],
            ),
        ];

        for (input, expected) in cases {
            let outcome = render_path(Preset::AbsAuthorFirst, input, DEFAULT_SERIES_INDEX_WIDTH);
            assert_eq!(
                outcome.expect_rendered(),
                expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                "input: {input:?}"
            );
        }
    }

    /// `abs-author-first` is the default preset used when none is named
    /// (AC-1): [`Preset::default()`] is [`Preset::AbsAuthorFirst`].
    #[test]
    fn abs_author_first_is_the_default_preset() {
        assert_eq!(Preset::default(), Preset::AbsAuthorFirst);
    }

    /// `title-first` renders the discovery examples to the documented
    /// `{Title} - {Author} ({Year})/` shape, flat regardless of series
    /// membership.
    #[test]
    fn title_first_renders_the_discovery_examples_to_the_documented_shape() {
        let cases: &[(TemplateFields, &[&str])] = &[
            (
                fields(
                    Some("N.K. Jemisin"),
                    Some("The Fifth Season"),
                    Some("The Broken Earth"),
                    Some("1"),
                    Some(2016),
                ),
                &["The Fifth Season - N.K. Jemisin (2016)"],
            ),
            (
                fields(
                    Some("Ann Leckie"),
                    Some("Ancillary Justice"),
                    None,
                    None,
                    Some(2014),
                ),
                &["Ancillary Justice - Ann Leckie (2014)"],
            ),
        ];

        for (input, expected) in cases {
            let outcome = render_path(Preset::TitleFirst, input, DEFAULT_SERIES_INDEX_WIDTH);
            assert_eq!(
                outcome.expect_rendered(),
                expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                "input: {input:?}"
            );
        }
    }

    /// `hybrid-genre`, given an explicit genre value, prepends a `{Genre}`
    /// shelf ahead of the otherwise-identical `abs-author-first` shape.
    #[test]
    fn hybrid_genre_prepends_a_genre_shelf_when_a_genre_value_is_supplied() {
        let mut input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );
        input.genre = Some("Science Fiction");

        let outcome = render_path(Preset::HybridGenre, &input, DEFAULT_SERIES_INDEX_WIDTH);
        assert_eq!(
            outcome.expect_rendered(),
            vec![
                "Science Fiction".to_string(),
                "N.K. Jemisin".to_string(),
                "The Broken Earth".to_string(),
                "Book 1 - 2016 - The Fifth Season".to_string(),
            ]
        );
    }

    /// Without a genre value (the only case reachable today: nothing in
    /// `crate::parse` produces one), `hybrid-genre` collapses to exactly the
    /// `abs-author-first` shape - the documented target shape for this
    /// preset until a later release adds real genre data (module doc,
    /// "Genre is not a parsed field").
    #[test]
    fn hybrid_genre_collapses_to_abs_author_first_shape_without_a_genre_value() {
        let input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );

        let hybrid =
            render_path(Preset::HybridGenre, &input, DEFAULT_SERIES_INDEX_WIDTH).expect_rendered();
        let abs_author_first =
            render_path(Preset::AbsAuthorFirst, &input, DEFAULT_SERIES_INDEX_WIDTH)
                .expect_rendered();
        assert_eq!(hybrid, abs_author_first);
    }

    // ---- AC-2: missing-field fallbacks never emit an empty segment. -----

    /// Each of the seven F-401 variables, absent one at a time (others
    /// held present), never produces an empty segment, a bare `()`, or a
    /// trailing/leading separator artifact inside any segment.
    #[test]
    fn missing_field_fallbacks_never_emit_an_empty_segment_table() {
        let base = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );

        // (label, mutated fields, expected outcome under abs-author-first)
        let mut missing_year = base;
        missing_year.year = None;
        assert_no_empty_segment_and_matches(
            "Year absent: decoration omitted, no bare parens/dashes",
            Preset::AbsAuthorFirst,
            &missing_year,
            &[
                "N.K. Jemisin",
                "The Broken Earth",
                "Book 1 - The Fifth Season",
            ],
        );

        let mut missing_series = base;
        missing_series.series = None;
        missing_series.series_index = None;
        assert_no_empty_segment_and_matches(
            "Series (and index) absent: series/Book-N structure collapses away",
            Preset::AbsAuthorFirst,
            &missing_series,
            &["N.K. Jemisin", "The Fifth Season (2016)"],
        );

        let mut missing_narrator = base;
        missing_narrator.narrator = None;
        assert_no_empty_segment_and_matches(
            "Narrator absent: no shipped preset renders it, so no effect",
            Preset::AbsAuthorFirst,
            &missing_narrator,
            &[
                "N.K. Jemisin",
                "The Broken Earth",
                "Book 1 - 2016 - The Fifth Season",
            ],
        );

        let mut missing_subtitle = base;
        missing_subtitle.subtitle = None;
        assert_no_empty_segment_and_matches(
            "Subtitle absent: no shipped preset renders it, so no effect",
            Preset::AbsAuthorFirst,
            &missing_subtitle,
            &[
                "N.K. Jemisin",
                "The Broken Earth",
                "Book 1 - 2016 - The Fifth Season",
            ],
        );

        // Author and Title absent are covered by the dedicated
        // manual-review tests below (they are not "silent fallback" cases:
        // rendering refuses rather than degrading).
    }

    fn assert_no_empty_segment_and_matches(
        label: &str,
        preset: Preset,
        input: &TemplateFields<'_>,
        expected: &[&str],
    ) {
        let segments = render_path(preset, input, DEFAULT_SERIES_INDEX_WIDTH).expect_rendered();
        for segment in &segments {
            assert!(
                !segment.is_empty(),
                "{label}: empty segment in {segments:?}"
            );
            assert!(
                !segment.contains("()"),
                "{label}: bare parens in {segments:?}"
            );
            assert!(
                !segment.trim_start().starts_with('-') && !segment.trim_end().ends_with('-'),
                "{label}: dangling dash in {segments:?}"
            );
        }
        assert_eq!(
            segments,
            expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "{label}"
        );
    }

    /// Narrator and Subtitle presence/absence never changes rendered output
    /// under any shipped preset (module doc: neither variable is used by
    /// any v0.3.0 preset), which is itself the strongest form of "never
    /// emits an empty segment" for those two variables.
    #[test]
    fn narrator_and_subtitle_never_affect_rendered_output() {
        let mut without = fields(
            Some("Ann Leckie"),
            Some("Ancillary Justice"),
            None,
            None,
            Some(2014),
        );
        let mut with = without;
        with.narrator = Some("Adjoa Andoh");
        with.subtitle = Some("Imperial Radch, Book 1");

        for preset in [
            Preset::AbsAuthorFirst,
            Preset::TitleFirst,
            Preset::HybridGenre,
        ] {
            let a = render_path(preset, &without, DEFAULT_SERIES_INDEX_WIDTH).expect_rendered();
            let b = render_path(preset, &with, DEFAULT_SERIES_INDEX_WIDTH).expect_rendered();
            assert_eq!(
                a, b,
                "preset {preset:?}: narrator/subtitle must not affect output"
            );
        }
        // Silence an "unused assignment" style lint concern: `without` is
        // read above via both `a` computations; nothing further needed.
        let _ = &mut without;
    }

    /// Missing Author routes to manual review rather than degrading to a
    /// guessed or partial path, under every shipped preset.
    #[test]
    fn missing_author_routes_to_manual_review() {
        let input = fields(
            None,
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );
        for preset in [
            Preset::AbsAuthorFirst,
            Preset::TitleFirst,
            Preset::HybridGenre,
        ] {
            assert_eq!(
                render_path(preset, &input, DEFAULT_SERIES_INDEX_WIDTH),
                RenderOutcome::ManualReview(ManualReviewReason::MissingAuthor),
                "preset {preset:?}"
            );
        }
    }

    /// Missing Title routes to manual review rather than degrading to a
    /// guessed or partial path, under every shipped preset.
    #[test]
    fn missing_title_routes_to_manual_review() {
        let input = fields(
            Some("N.K. Jemisin"),
            None,
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );
        for preset in [
            Preset::AbsAuthorFirst,
            Preset::TitleFirst,
            Preset::HybridGenre,
        ] {
            assert_eq!(
                render_path(preset, &input, DEFAULT_SERIES_INDEX_WIDTH),
                RenderOutcome::ManualReview(ManualReviewReason::MissingTitle),
                "preset {preset:?}"
            );
        }
    }

    /// A series book (Series present) with no SeriesIndex routes to manual
    /// review rather than guessing an index (spec's explicit rule), under
    /// both presets that use series structure.
    #[test]
    fn series_present_without_index_routes_to_manual_review() {
        let input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            None,
            Some(2016),
        );
        for preset in [Preset::AbsAuthorFirst, Preset::HybridGenre] {
            assert_eq!(
                render_path(preset, &input, DEFAULT_SERIES_INDEX_WIDTH),
                RenderOutcome::ManualReview(ManualReviewReason::MissingSeriesIndex),
                "preset {preset:?}"
            );
        }
    }

    /// Blank/whitespace-only strings are treated exactly like an absent
    /// field (never rendered as a blank segment), for every optional field.
    #[test]
    fn blank_strings_are_treated_as_absent() {
        let input = fields(
            Some("Ann Leckie"),
            Some("Ancillary Justice"),
            Some("   "),
            Some(""),
            Some(2014),
        );
        let segments = render_path(Preset::AbsAuthorFirst, &input, DEFAULT_SERIES_INDEX_WIDTH)
            .expect_rendered();
        assert_eq!(
            segments,
            vec![
                "Ann Leckie".to_string(),
                "Ancillary Justice (2014)".to_string()
            ]
        );
    }

    // ---- AC-3: no award/genre folder segment under abs-author-first. ----

    /// An award-marked fixture (`^`, pattern 4's Hugo-winner shape) renders
    /// under `abs-author-first` with no award folder and no `^` anywhere in
    /// the output: [`TemplateFields`] has no award field at all, so there is
    /// no data path for the marker to reach a segment (module doc).
    #[test]
    fn award_marked_fixture_produces_no_award_folder() {
        // The exact fields `parse::matchers::pattern4::try_match` extracts
        // from "2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth
        // Book 1)": the `^` is consumed by the matcher's regex and never
        // copied into a field (see that matcher's doc comment).
        let input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );

        let segments = render_path(Preset::AbsAuthorFirst, &input, DEFAULT_SERIES_INDEX_WIDTH)
            .expect_rendered();
        assert_eq!(segments.len(), 3, "no extra award-folder segment");
        for segment in &segments {
            assert!(
                !segment.contains('^'),
                "award marker leaked into {segment:?}"
            );
        }
    }

    /// Genre never appears anywhere in `abs-author-first` output, even when
    /// a caller supplies one directly (D-02: genre is never a folder
    /// segment under the default preset - only `hybrid-genre` may use it).
    #[test]
    fn genre_never_appears_under_abs_author_first_even_if_supplied() {
        let mut input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );
        input.genre = Some("Science Fiction");

        let segments = render_path(Preset::AbsAuthorFirst, &input, DEFAULT_SERIES_INDEX_WIDTH)
            .expect_rendered();
        assert!(
            !segments.iter().any(|s| s.contains("Science Fiction")),
            "genre must never appear under abs-author-first: {segments:?}"
        );
        assert_eq!(segments.len(), 3);
    }

    // ---- SeriesIndex width variants. --------------------------------

    /// `{SeriesIndex}` zero-pads to the configured width: `1` -> `01` ->
    /// `001`, matching the spec's literal width examples.
    #[test]
    fn series_index_width_variants_table() {
        let cases: &[(usize, &str)] = &[(1, "1"), (2, "01"), (3, "001")];
        for (width, expected_index) in cases {
            let input = fields(
                Some("N.K. Jemisin"),
                Some("The Fifth Season"),
                Some("The Broken Earth"),
                Some("1"),
                Some(2016),
            );
            let segments = render_path(Preset::AbsAuthorFirst, &input, *width).expect_rendered();
            let expected_book_segment = format!("Book {expected_index} - 2016 - The Fifth Season");
            assert_eq!(segments[2], expected_book_segment, "width {width}");
        }
    }

    /// A width of `0` is treated as `1` (never a no-op/empty-index cap),
    /// mirroring `parse::normalize`'s own zero-length option guard.
    #[test]
    fn series_index_width_zero_is_treated_as_one() {
        let input = fields(
            Some("N.K. Jemisin"),
            Some("The Fifth Season"),
            Some("The Broken Earth"),
            Some("1"),
            Some(2016),
        );
        let segments = render_path(Preset::AbsAuthorFirst, &input, 0).expect_rendered();
        assert_eq!(segments[2], "Book 1 - 2016 - The Fifth Season");
    }

    /// A decimal series index (`"14.0"`, `"08.0"` - real captured shapes per
    /// `crate::parse::ParsedFields::series_index`'s doc comment) is padded
    /// on its integer part only; the decimal suffix is preserved exactly,
    /// never truncated or reinterpreted.
    #[test]
    fn series_index_with_decimal_suffix_pads_integer_part_only() {
        assert_eq!(format_series_index("14.0", 2), "14.0");
        assert_eq!(format_series_index("8.0", 2), "08.0");
        assert_eq!(format_series_index("8.0", 3), "008.0");
        assert_eq!(format_series_index("14.0", 1), "14.0");
    }

    /// A series index that is not a plain non-negative integer (a lettered
    /// sub-index) is returned unchanged rather than guessed at.
    #[test]
    fn non_numeric_series_index_is_returned_unchanged() {
        assert_eq!(format_series_index("1a", 2), "1a");
    }

    // ---- from_parsed ---------------------------------------------------

    /// [`TemplateFields::from_parsed`] mirrors every `ParsedFields` field
    /// and always leaves `genre` `None`.
    #[test]
    fn from_parsed_mirrors_parsed_fields_and_leaves_genre_none() {
        let parsed = ParsedFields {
            title: Some("The Fifth Season".to_string()),
            author: Some("N.K. Jemisin".to_string()),
            series: Some("The Broken Earth".to_string()),
            series_index: Some("1".to_string()),
            year: Some(2016),
            narrator: Some("Robin Miles".to_string()),
            subtitle: None,
        };
        let view = TemplateFields::from_parsed(&parsed);
        assert_eq!(view.title, Some("The Fifth Season"));
        assert_eq!(view.author, Some("N.K. Jemisin"));
        assert_eq!(view.series, Some("The Broken Earth"));
        assert_eq!(view.series_index, Some("1"));
        assert_eq!(view.year, Some(2016));
        assert_eq!(view.narrator, Some("Robin Miles"));
        assert_eq!(view.subtitle, None);
        assert_eq!(view.genre, None);
    }
}
