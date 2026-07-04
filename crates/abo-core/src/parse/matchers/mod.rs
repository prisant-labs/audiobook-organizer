//! F-301 (pattern matcher set): nine pure-function matchers, one per
//! discovery naming pattern, run in specificity order (AC-301.2). Each
//! matcher inspects a single raw folder or file-stem name and, on success,
//! returns the fields it could read plus a match score;
//! [`resolve`] (used by [`match_name`]) picks the highest-scoring candidate,
//! or reports a tie as [`MatchOutcome::Ambiguous`].
//!
//! Specificity is expressed as a fixed per-pattern base score (higher =
//! more structurally distinctive), not as "first matcher to claim it wins":
//! every enabled matcher runs on every input, and [`resolve`] compares
//! scores across whichever candidates actually matched. This means two
//! DIFFERENT patterns can both match the same string (e.g. `14.0 - Cold
//! Days` loosely fits pattern 2's generic `X - Y` shape as well as pattern
//! 6's index-title shape) without creating a false ambiguity: pattern 6's
//! higher score wins outright, exactly matching the spec's "resolves to the
//! more specific one" language. A genuine tie (two candidates at the SAME
//! top score) is the only case [`MatchOutcome::Ambiguous`] is returned for.
//!
//! Pattern 8 (bare title) is the universal last-resort fallback: every
//! input that no other pattern claims still gets a Pattern 8 match, so
//! `match_name` only returns [`MatchOutcome::NoMatch`] for empty input. It
//! scores at one of two tiers ([`BARE_TITLE_SCORE`] for a genuine bare
//! title, [`DEGRADED_FALLBACK_SCORE`] for a name that still carries a
//! marker another pattern's content guard rejected) rather than a single
//! fixed score; see `pattern8`'s doc comment for why, and
//! `crate::parse::coverage` for how the distinction feeds the parser-
//! coverage metric.
//!
//! Design note on Pattern 9 (`Author - Series Name` containers): the
//! discovery doc lists two examples under this heading,
//! `Frank Herbert-Dune-#1-Chronicles[1-8}` (irregular, no-space hyphens) and
//! `Roald Dahl - Charlie and the Chocolate Factory` (canonical ` - `
//! separator). The second is STRUCTURALLY IDENTICAL to a Pattern 2
//! `Author - Title` example (`Andy Weir - Project Hail Mary`); a pure
//! string matcher cannot tell "this is a series container with one member
//! so far" from "this is a book" without folder-content context, which is
//! F-201/F-203 classification territory, not this module's. This
//! implementation therefore has Pattern 9 claim ONLY the irregular-
//! separator shape (its genuinely distinguishing signal) and lets the
//! canonical-separator example resolve as Pattern 2. This is a documented
//! interpretation of ambiguous discovery data, not a spec requirement;
//! P4b/P5 should confirm or override it once real folder contents are
//! available. See `crate::parse::tests` for the table test recording this
//! choice explicitly.

mod pattern1;
mod pattern2;
mod pattern3;
mod pattern4;
mod pattern5;
mod pattern6;
mod pattern7;
mod pattern8;
mod pattern9;

pub(crate) use pattern8::DEGRADED_FALLBACK_SCORE;

use crate::parse::ParsedFields;

/// Stable matcher identity. `as_str` gives the stable `pattern-N` id form
/// the brief calls for; `handle` gives the short human-readable pattern
/// shape, always used alongside the id in reports (never a bare id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatcherId {
    Pattern1TitleByAuthorLooseRoot,
    Pattern2AuthorDashTitle,
    Pattern3TitleByAuthorFolder,
    Pattern4YearAuthorTitleSeriesAward,
    Pattern5RankTitleAuthorYear,
    Pattern6SeriesIndexTitle,
    Pattern7Underscored,
    Pattern8BareTitle,
    Pattern9IrregularSeriesContainer,
}

impl MatcherId {
    pub fn as_str(self) -> &'static str {
        match self {
            MatcherId::Pattern1TitleByAuthorLooseRoot => "pattern-1",
            MatcherId::Pattern2AuthorDashTitle => "pattern-2",
            MatcherId::Pattern3TitleByAuthorFolder => "pattern-3",
            MatcherId::Pattern4YearAuthorTitleSeriesAward => "pattern-4",
            MatcherId::Pattern5RankTitleAuthorYear => "pattern-5",
            MatcherId::Pattern6SeriesIndexTitle => "pattern-6",
            MatcherId::Pattern7Underscored => "pattern-7",
            MatcherId::Pattern8BareTitle => "pattern-8",
            MatcherId::Pattern9IrregularSeriesContainer => "pattern-9",
        }
    }

    /// Short human-readable pattern shape, paired with `as_str`'s bare id
    /// everywhere the id is surfaced (never a bare `pattern-N` alone).
    pub fn handle(self) -> &'static str {
        match self {
            MatcherId::Pattern1TitleByAuthorLooseRoot => "Title by Author (loose root file)",
            MatcherId::Pattern2AuthorDashTitle => "Author - Title",
            MatcherId::Pattern3TitleByAuthorFolder => "Title by Author (folder variant)",
            MatcherId::Pattern4YearAuthorTitleSeriesAward => {
                "Year - Author - Title (Series #N) [award]"
            }
            MatcherId::Pattern5RankTitleAuthorYear => "N - Title - Author - Year",
            MatcherId::Pattern6SeriesIndexTitle => "NN.N - Title (series entry)",
            MatcherId::Pattern7Underscored => "Author_Name_-_Title (underscored)",
            MatcherId::Pattern8BareTitle => "Title (bare, fallback)",
            MatcherId::Pattern9IrregularSeriesContainer => "Author-Series (irregular separators)",
        }
    }
}

/// One matcher's successful result: the fields it read, its id, and its
/// specificity score (used by [`resolve`] to pick a winner or detect a
/// tie).
#[derive(Debug, Clone, PartialEq)]
pub struct MatcherMatch {
    pub id: MatcherId,
    pub fields: ParsedFields,
    pub score: u32,
}

/// The result of running all nine matchers against a name.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchOutcome {
    /// Exactly one matcher achieved the top score.
    Matched(MatcherMatch),
    /// Two or more matchers tied at the top score (AC-301.2).
    Ambiguous(Vec<MatcherMatch>),
    /// No matcher produced a result at all (defensive; in practice pattern
    /// 8's universal fallback means this is reachable only for empty or
    /// all-whitespace input).
    NoMatch,
}

type MatcherFn = fn(&str) -> Option<MatcherMatch>;

/// Specificity order (most to least specific), matching the score ranking
/// documented on each `try_match`. This is the canonical list every matcher
/// belongs to; adding a tenth pattern means adding one entry here.
const ORDER: &[MatcherFn] = &[
    pattern4::try_match,
    pattern5::try_match,
    pattern9::try_match,
    pattern7::try_match,
    pattern6::try_match,
    pattern3::try_match,
    pattern1::try_match,
    pattern2::try_match,
    pattern8::try_match,
];

/// Run every matcher against `raw` and collect whichever ones matched.
fn run_all(raw: &str) -> Vec<MatcherMatch> {
    ORDER.iter().filter_map(|matcher| matcher(raw)).collect()
}

/// Resolve a set of candidate matches to a single outcome: the unique
/// top-scoring candidate, an [`MatchOutcome::Ambiguous`] tie, or
/// [`MatchOutcome::NoMatch`] if there were no candidates at all.
pub fn resolve(mut candidates: Vec<MatcherMatch>) -> MatchOutcome {
    if candidates.is_empty() {
        return MatchOutcome::NoMatch;
    }
    let max_score = candidates
        .iter()
        .map(|c| c.score)
        .max()
        .expect("checked non-empty above");
    candidates.retain(|c| c.score == max_score);
    if candidates.len() == 1 {
        MatchOutcome::Matched(candidates.remove(0))
    } else {
        MatchOutcome::Ambiguous(candidates)
    }
}

/// Run the full F-301 matcher set against a single raw name and resolve the
/// winner (AC-301.1, AC-301.2).
pub fn match_name(raw: &str) -> MatchOutcome {
    resolve(run_all(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(id: MatcherId, score: u32) -> MatcherMatch {
        MatcherMatch {
            id,
            fields: ParsedFields::default(),
            score,
        }
    }

    /// AC-301.2: a name matchable by two patterns resolves to the more
    /// specific (higher-scoring) one, not a tie.
    #[test]
    fn resolve_picks_the_strictly_higher_score() {
        let candidates = vec![
            stub(MatcherId::Pattern2AuthorDashTitle, 55),
            stub(MatcherId::Pattern6SeriesIndexTitle, 75),
        ];
        let outcome = resolve(candidates);
        match outcome {
            MatchOutcome::Matched(m) => assert_eq!(m.id, MatcherId::Pattern6SeriesIndexTitle),
            other => panic!("expected Matched(Pattern6), got {other:?}"),
        }
    }

    /// AC-301.2: equal-score ties return `Ambiguous`, carrying every tied
    /// candidate.
    #[test]
    fn resolve_reports_equal_score_ties_as_ambiguous() {
        let candidates = vec![
            stub(MatcherId::Pattern2AuthorDashTitle, 55),
            stub(MatcherId::Pattern9IrregularSeriesContainer, 55),
        ];
        let outcome = resolve(candidates);
        match outcome {
            MatchOutcome::Ambiguous(tied) => assert_eq!(tied.len(), 2),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn resolve_reports_no_candidates_as_no_match() {
        assert_eq!(resolve(Vec::new()), MatchOutcome::NoMatch);
    }

    #[test]
    fn ids_are_stable_pattern_n_strings_and_every_id_has_a_handle() {
        let all = [
            MatcherId::Pattern1TitleByAuthorLooseRoot,
            MatcherId::Pattern2AuthorDashTitle,
            MatcherId::Pattern3TitleByAuthorFolder,
            MatcherId::Pattern4YearAuthorTitleSeriesAward,
            MatcherId::Pattern5RankTitleAuthorYear,
            MatcherId::Pattern6SeriesIndexTitle,
            MatcherId::Pattern7Underscored,
            MatcherId::Pattern8BareTitle,
            MatcherId::Pattern9IrregularSeriesContainer,
        ];
        for (i, id) in all.iter().enumerate() {
            assert_eq!(id.as_str(), format!("pattern-{}", i + 1));
            assert!(!id.handle().is_empty());
        }
    }

    /// Pattern 8 is a universal fallback: any non-empty, non-whitespace
    /// input resolves to SOME match (never `NoMatch`), because pattern 8
    /// always claims it at the lowest score if nothing more specific does.
    #[test]
    fn non_empty_input_always_resolves_to_some_match() {
        for raw in ["completely unstructured text", "Rework", "x"] {
            assert!(!matches!(match_name(raw), MatchOutcome::NoMatch), "{raw:?}");
        }
    }

    #[test]
    fn empty_input_is_no_match() {
        assert_eq!(match_name(""), MatchOutcome::NoMatch);
        assert_eq!(match_name("   "), MatchOutcome::NoMatch);
    }
}
