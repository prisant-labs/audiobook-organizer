//! Pattern 4: `Year - Author - Title (Series #N) [noise]` with a `^` award
//! marker (Hugo/Nebula collections). The most structurally distinctive
//! pattern (leading 4-digit year, optional award marker, exactly three
//! dash-separated segments, optional trailing series parenthetical), hence
//! the highest specificity score.
//!
//! This matcher captures its own leading year (rather than relying on
//! `crate::parse::strip::strip_year_prefix`) because the year is
//! structurally part of what makes a string recognizable as THIS pattern;
//! see the `matchers` module doc comment for why year/rank-prefix stripping
//! is excluded from the default pre-matching noise bundle. Input is assumed
//! to already have trailing bracket/bitrate/size noise removed (that part
//! IS safe to strip ahead of matching, since it never removes the leading
//! year/author/title/series structure this matcher reads).

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;
use regex::Regex;
use std::sync::LazyLock;

static PATTERN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{4})(\^)?\s*-\s*(.+?)\s*-\s*(.+?)(?:\s*\(([^()]+)\))?$")
        .expect("valid pattern-4 regex")
});

/// Split a trailing series parenthetical (`Freyaverse #2`, `The Broken
/// Earth Book 1`) into a series name and index, when it fits one of the two
/// real discovery shapes. Otherwise the whole parenthetical is kept as the
/// series name with no index (still useful, never fabricated).
fn split_series_paren(content: &str) -> (Option<String>, Option<String>) {
    static HASH_INDEX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+?)\s*#(\d+)$").expect("valid series #N regex"));
    static BOOK_INDEX_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(.+?)\s+Book\s+(\d+)$").expect("valid series Book N regex")
    });

    if let Some(caps) = HASH_INDEX_RE.captures(content) {
        return (Some(caps[1].trim().to_string()), Some(caps[2].to_string()));
    }
    if let Some(caps) = BOOK_INDEX_RE.captures(content) {
        return (Some(caps[1].trim().to_string()), Some(caps[2].to_string()));
    }
    (Some(content.trim().to_string()), None)
}

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let caps = PATTERN_RE.captures(raw)?;
    let year: u16 = caps[1].parse().ok()?;
    let author = caps[3].trim();
    let title = caps[4].trim();
    if author.is_empty() || title.is_empty() {
        return None;
    }
    let (series, series_index) = match caps.get(5) {
        Some(m) => split_series_paren(m.as_str()),
        None => (None, None),
    };

    Some(MatcherMatch {
        id: MatcherId::Pattern4YearAuthorTitleSeriesAward,
        fields: ParsedFields {
            year: Some(year),
            author: Some(author.to_string()),
            title: Some(title.to_string()),
            series,
            series_index,
            ..Default::default()
        },
        score: 95,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        // Award-marker year, series with a "Book N" suffix.
        let m = try_match("2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1)")
            .expect("expected a match");
        assert_eq!(m.id, MatcherId::Pattern4YearAuthorTitleSeriesAward);
        assert_eq!(m.fields.year, Some(2016));
        assert_eq!(m.fields.author.as_deref(), Some("N.K. Jemisin"));
        assert_eq!(m.fields.title.as_deref(), Some("The Fifth Season"));
        assert_eq!(m.fields.series.as_deref(), Some("The Broken Earth"));
        assert_eq!(m.fields.series_index.as_deref(), Some("1"));

        // Plain year, series with a "#N" suffix.
        let m = try_match("2014 - Charles Stross - Neptune's Brood (Freyaverse #2)")
            .expect("expected a match");
        assert_eq!(m.fields.year, Some(2014));
        assert_eq!(m.fields.author.as_deref(), Some("Charles Stross"));
        assert_eq!(m.fields.title.as_deref(), Some("Neptune's Brood"));
        assert_eq!(m.fields.series.as_deref(), Some("Freyaverse"));
        assert_eq!(m.fields.series_index.as_deref(), Some("2"));

        // No series parenthetical at all (bulkrename_chatgpt.md 2.4).
        let m = try_match("2014 - Ann Leckie - Ancillary Justice").expect("expected a match");
        assert_eq!(m.fields.year, Some(2014));
        assert_eq!(m.fields.author.as_deref(), Some("Ann Leckie"));
        assert_eq!(m.fields.title.as_deref(), Some("Ancillary Justice"));
        assert_eq!(m.fields.series, None);
        assert_eq!(m.fields.series_index, None);
    }

    #[test]
    fn a_3_digit_rank_prefix_does_not_match() {
        assert!(try_match("1 - Ender's Game - Orson Scott Card - 1985").is_none());
    }
}
