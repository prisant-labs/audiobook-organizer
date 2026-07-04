//! Pattern 7: `Author_Name_-_Title`, underscored. Keyed on the literal
//! `_-_` separator in the RAW string (before any underscore-to-space
//! conversion), which is what keeps this pattern distinct from pattern 2:
//! pattern 2's separator requires real space characters, so it never fires
//! on an underscored name, and this matcher never runs on a
//! space-separated name (no `_-_` to find). No ambiguity, no tie.

use super::{MatcherId, MatcherMatch};
use crate::parse::ParsedFields;

const SEP: &str = "_-_";

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    let idx = raw.find(SEP)?;
    let author_raw = &raw[..idx];
    let title_raw = &raw[idx + SEP.len()..];
    if author_raw.is_empty() || title_raw.is_empty() {
        return None;
    }
    let author = author_raw.replace('_', " ");
    let title = title_raw.replace('_', " ");
    if author.trim().is_empty() || title.trim().is_empty() {
        return None;
    }
    Some(MatcherMatch {
        id: MatcherId::Pattern7Underscored,
        fields: ParsedFields {
            author: Some(author),
            title: Some(title),
            ..Default::default()
        },
        score: 80,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        let cases: &[(&str, &str, &str)] = &[
            (
                "Chris_Dixon_-_Read_Write_Own",
                "Chris Dixon",
                "Read Write Own",
            ),
            (
                "Michael_Easter_-_Scarcity_Brain",
                "Michael Easter",
                "Scarcity Brain",
            ),
        ];
        for (raw, expected_author, expected_title) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern7Underscored);
            assert_eq!(m.fields.author.as_deref(), Some(*expected_author));
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
        }
    }

    #[test]
    fn space_separated_names_do_not_match() {
        assert!(try_match("Andy Weir - Project Hail Mary").is_none());
    }
}
