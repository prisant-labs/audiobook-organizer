//! Pattern 9: `Author - Series Name` containers with irregular separators,
//! the `Frank Herbert-Dune-#1-Chronicles[1-8}` case: hyphens with no
//! surrounding spaces (distinguishing it from pattern 2's canonical
//! ` - `), an embedded `#N` marker, and (in the real example) a malformed
//! trailing bracket/brace. See the `matchers` module doc comment for why
//! the discovery doc's OTHER Pattern 9 example
//! (`Roald Dahl - Charlie and the Chocolate Factory`, canonical separator)
//! is deliberately NOT claimed here.

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;
use regex::Regex;
use std::sync::LazyLock;

static HASH_INDEX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#(\d+)").expect("valid #N regex"));

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    if raw.contains(" - ") {
        // Canonical separator: pattern 2's (or pattern 9's own irregular
        // signal is absent).
        return None;
    }
    // Require at least two hyphens (three `splitn` segments): a single
    // hyphenated word or title (`Anti-Fragile`) must not be mistaken for an
    // author/series pair.
    let mut parts = raw.splitn(3, '-');
    let author = parts.next()?.trim();
    let series = parts.next()?.trim();
    let rest = parts.next()?.trim();
    if author.is_empty() || series.is_empty() {
        return None;
    }
    let series_index = HASH_INDEX_RE.captures(rest).map(|caps| caps[1].to_string());

    Some(MatcherMatch {
        id: MatcherId::Pattern9IrregularSeriesContainer,
        fields: ParsedFields {
            author: Some(author.to_string()),
            series: Some(series.to_string()),
            series_index,
            ..Default::default()
        },
        score: 85,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_example_table() {
        let m = try_match("Frank Herbert-Dune-#1-Chronicles[1-8}")
            .expect("expected a match for the irregular Frank Herbert example");
        assert_eq!(m.id, MatcherId::Pattern9IrregularSeriesContainer);
        assert_eq!(m.fields.author.as_deref(), Some("Frank Herbert"));
        assert_eq!(m.fields.series.as_deref(), Some("Dune"));
        assert_eq!(m.fields.series_index.as_deref(), Some("1"));
    }

    #[test]
    fn canonical_separator_does_not_match() {
        assert!(try_match("Roald Dahl - Charlie and the Chocolate Factory").is_none());
        assert!(try_match("Andy Weir - Project Hail Mary").is_none());
    }

    #[test]
    fn a_single_hyphen_does_not_match() {
        // Needs at least two hyphen-delimited segments after the author.
        assert!(try_match("Some-Title").is_none());
    }
}
