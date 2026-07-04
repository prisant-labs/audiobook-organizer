//! Pattern 1: `Title by Author.m4b` (loose root files). 238 root files in
//! the 2026-03-25 baseline, 237 parse cleanly (FD-18, AC-301.3).
//!
//! Distinguished from pattern 3 (the folder variant of the same `X by Y`
//! shape) by the presence of a recognized audio-file extension: a folder
//! name never has one, a loose root file always does.

use super::{MatcherId, MatcherMatch};
use crate::parse::{strip_known_extension, ParsedFields};

/// AC-301.3: the one known non-clean outlier is a "FREE SAMPLE_" prefix
/// leaking into the title. A pattern-1 title containing an underscore is
/// exactly that smell: none of the three clean discovery examples
/// (`Sapiens`, `Atomic Habits`, `The Phoenix Project`) contain one, so
/// rejecting on it defeats the outlier without misclassifying real titles.
/// This falls through to pattern 8's low-confidence bare-title fallback
/// rather than a wrong split of title/author (the spec's "not a wrong
/// parse" requirement).
fn looks_like_leftover_noise(title: &str) -> bool {
    title.contains('_')
}

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let stem = strip_known_extension(raw);
    if stem == raw {
        // No recognized audio extension: not a loose root file.
        return None;
    }
    let idx = stem.rfind(" by ")?;
    let title = stem[..idx].trim();
    let author = stem[idx + " by ".len()..].trim();
    if title.is_empty() || author.is_empty() || looks_like_leftover_noise(title) {
        return None;
    }
    Some(MatcherMatch {
        id: MatcherId::Pattern1TitleByAuthorLooseRoot,
        fields: ParsedFields {
            title: Some(title.to_string()),
            author: Some(author.to_string()),
            ..Default::default()
        },
        score: 60,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "Sapiens by Yuval Noah Harari.m4b",
                "Sapiens",
                "Yuval Noah Harari",
            ),
            (
                "Atomic Habits by James Clear.m4b",
                "Atomic Habits",
                "James Clear",
            ),
            (
                "The Phoenix Project by Gene Kim.m4b",
                "The Phoenix Project",
                "Gene Kim",
            ),
        ];
        for (raw, expected_title, expected_author) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern1TitleByAuthorLooseRoot);
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
            assert_eq!(m.fields.author.as_deref(), Some(*expected_author));
        }
    }

    /// AC-301.3: the 1-of-238 outlier does not cleanly match pattern 1.
    #[test]
    fn non_clean_outlier_is_rejected() {
        assert!(try_match("FREE SAMPLE_ The Martian by Andy Weir.m4b").is_none());
    }

    #[test]
    fn folder_names_without_an_extension_do_not_match() {
        assert!(try_match("How to Think About AI - A Guide by Richard Susskind").is_none());
    }
}
