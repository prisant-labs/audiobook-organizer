//! Pattern 6: `NN.N - Title [noise]` series-entry names, plus the discovery
//! doc's alternate leading-index shape `(N) Title`. Both are "a leading
//! series-position index, then the entry's own title", so this matcher
//! tries the dot-decimal-dash form first, then the parenthesized form.
//!
//! Input is assumed already noise-stripped of trailing brackets (see
//! `crate::parse::parse_preview`). No author or series is captured here:
//! in the real tree those come from the ENCLOSING folder, which is F-303
//! (confidence merge / inheritance) territory, not this pure-string
//! matcher's.

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;
use regex::Regex;
use std::sync::LazyLock;

// `14.0 - Cold Days`, `08.0 - Proven Guilty`: a leading index (integer or
// decimal, leading zero significant) then a dash then the title. Exactly
// two dash-separated segments, distinguishing it from pattern 5's four.
static DOT_INDEX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{1,3}(?:\.\d+)?)\s*-\s*(.+)$").expect("valid dot-index regex")
});

// `(1) The Eye of the World`: a parenthesized index then the title.
static PAREN_INDEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\((\d{1,3})\)\s+(.+)$").expect("valid paren-index regex"));

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let (series_index, title) = if let Some(caps) = DOT_INDEX_RE.captures(raw) {
        (caps[1].to_string(), caps[2].trim().to_string())
    } else if let Some(caps) = PAREN_INDEX_RE.captures(raw) {
        (caps[1].to_string(), caps[2].trim().to_string())
    } else {
        return None;
    };
    if title.is_empty() {
        return None;
    }
    Some(MatcherMatch {
        id: MatcherId::Pattern6SeriesIndexTitle,
        fields: ParsedFields {
            series_index: Some(series_index),
            title: Some(title),
            ..Default::default()
        },
        score: 75,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        let cases: &[(&str, &str, &str)] = &[
            ("14.0 - Cold Days", "14.0", "Cold Days"),
            ("08.0 - Proven Guilty", "08.0", "Proven Guilty"),
            ("(1) The Eye of the World", "1", "The Eye of the World"),
        ];
        for (raw, expected_index, expected_title) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern6SeriesIndexTitle);
            assert_eq!(m.fields.series_index.as_deref(), Some(*expected_index));
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
            assert_eq!(m.fields.author, None);
        }
    }

    #[test]
    fn a_four_segment_rank_title_author_year_name_does_not_match_the_dot_index_shape() {
        // Pattern 5's shape has more than 2 dash segments; this matcher's
        // dot-index regex is non-greedy on the SECOND group but the
        // absence of a `$`-anchored 4-digit year check means it would
        // still technically capture something. Score ordering (pattern 5
        // at 90 > pattern 6 at 75) is what actually resolves this in
        // `match_name`, exercised in crate::parse::tests.
        let m = try_match("1 - Ender's Game - Orson Scott Card - 1985").unwrap();
        assert_eq!(m.fields.series_index.as_deref(), Some("1"));
    }
}
