//! Pattern 3: `Title by Author` folder variant. Same `X by Y` shape as
//! pattern 1, distinguished by the ABSENCE of a recognized audio-file
//! extension (this is a folder name, not a loose file). Real examples embed
//! an optional subtitle before the " by ": `Title - Subtitle by Author`.

use super::{MatcherId, MatcherMatch};
use crate::parse::{strip_known_extension, ParsedFields};

pub(super) fn try_match(raw: &str) -> Option<MatcherMatch> {
    if strip_known_extension(raw) != raw {
        // Has a recognized extension: that is pattern 1's territory.
        return None;
    }
    let idx = raw.rfind(" by ")?;
    let title_and_subtitle = raw[..idx].trim();
    let author = raw[idx + " by ".len()..].trim();
    if title_and_subtitle.is_empty() || author.is_empty() {
        return None;
    }

    let (title, subtitle) = match title_and_subtitle.find(" - ") {
        Some(dash_idx) => (
            title_and_subtitle[..dash_idx].trim().to_string(),
            Some(
                title_and_subtitle[dash_idx + " - ".len()..]
                    .trim()
                    .to_string(),
            ),
        ),
        None => (title_and_subtitle.to_string(), None),
    };
    if title.is_empty() {
        return None;
    }

    Some(MatcherMatch {
        id: MatcherId::Pattern3TitleByAuthorFolder,
        fields: ParsedFields {
            title: Some(title),
            author: Some(author.to_string()),
            subtitle,
            ..Default::default()
        },
        score: 65,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_examples_table() {
        let cases: &[(&str, &str, Option<&str>, &str)] = &[
            (
                "How to Think About AI - A Guide for the Perplexe by Richard Susskind",
                "How to Think About AI",
                Some("A Guide for the Perplexe"),
                "Richard Susskind",
            ),
            (
                "Every Tool's a Hammer - Life Is What You Make It by Adam Savage",
                "Every Tool's a Hammer",
                Some("Life Is What You Make It"),
                "Adam Savage",
            ),
        ];
        for (raw, expected_title, expected_subtitle, expected_author) in cases {
            let m = try_match(raw).unwrap_or_else(|| panic!("expected a match for {raw:?}"));
            assert_eq!(m.id, MatcherId::Pattern3TitleByAuthorFolder);
            assert_eq!(m.fields.title.as_deref(), Some(*expected_title));
            assert_eq!(m.fields.subtitle.as_deref(), *expected_subtitle);
            assert_eq!(m.fields.author.as_deref(), Some(*expected_author));
        }
    }

    #[test]
    fn no_subtitle_is_none() {
        let m = try_match("Some Title by Some Author").unwrap();
        assert_eq!(m.fields.title.as_deref(), Some("Some Title"));
        assert_eq!(m.fields.subtitle, None);
    }

    #[test]
    fn loose_root_files_with_an_extension_do_not_match() {
        assert!(try_match("Sapiens by Yuval Noah Harari.m4b").is_none());
    }
}
