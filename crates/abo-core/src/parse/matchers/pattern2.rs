//! Pattern 2: `Author - Title` (genre subfolders). The lowest-specificity
//! two-segment shape: any other pattern that also happens to fit `X - Y`
//! (pattern 6's index-title, pattern 9's irregular container) is scored
//! higher and wins per [`super::resolve`], so this matcher does not need to
//! defensively exclude those shapes itself.
//!
//! Input is assumed already noise-stripped upstream (bracket tags,
//! bare size/bitrate markers): see `crate::parse::parse_preview` and
//! `crate::parse::tests` for the raw-example composition.

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let idx = raw.find(" - ")?;
    let author = raw[..idx].trim();
    let title = raw[idx + " - ".len()..].trim();
    if author.is_empty() || title.is_empty() {
        return None;
    }
    Some(MatcherMatch {
        id: MatcherId::Pattern2AuthorDashTitle,
        fields: ParsedFields {
            author: Some(author.to_string()),
            title: Some(title.to_string()),
            ..Default::default()
        },
        score: 55,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        // Noise-stripped forms of the real examples (the raw, noisy
        // versions are covered end to end in crate::parse::tests).
        let cases: &[(&str, &str, &str)] = &[
            (
                "Jim Butcher - Dresden Files",
                "Jim Butcher",
                "Dresden Files",
            ),
            (
                "Andy Weir - Project Hail Mary",
                "Andy Weir",
                "Project Hail Mary",
            ),
            (
                "Cixin Liu - The Three-Body Problem",
                "Cixin Liu",
                "The Three-Body Problem",
            ),
            // See matchers::mod doc comment: documented as Pattern 2, not
            // Pattern 9, for this pure-string matcher.
            (
                "Roald Dahl - Charlie and the Chocolate Factory",
                "Roald Dahl",
                "Charlie and the Chocolate Factory",
            ),
        ];
        for (raw, expected_author, expected_title) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern2AuthorDashTitle);
            assert_eq!(m.fields.author.as_deref(), Some(*expected_author));
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
        }
    }

    #[test]
    fn no_separator_does_not_match() {
        assert!(try_match("Rework").is_none());
    }
}
