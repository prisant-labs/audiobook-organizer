//! Pattern 8: bare `Title`, no author. This is also the universal
//! last-resort fallback: it accepts any non-empty, non-whitespace input
//! verbatim as a title, so `match_name` never returns `NoMatch` on real
//! content. Both roles share this one matcher, but at two different
//! scores, which is what keeps the parser-coverage metric ([`crate::parse::coverage`])
//! meaningful:
//!
//! - A genuine bare title (no leftover structural marker from any other
//!   pattern) scores [`BARE_TITLE_SCORE`]: a complete, confident parse for
//!   ITS shape, counted as "parsed cleanly."
//! - A name that still carries a structural marker another pattern would
//!   normally key on (` by `, an underscore) but got rejected by that
//!   pattern's own content guard scores [`DEGRADED_FALLBACK_SCORE`]: this is
//!   the AC-301.3 outlier's path (rejected by pattern 1's underscore guard),
//!   counted as "not parsed cleanly" so it correctly pulls down the
//!   237-of-238 baseline instead of silently passing as a clean title.
//!
//! Either way the result is never a WRONG parse (the spec's AC-301.3
//! requirement): the whole string is kept verbatim as the title, not split
//! into a mismatched title/author pair.

use super::{MatcherId, MatcherMatch};
use crate::parse::{strip_known_extension, ParsedFields};

/// Score for a genuine bare title: still the lowest of any REAL pattern
/// score (pattern 2's 55 is the next lowest), so it never wins a tie
/// against a more specific match, but high enough to count as a clean
/// parse in [`crate::parse::coverage`].
pub(crate) const BARE_TITLE_SCORE: u32 = 50;

/// Score for the degraded fallback: strictly lower than
/// [`BARE_TITLE_SCORE`], so [`crate::parse::coverage`] can tell the two
/// apart.
pub(crate) const DEGRADED_FALLBACK_SCORE: u32 = 10;

/// A leftover structural marker from another pattern's shape that a
/// content guard (elsewhere) rejected: `" by "` (pattern 1/3's separator)
/// or a literal underscore (pattern 1's leftover-noise smell, pattern 7's
/// separator character). Real bare titles in the discovery corpus contain
/// neither.
fn carries_a_rejected_structural_marker(title: &str) -> bool {
    title.contains(" by ") || title.contains('_')
}

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let stem = strip_known_extension(raw);
    let title = stem.trim();
    if title.is_empty() {
        return None;
    }
    let score = if carries_a_rejected_structural_marker(title) {
        DEGRADED_FALLBACK_SCORE
    } else {
        BARE_TITLE_SCORE
    };
    Some(MatcherMatch {
        id: MatcherId::Pattern8BareTitle,
        fields: ParsedFields {
            title: Some(title.to_string()),
            ..Default::default()
        },
        score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        for raw in [
            "Out of the Silent Planet",
            "The Soul of a New Machine",
            "Rework",
        ] {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern8BareTitle);
            assert_eq!(m.fields.title.as_deref(), Some(raw));
            assert_eq!(
                m.score, BARE_TITLE_SCORE,
                "expected the clean bare-title tier"
            );
        }
    }

    /// AC-301.3: the outlier still gets a (low-confidence, verbatim)
    /// match, never a wrong parse, and is scored into the degraded tier so
    /// it counts against coverage rather than passing as clean.
    #[test]
    fn fallback_never_wrongly_splits_the_outlier() {
        let m = try_match("FREE SAMPLE_ The Martian by Andy Weir.m4b").unwrap();
        assert_eq!(
            m.fields.title.as_deref(),
            Some("FREE SAMPLE_ The Martian by Andy Weir")
        );
        assert_eq!(m.fields.author, None);
        assert_eq!(m.score, DEGRADED_FALLBACK_SCORE);
    }

    #[test]
    fn empty_or_whitespace_only_does_not_match() {
        assert!(try_match("").is_none());
        assert!(try_match("   ").is_none());
    }
}
