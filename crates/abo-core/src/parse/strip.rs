//! F-302 (noise strippers): composable, individually toggleable passes that
//! remove naming noise, adapted from the seed regexes in
//! `_local/prior-work/bulkrename_chatgpt.md` and `chatgpt_shell.md`.
//!
//! Every pass is anchored to the start or the end of the string (never the
//! middle), which is what makes the whole composed pipeline idempotent
//! (AC-302.2): a pass can only shrink the true boundary it targets, it can
//! never manufacture a NEW instance of a different pass's target pattern
//! away from that boundary, and running the same fixed sequence of anchored
//! passes twice in a row finds nothing left to remove the second time.
//!
//! Every pass also guards against zeroing a name to nothing (AC-302.4): if
//! removing the matched noise would leave an empty (or whitespace-only)
//! result, the pass leaves the input untouched instead. An all-noise name is
//! preserved, not erased.

use regex::Regex;
use std::sync::LazyLock;

/// A year captured from a leading `YYYY[^]` prefix (AC-302.3: "year
/// prefixes are extracted as a field before stripping"). `award_marker`
/// records whether the discovery `^` suffix (Hugo/Nebula award-winner
/// marker) was present; per the F-301 edge case this is provenance signal
/// for a later phase (FD-01 pack provenance, v0.3.0), not noise, so it is
/// recorded here rather than silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YearPrefixExtraction {
    pub year: u16,
    pub award_marker: bool,
}

/// Which passes [`strip`] applies, each independently toggleable
/// (F-302: "composable, individually toggleable passes").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripOptions {
    pub bracket_tags: bool,
    pub release_group_suffix: bool,
    pub bitrate: bool,
    pub size: bool,
    pub rank_prefix: bool,
    pub year_prefix: bool,
    pub underscores_to_spaces: bool,
}

impl StripOptions {
    /// Every pass enabled.
    pub fn all() -> Self {
        StripOptions {
            bracket_tags: true,
            release_group_suffix: true,
            bitrate: true,
            size: true,
            rank_prefix: true,
            year_prefix: true,
            underscores_to_spaces: true,
        }
    }

    /// Every pass disabled (a no-op pipeline; useful as a base to toggle
    /// individual passes on from).
    pub fn none() -> Self {
        StripOptions {
            bracket_tags: false,
            release_group_suffix: false,
            bitrate: false,
            size: false,
            rank_prefix: false,
            year_prefix: false,
            underscores_to_spaces: false,
        }
    }
}

/// The result of running [`strip`]: the cleaned text, and the year
/// (AC-302.3) if `year_prefix` was enabled and a leading year was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripResult {
    pub text: String,
    pub year: Option<u16>,
}

/// Run the enabled passes over `input`, in order: bracket tags,
/// release-group suffix, bitrate, size, rank prefix, year prefix,
/// underscores-to-spaces.
///
/// The WHOLE sequence runs to a FIXED POINT (repeated until a full round
/// changes the text not at all), not just once: removing one kind of noise
/// can expose another pass's target that was not previously reachable (a
/// bare release-group suffix like ` jZQ` can sit in front of a `[...]`
/// bracket, so removing the suffix exposes a NEW trailing bracket;
/// converting underscores to spaces can turn `2014_-_Title` into
/// `2014 - Title`, exposing a leading year prefix that the literal
/// underscore had blocked). Without the fixed-point loop, a single pass
/// through the sequence can leave the text one step short of what a SECOND
/// full `strip()` call would find, which is exactly an AC-302.2 idempotence
/// violation; looping here means a second call can never find anything
/// left to do. Every pass only shrinks or transforms (never lengthens) the
/// string, so the loop is guaranteed to terminate.
///
/// `year` is captured from whichever round's `year_prefix` pass actually
/// found one (it is never overwritten back to `None` by a later round that
/// no longer sees a year prefix, since by then the text has already had it
/// removed).
pub fn strip(input: &str, options: StripOptions) -> StripResult {
    let mut text = input.to_string();
    let mut year = None;

    loop {
        let before = text.clone();
        if options.bracket_tags {
            text = strip_bracket_tags(&text);
        }
        if options.release_group_suffix {
            text = strip_release_group_suffix(&text);
        }
        if options.bitrate {
            text = strip_bitrate(&text);
        }
        if options.size {
            text = strip_size(&text);
        }
        if options.rank_prefix {
            text = strip_rank_prefix(&text);
        }
        if options.year_prefix {
            let (remainder, extraction) = strip_year_prefix(&text);
            text = remainder;
            if let Some(e) = extraction {
                year = Some(e.year);
            }
        }
        if options.underscores_to_spaces {
            text = underscores_to_spaces(&text);
        }
        if text == before {
            break;
        }
    }

    StripResult { text, year }
}

/// Apply `re`'s replacement (to empty string) but never let the result
/// become empty (AC-302.4): if it would, keep the original `input`.
///
/// Trims ONLY when the regex actually matched something: if `re` found
/// nothing to remove, `input` is returned completely untouched, including
/// any leading/trailing whitespace it already had. This is load-bearing for
/// idempotence (AC-302.2): without it, a pass that found nothing to strip
/// could still silently trim whitespace another pass introduced earlier in
/// the same call, so a second full `strip()` call (which re-runs this same
/// pass first) would see a different, already-trimmed input and diverge.
fn strip_or_keep(input: &str, re: &Regex) -> String {
    let candidate = re.replace(input, "");
    if candidate.as_ref() == input {
        return input.to_string();
    }
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        input.to_string()
    } else {
        trimmed.to_string()
    }
}

// ---- bracket tags (203 folders, 2026-03-25 baseline) ----

static BRACKET_TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s*\[[^\[\]]*\]\s*$").expect("valid bracket-tag regex"));

/// Remove trailing `[...]` bracket tags, repeatedly, until none remain at
/// the end of the string (handles `Title [FIXED] [64k 12;00;00 100MB]`,
/// where two bracket groups sit back to back). Never removes a bracket that
/// is not at the true end (a mid-string bracket like `[1-21]` in `Dune
/// Universe Series [1-21] (Audiobooks)` is untouched), and never zeroes a
/// name that is entirely one bracket (AC-302.4).
pub fn strip_bracket_tags(input: &str) -> String {
    let mut current = input.to_string();
    loop {
        let replaced = BRACKET_TAG_RE.replace(&current, "");
        if replaced.as_ref() == current {
            // No trailing bracket found: stop. Deliberately does NOT fall
            // through to a trim, for the same idempotence reason
            // documented on `strip_or_keep`.
            break;
        }
        let trimmed = replaced.trim();
        if trimmed.is_empty() {
            // Would zero out the whole name (AC-302.4): keep the last
            // non-empty `current` instead.
            break;
        }
        current = trimmed.to_string();
    }
    current
}

// ---- release-group suffixes (` jZQ`, `[Thomas]`) ----

// A bracketed release-group tag: short, letters/spaces only (no digits, no
// `;` duration separators), which is exactly what distinguishes it from a
// bitrate/size/duration bracket like `[64k 12;00;34 660MB]`.
static RELEASE_GROUP_BRACKET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*\[[A-Za-z][A-Za-z ]{0,19}\]\s*$").expect("valid release-group bracket regex")
});

/// Remove a trailing release-group suffix: either a short letters-only
/// bracket (`[Thomas]`) or a bare trailing word with an internal
/// capitalization pattern real prose never uses (`jZQ`: lowercase first
/// letter, uppercase later letters), the classic scene/ripper-tag
/// signature. Ordinary trailing words (a title that happens to end in a
/// normal word) are left alone.
pub fn strip_release_group_suffix(input: &str) -> String {
    let after_bracket = strip_or_keep(input, &RELEASE_GROUP_BRACKET_RE);
    strip_bare_release_group_word(&after_bracket)
}

fn strip_bare_release_group_word(input: &str) -> String {
    let trimmed_end = input.trim_end();
    // `rfind` gives the whitespace char's START byte offset; a naive `+ 1`
    // assumes it is one byte, which panics on a multi-byte whitespace
    // character (e.g. U+00A0 NO-BREAK SPACE). Use `char_indices` and the
    // char's own UTF-8 length to land on a real char boundary.
    let Some(word_start) = trimmed_end
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
    else {
        return input.to_string();
    };
    let word = &trimmed_end[word_start..];
    if is_release_group_word(word) {
        let candidate = trimmed_end[..word_start].trim_end();
        if candidate.is_empty() {
            input.to_string()
        } else {
            candidate.to_string()
        }
    } else {
        input.to_string()
    }
}

/// A scene/ripper-tag word: 2-8 ASCII letters, at least one lowercase and
/// one uppercase letter, and NOT simple TitleCase (first letter uppercase,
/// every other letter lowercase) -- real words and real names are
/// TitleCase or all-lowercase; a tag like `jZQ` or `xVid` has an uppercase
/// letter appearing after a lowercase one, which ordinary English text does
/// not produce.
fn is_release_group_word(word: &str) -> bool {
    if word.len() < 2 || word.len() > 8 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let has_lower = word.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = word.chars().any(|c| c.is_ascii_uppercase());
    if !has_lower || !has_upper {
        return false;
    }
    let mut chars = word.chars();
    let first = chars.next().expect("checked non-empty above");
    let is_simple_titlecase = first.is_ascii_uppercase() && chars.all(|c| c.is_ascii_lowercase());
    !is_simple_titlecase
}

// ---- bitrate markers (170, 2026-03-25 baseline) ----

static BITRATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*\b\d{2,4}\s?kbps?\b\s*$").expect("valid bitrate regex"));

/// Remove a trailing bare bitrate marker (`128kbps`, `64 Kbps`). Real
/// discovery examples carry bitrate inside a `[...]` bracket, already
/// handled by [`strip_bracket_tags`]; this pass exists for bitrate markers
/// that appear bare (and so it can be toggled independently, per F-302).
pub fn strip_bitrate(input: &str) -> String {
    strip_or_keep(input, &BITRATE_RE)
}

// ---- size markers (214, 2026-03-25 baseline) ----

static SIZE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*\b\d+(?:\.\d+)?\s?(?:KB|MB|GB|TB)\b\s*$").expect("valid size regex")
});

/// Remove a trailing bare size marker (`22GB`, `1.06 MB`). Covers the real
/// `Jim Butcher - Dresden Files 22GB` case, where the size marker is bare
/// (no brackets) at the very end of the folder name.
pub fn strip_size(input: &str) -> String {
    strip_or_keep(input, &SIZE_RE)
}

// ---- rank prefixes (143, 2026-03-25 baseline) ----

static RANK_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\d{1,3}\s*-\s*").expect("valid rank-prefix regex"));

/// Remove a leading rank number (`1 - `, `16 - `, up to 3 digits so a
/// 4-digit year prefix is never mistaken for a rank; see
/// [`strip_year_prefix`]). Rank has no corresponding [`crate::parse::ParsedFields`]
/// field: unlike year, it is pure positional noise once the shape it
/// anchors (F-301 pattern 5) has been recognized, so this pass discards it
/// rather than returning a captured value.
pub fn strip_rank_prefix(input: &str) -> String {
    strip_or_keep(input, &RANK_PREFIX_RE)
}

// ---- year prefixes (116, 2026-03-25 baseline; AC-302.3 keep-as-field) ----

static YEAR_PREFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(\d{4})(\^)?\s*-\s*").expect("valid year-prefix regex"));

/// Remove a leading `YYYY - ` or `YYYY^ - ` prefix, returning the cleaned
/// remainder AND the extracted year (AC-302.3: captured as a field before
/// being stripped from the text, so no year data is lost even though the
/// text itself no longer carries it). If removing the prefix would leave an
/// empty remainder, the text is left untouched (AC-302.4), but the
/// extraction is still returned: identifying the year does not require
/// physically removing it.
pub fn strip_year_prefix(input: &str) -> (String, Option<YearPrefixExtraction>) {
    let Some(caps) = YEAR_PREFIX_RE.captures(input) else {
        return (input.to_string(), None);
    };
    let year_str = caps
        .get(1)
        .expect("group 1 always present on match")
        .as_str();
    let Ok(year) = year_str.parse::<u16>() else {
        return (input.to_string(), None);
    };
    let award_marker = caps.get(2).is_some();
    let extraction = YearPrefixExtraction { year, award_marker };

    let stripped = YEAR_PREFIX_RE.replace(input, "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        (input.to_string(), Some(extraction))
    } else {
        (trimmed.to_string(), Some(extraction))
    }
}

// ---- underscores to spaces ----

/// Replace every underscore with a space. Always safe (cannot introduce a
/// NEW bracket/digit/dash pattern another pass would need to react to) and
/// trivially idempotent (no underscores survive a first pass). Guards
/// against an all-underscore name collapsing to an all-whitespace (visually
/// empty) result, per the same AC-302.4 spirit as the other passes.
pub fn underscores_to_spaces(input: &str) -> String {
    let replaced = input.replace('_', " ");
    if replaced.trim().is_empty() {
        input.to_string()
    } else {
        replaced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC-302.1: each pass removes its target noise and leaves non-noise
    /// intact, table-driven against discovery-derived examples.
    #[test]
    fn bracket_tags_table() {
        let cases: &[(&str, &str)] = &[
            (
                "Neptune's Brood (Freyaverse #2) [64k 12;00;34 660MB]",
                "Neptune's Brood (Freyaverse #2)",
            ),
            (
                "The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]",
                "The Fifth Season (The Broken Earth Book 1)",
            ),
            (
                "14.0 - Cold Days [320k 18;48;03 2.52GB]",
                "14.0 - Cold Days",
            ),
            (
                "08.0 - Proven Guilty [320k 16;15;24 2.19GB]",
                "08.0 - Proven Guilty",
            ),
            (
                "(1) The Eye of the World [64k 29;57;14 826MB]",
                "(1) The Eye of the World",
            ),
            // multiple trailing bracket groups, back to back
            ("Title [FIXED] [64k 12;00;00 100MB]", "Title"),
            // a mid-string bracket ([1-21]) is not at the true end and is
            // left alone; the genuinely trailing bracket ([Thomas]) is
            // removed. (Release-group-specific handling of short
            // letters-only brackets like this one is ALSO covered
            // independently by strip_release_group_suffix, tested below;
            // bracket_tags itself is content-agnostic about what is
            // inside a trailing bracket.)
            (
                "Dune Universe Series [1-21] (Audiobooks) [Thomas]",
                "Dune Universe Series [1-21] (Audiobooks)",
            ),
            // non-noise: no trailing bracket, unchanged
            (
                "Andy Weir - Project Hail Mary",
                "Andy Weir - Project Hail Mary",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(strip_bracket_tags(input), *expected, "input: {input:?}");
        }
    }

    /// AC-302.4: a name that is entirely a bracket is preserved, not
    /// zeroed.
    #[test]
    fn bracket_tags_never_zero_an_all_noise_name() {
        let input = "[64k 12;00;00 100MB]";
        assert_eq!(strip_bracket_tags(input), input);
    }

    #[test]
    fn release_group_suffix_table() {
        let cases: &[(&str, &str)] = &[
            (
                "11 Years of Nebula Best Novel Nominees - 2014-2024 - 68 Audiobooks jZQ",
                "11 Years of Nebula Best Novel Nominees - 2014-2024 - 68 Audiobooks",
            ),
            (
                "Dune Universe Series [1-21] (Audiobooks) [Thomas]",
                "Dune Universe Series [1-21] (Audiobooks)",
            ),
            // ordinary trailing word, TitleCase, real content: untouched
            (
                "Andy Weir - Project Hail Mary",
                "Andy Weir - Project Hail Mary",
            ),
            // ordinary trailing word, all lowercase: untouched
            ("Out of the Silent Planet", "Out of the Silent Planet"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                strip_release_group_suffix(input),
                *expected,
                "input: {input:?}"
            );
        }
    }

    #[test]
    fn release_group_suffix_never_zeroes_an_all_noise_name() {
        let input = "[Thomas]";
        assert_eq!(strip_release_group_suffix(input), input);
    }

    #[test]
    fn bitrate_table() {
        let cases: &[(&str, &str)] = &[
            ("Some Title 128kbps", "Some Title"),
            ("Some Title 64 Kbps", "Some Title"),
            // no unit suffix: not a bitrate marker, untouched
            ("Fahrenheit 451", "Fahrenheit 451"),
        ];
        for (input, expected) in cases {
            assert_eq!(strip_bitrate(input), *expected, "input: {input:?}");
        }
    }

    #[test]
    fn size_table() {
        let cases: &[(&str, &str)] = &[
            (
                "Jim Butcher - Dresden Files 22GB",
                "Jim Butcher - Dresden Files",
            ),
            ("Some Collection 1.06GB", "Some Collection"),
            // "68 Audiobooks" is a count, not a byte-unit size: untouched
            (
                "11 Years of Nebula - 68 Audiobooks",
                "11 Years of Nebula - 68 Audiobooks",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(strip_size(input), *expected, "input: {input:?}");
        }
    }

    #[test]
    fn size_never_zeroes_an_all_noise_name() {
        let input = "22GB";
        assert_eq!(strip_size(input), input);
    }

    #[test]
    fn rank_prefix_table() {
        let cases: &[(&str, &str)] = &[
            (
                "1 - Ender's Game - Orson Scott Card - 1985",
                "Ender's Game - Orson Scott Card - 1985",
            ),
            (
                "16 - Brave New World - Aldous Huxley - 1932",
                "Brave New World - Aldous Huxley - 1932",
            ),
            ("143 - Some Book", "Some Book"),
            // a 4-digit year prefix must NOT be mistaken for a rank
            ("1985 - Something Else", "1985 - Something Else"),
        ];
        for (input, expected) in cases {
            assert_eq!(strip_rank_prefix(input), *expected, "input: {input:?}");
        }
    }

    #[test]
    fn year_prefix_table() {
        let cases: &[(&str, &str, Option<YearPrefixExtraction>)] = &[
            (
                "2014 - Charles Stross - Neptune's Brood",
                "Charles Stross - Neptune's Brood",
                Some(YearPrefixExtraction {
                    year: 2014,
                    award_marker: false,
                }),
            ),
            (
                "2016^ - N.K. Jemisin - The Fifth Season",
                "N.K. Jemisin - The Fifth Season",
                Some(YearPrefixExtraction {
                    year: 2016,
                    award_marker: true,
                }),
            ),
            (
                "2014 - Ann Leckie - Ancillary Justice",
                "Ann Leckie - Ancillary Justice",
                Some(YearPrefixExtraction {
                    year: 2014,
                    award_marker: false,
                }),
            ),
            // a rank prefix (< 4 digits) must NOT be mistaken for a year
            ("1 - Ender's Game", "1 - Ender's Game", None),
        ];
        for (input, expected_text, expected_year) in cases {
            let (text, year) = strip_year_prefix(input);
            assert_eq!(text, *expected_text, "input: {input:?}");
            assert_eq!(year, *expected_year, "input: {input:?}");
        }
    }

    #[test]
    fn year_prefix_extraction_survives_even_when_stripping_would_zero_the_text() {
        // Pathological input: nothing follows the year prefix. The text is
        // preserved untouched (AC-302.4), but the year is still identified
        // and returned (AC-302.3: capturing the value never depends on
        // whether the text was actually shortened).
        let (text, year) = strip_year_prefix("2014 - ");
        assert_eq!(text, "2014 - ");
        assert_eq!(
            year,
            Some(YearPrefixExtraction {
                year: 2014,
                award_marker: false
            })
        );
    }

    #[test]
    fn underscores_to_spaces_table() {
        assert_eq!(
            underscores_to_spaces("Chris_Dixon_-_Read_Write_Own"),
            "Chris Dixon - Read Write Own"
        );
        assert_eq!(
            underscores_to_spaces("Michael_Easter_-_Scarcity_Brain"),
            "Michael Easter - Scarcity Brain"
        );
        // no underscores: unchanged
        assert_eq!(underscores_to_spaces("Rework"), "Rework");
    }

    #[test]
    fn underscores_to_spaces_never_zeroes_an_all_underscore_name() {
        let input = "____";
        assert_eq!(underscores_to_spaces(input), input);
    }

    /// AC-302.2 (fixed-input sanity check; the exhaustive property lives in
    /// the proptest module below): running the full default pipeline twice
    /// yields the same TEXT as running it once, for every discovery example
    /// this module's table tests use. The idempotence claim is scoped to
    /// the text: `year` is a one-shot extraction (once a year prefix has
    /// been read out of the text on the first call, the second call's input
    /// no longer has one to find, so `year` correctly reads back `None`
    /// rather than "remembering" the earlier extraction; that is the
    /// intended behavior of a pure `&str -> _` function, not an idempotence
    /// violation of the text transformation).
    #[test]
    fn strip_all_is_idempotent_on_discovery_examples() {
        let examples = [
            "Neptune's Brood (Freyaverse #2) [64k 12;00;34 660MB]",
            "2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]",
            "11 Years of Nebula Best Novel Nominees - 2014-2024 - 68 Audiobooks jZQ",
            "Dune Universe Series [1-21] (Audiobooks) [Thomas]",
            "Jim Butcher - Dresden Files 22GB",
            "Chris_Dixon_-_Read_Write_Own",
            "[64k 12;00;00 100MB]",
            "____",
            "1 - Ender's Game - Orson Scott Card - 1985",
        ];
        for example in examples {
            let once = strip(example, StripOptions::all());
            let twice = strip(&once.text, StripOptions::all());
            assert_eq!(
                once.text, twice.text,
                "not idempotent for input: {example:?}"
            );
        }
    }

    /// AC-302.4, restated at the composed-pipeline level: no combination of
    /// enabled passes ever reduces a non-empty name to an empty one.
    #[test]
    fn strip_all_never_produces_an_empty_result() {
        let inputs = [
            "[64k 12;00;00 100MB]",
            "[Thomas]",
            "____",
            "22GB",
            "1 - ",
            "2014 - ",
        ];
        for input in inputs {
            let result = strip(input, StripOptions::all());
            assert!(
                !result.text.trim().is_empty(),
                "strip zeroed {input:?} to an empty result"
            );
        }
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Realistic name fragments (real separators, markers, and noise
        /// tokens lifted from the discovery corpus) composed with arbitrary
        /// unicode strings, per the brief: "a generator that composes
        /// realistic name fragments AND arbitrary unicode strings."
        fn arb_fragment() -> impl Strategy<Value = String> {
            prop_oneof![
                Just("Sapiens".to_string()),
                Just("by".to_string()),
                Just("Yuval Noah Harari".to_string()),
                Just("Jim Butcher".to_string()),
                Just("Dresden Files".to_string()),
                Just("22GB".to_string()),
                Just("[64k 12;39;51 349MB]".to_string()),
                Just("[Thomas]".to_string()),
                Just("jZQ".to_string()),
                Just("2014".to_string()),
                Just("2016".to_string()),
                Just("^".to_string()),
                Just("-".to_string()),
                Just("_-_".to_string()),
                Just("_".to_string()),
                Just("14.0".to_string()),
                Just("#1".to_string()),
                Just("(1)".to_string()),
            ]
        }

        fn arb_composed_name() -> impl Strategy<Value = String> {
            prop_oneof![
                3 => any::<String>(),
                7 => proptest::collection::vec(arb_fragment(), 0..8)
                    .prop_map(|parts| parts.join(" ")),
            ]
        }

        proptest! {
            /// AC-302.2 (the hard property test): `strip(strip(x)) == strip(x)`
            /// for every generated name. Scoped to `.text` (see the doc
            /// comment on `strip_all_is_idempotent_on_discovery_examples`
            /// for why `.year` is correctly one-shot, not idempotent in the
            /// same sense).
            #[test]
            fn strip_is_idempotent(name in arb_composed_name()) {
                let once = strip(&name, StripOptions::all());
                let twice = strip(&once.text, StripOptions::all());
                prop_assert_eq!(once.text, twice.text);
            }

            /// AC-302.4 restated as a property: stripping never turns a
            /// non-blank name into a blank one.
            #[test]
            fn strip_never_blanks_a_non_blank_name(name in arb_composed_name()) {
                let result = strip(&name, StripOptions::all());
                if !name.trim().is_empty() {
                    prop_assert!(!result.text.trim().is_empty());
                }
            }
        }
    }
}
