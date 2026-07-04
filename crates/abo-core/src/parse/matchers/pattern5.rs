//! Pattern 5: `N - Title - Author - Year` (Top 100 Sci-Fi style rankings).
//! Distinguished from pattern 4 by WHERE the year sits (trailing here,
//! leading there) and from pattern 2 by requiring exactly four dash-
//! separated segments with a numeric rank at the start and a 4-digit year
//! at the end.
//!
//! The rank number is used only to confirm the shape; like
//! `crate::parse::strip::strip_rank_prefix`, it has no corresponding
//! [`ParsedFields`] field and is discarded once the match succeeds.

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;
use regex::Regex;
use std::sync::LazyLock;

static PATTERN_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Rank is bounded to 1-3 digits (matching `strip::strip_rank_prefix`'s
    // bound and the 2026-03-25 real baseline, ranks 1..=143), which also
    // keeps this shape from ever colliding with a 4-digit leading YEAR
    // (pattern 4's territory).
    Regex::new(r"^\d{1,3}\s*-\s*(.+?)\s*-\s*(.+?)\s*-\s*(\d{4})$").expect("valid pattern-5 regex")
});

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let caps = PATTERN_RE.captures(raw)?;
    let title = caps[1].trim();
    let author = caps[2].trim();
    let year: u16 = caps[3].parse().ok()?;
    if title.is_empty() || author.is_empty() {
        return None;
    }
    Some(MatcherMatch {
        id: MatcherId::Pattern5RankTitleAuthorYear,
        fields: ParsedFields {
            title: Some(title.to_string()),
            author: Some(author.to_string()),
            year: Some(year),
            ..Default::default()
        },
        score: 90,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        let cases: &[(&str, &str, &str, u16)] = &[
            (
                "1 - Ender's Game - Orson Scott Card - 1985",
                "Ender's Game",
                "Orson Scott Card",
                1985,
            ),
            (
                "16 - Brave New World - Aldous Huxley - 1932",
                "Brave New World",
                "Aldous Huxley",
                1932,
            ),
        ];
        for (raw, expected_title, expected_author, expected_year) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern5RankTitleAuthorYear);
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
            assert_eq!(m.fields.author.as_deref(), Some(*expected_author));
            assert_eq!(m.fields.year, Some(*expected_year));
        }
    }

    #[test]
    fn only_three_segments_does_not_match() {
        assert!(try_match("2014 - Charles Stross - Neptune's Brood").is_none());
    }
}
