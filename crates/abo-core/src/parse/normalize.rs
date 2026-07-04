//! F-304 (name normalizer): pure functions that turn an extracted field
//! value (or any candidate name) into a single filesystem-safe path
//! component.
//!
//! Like the rest of `crate::parse`, this module is pure string logic: no
//! I/O, no database, no filesystem access, and (per the CFG RULE this
//! phase's brief calls out) nothing here is `cfg`-gated. The rules it
//! enforces describe Windows filename SEMANTICS (FD-19), but the code that
//! enforces them is platform-independent: a name that is illegal on Windows
//! is normalized identically whether the normalizer runs on Windows, Linux,
//! or macOS, so a plan built on one host is byte-identical on another
//! (NFR Determinism). This is normalization only; the plan-side path-safety
//! backstop that re-checks a whole target path is F-404 (v0.3.0), out of
//! scope here.
//!
//! The pipeline, in order (order is load-bearing, see each step):
//!   1. Unicode NFC normalization (AC-304.3).
//!   2. Remove Windows-illegal characters `<>:"/\|?*` and ASCII control
//!      characters (AC-304.1).
//!   3. Collapse internal whitespace runs to a single space and trim both
//!      ends (F-304 "collapse whitespace").
//!   4. Remove trailing dots and spaces (AC-304.1; Win32 strips them
//!      silently, so a name that only differs by them is not a distinct
//!      name on disk).
//!   5. Reserved device-name guard (AC-304.2): if the STEM (the part before
//!      the first dot) is a reserved DOS device name, suffix the stem with
//!      `_` so it is no longer reserved. Run after step 4 so a trailing-dot
//!      form (`CON.`) is caught, and after illegal-char removal so a name
//!      that only became reserved once illegal characters were stripped is
//!      still caught.
//!   6. Cap the component length (default [`DEFAULT_MAX_GRAPHEMES`]) with
//!      grapheme-safe, word-boundary truncation (AC-304.4), then re-trim any
//!      trailing dot/space the cut exposed, then re-run the reserved guard:
//!      a long title whose first word is a device name (`CON tinental ...`)
//!      can be cut down to a reserved name by truncation, which the step-5
//!      guard ran too early to see. The re-guard only ever fires on a short
//!      (<= 4 grapheme) truncation result, so its one-character suffix stays
//!      inside the cap.
//!   7. Never emit empty (AC-304.5): if every previous step left the
//!      component empty or whitespace-only, substitute [`PLACEHOLDER`].
//!
//! Every transformation that fired is recorded on
//! [`NormalizedComponent::flags`] so a later review surface can explain what
//! it changed and why, rather than silently rewriting a family's book name.

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Default maximum component length, measured in Unicode grapheme clusters
/// (AC-304.4). 120 is the product default; Windows itself allows up to 255
/// UTF-16 code units per component, so this is a deliberate readability cap
/// well inside the hard limit, configurable via [`NormalizeOptions`].
pub const DEFAULT_MAX_GRAPHEMES: usize = 120;

/// The non-empty fallback substituted when normalization would otherwise
/// yield an empty component (AC-304.5). Chosen to be human-meaningful, NFC,
/// free of illegal characters, not a reserved device name, and with no
/// trailing dot or space, so it satisfies every other rule by construction.
pub const PLACEHOLDER: &str = "untitled";

/// Windows-illegal characters that are never legal in a path component
/// (FD-19). `/` and `\` are separators; the rest are reserved by the Win32
/// naming rules. Stripped outright rather than substituted, per the spec's
/// "illegal-char strip".
const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// A single transformation the normalizer applied, recorded so a review
/// surface can explain the change. A component that was already safe carries
/// an empty flag list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizeFlag {
    /// One or more Windows-illegal characters (`<>:"/\|?*`) were removed.
    IllegalCharsRemoved,
    /// One or more ASCII control characters were removed.
    ControlCharsRemoved,
    /// Trailing dot(s) and/or space(s) were removed.
    TrailingDotOrSpaceRemoved,
    /// The stem was a reserved DOS device name and was suffixed to make it
    /// safe (AC-304.2).
    ReservedDeviceNameSuffixed,
    /// The component exceeded the length cap and was truncated at a word
    /// boundary (AC-304.4).
    Truncated,
    /// Normalization would have produced an empty component, so
    /// [`PLACEHOLDER`] was substituted (AC-304.5).
    EmptyReplacedWithPlaceholder,
}

/// Tunable knobs for [`normalize_component_with`]. [`Default`] is the
/// v0.2.0 production configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizeOptions {
    /// Maximum component length in grapheme clusters before word-boundary
    /// truncation (AC-304.4).
    pub max_graphemes: usize,
    /// Collapse internal runs of whitespace to a single ASCII space. On by
    /// default (F-304 "collapse whitespace"); when off, only leading and
    /// trailing whitespace is trimmed and internal runs are preserved.
    pub collapse_whitespace: bool,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        NormalizeOptions {
            max_graphemes: DEFAULT_MAX_GRAPHEMES,
            collapse_whitespace: true,
        }
    }
}

/// The result of normalizing one component: the safe component string and
/// the list of transformations that were applied to produce it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedComponent {
    pub component: String,
    pub flags: Vec<NormalizeFlag>,
}

/// Normalize one path component with the default [`NormalizeOptions`].
pub fn normalize_component(input: &str) -> NormalizedComponent {
    normalize_component_with(input, &NormalizeOptions::default())
}

/// Normalize one path component (AC-304.1..AC-304.5). See the module doc
/// comment for the ordered pipeline; the returned [`NormalizedComponent`] is
/// guaranteed to satisfy every rule: NFC, no illegal or control character,
/// no reserved device-name stem, no trailing dot or space, at most
/// `options.max_graphemes` graphemes, and never empty.
pub fn normalize_component_with(input: &str, options: &NormalizeOptions) -> NormalizedComponent {
    let mut flags = Vec::new();

    // 1. Unicode NFC normalization (AC-304.3): an NFC and an NFD spelling of
    //    the same title become the identical byte sequence here.
    let nfc: String = input.nfc().collect();

    // 2. Remove Windows-illegal and control characters (AC-304.1).
    let mut removed_illegal = false;
    let mut removed_control = false;
    let cleaned: String = nfc
        .chars()
        .filter(|c| {
            if ILLEGAL_CHARS.contains(c) {
                removed_illegal = true;
                false
            } else if c.is_control() {
                removed_control = true;
                false
            } else {
                true
            }
        })
        .collect();
    if removed_illegal {
        flags.push(NormalizeFlag::IllegalCharsRemoved);
    }
    if removed_control {
        flags.push(NormalizeFlag::ControlCharsRemoved);
    }

    // 3. Collapse internal whitespace and trim both ends.
    let collapsed = if options.collapse_whitespace {
        cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        cleaned.trim().to_string()
    };

    // 4. Remove trailing dots and spaces (AC-304.1).
    let trimmed = trim_trailing_dots_and_spaces(&collapsed);
    if trimmed.len() != collapsed.len() {
        flags.push(NormalizeFlag::TrailingDotOrSpaceRemoved);
    }

    // 5. Reserved device-name guard (AC-304.2).
    let mut reserved_flagged = false;
    let guarded = match guard_reserved_stem(&trimmed) {
        Some(safe) => {
            reserved_flagged = true;
            safe
        }
        None => trimmed,
    };

    // 6. Length cap with grapheme-safe word-boundary truncation (AC-304.4),
    //    then re-trim any trailing dot/space the cut exposed, then re-guard
    //    in case truncation carved a reserved name out of a longer title.
    let (capped, truncated) = truncate_on_word_boundary(&guarded, options.max_graphemes);
    let mut component = if truncated {
        flags.push(NormalizeFlag::Truncated);
        let retrimmed = trim_trailing_dots_and_spaces(&capped);
        match guard_reserved_stem(&retrimmed) {
            Some(safe) => {
                reserved_flagged = true;
                safe
            }
            None => retrimmed,
        }
    } else {
        capped
    };
    if reserved_flagged {
        flags.push(NormalizeFlag::ReservedDeviceNameSuffixed);
    }

    // 7. Never emit empty (AC-304.5).
    if component.trim().is_empty() {
        component = PLACEHOLDER.to_string();
        flags.push(NormalizeFlag::EmptyReplacedWithPlaceholder);
    }

    NormalizedComponent { component, flags }
}

/// Trim trailing `.` and ` ` (space) characters from the end of `s`. Other
/// trailing whitespace has already been removed by the collapse/trim step;
/// this specifically targets the dot-and-space pair Win32 strips silently.
fn trim_trailing_dots_and_spaces(s: &str) -> String {
    s.trim_end_matches(['.', ' ']).to_string()
}

/// If the STEM of `component` (the part before the first `.`) is a reserved
/// DOS device name (case-insensitive), return a made-safe form with `_`
/// appended to the stem; otherwise return `None`. `CON`, `CON.mp3`, and
/// `con.txt` are all reserved (Windows treats the device name regardless of
/// extension); `CONsole` and `COM0` are not.
fn guard_reserved_stem(component: &str) -> Option<String> {
    let (stem, rest_with_dot) = match component.split_once('.') {
        Some((stem, after)) => (stem, format!(".{after}")),
        None => (component, String::new()),
    };
    if is_reserved_device_name(stem) {
        Some(format!("{stem}_{rest_with_dot}"))
    } else {
        None
    }
}

/// Whether `stem` is exactly a reserved DOS device name (case-insensitive):
/// `CON`, `PRN`, `AUX`, `NUL`, `COM1`..`COM9`, or `LPT1`..`LPT9`. `COM0`,
/// `LPT0`, and any longer string (`CONSOLE`) are not reserved.
fn is_reserved_device_name(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    // COMn / LPTn where n is 1-9 (single digit, not 0).
    if upper.len() == 4 && (upper.starts_with("COM") || upper.starts_with("LPT")) {
        let last = upper.as_bytes()[3];
        return last.is_ascii_digit() && last != b'0';
    }
    false
}

/// Truncate `s` to at most `max` grapheme clusters, preferring to cut at a
/// word boundary (whitespace) so a word is not split mid-token, and never
/// splitting a grapheme cluster (AC-304.4). Returns the (possibly
/// unchanged) string and whether truncation occurred.
///
/// A `max` of 0 is treated as "no cut" (a zero-length cap would force the
/// empty-placeholder path for every input, which is never the intent); the
/// caller's default is [`DEFAULT_MAX_GRAPHEMES`].
fn truncate_on_word_boundary(s: &str, max: usize) -> (String, bool) {
    if max == 0 {
        return (s.to_string(), false);
    }
    let graphemes: Vec<&str> = s.graphemes(true).collect();
    if graphemes.len() <= max {
        return (s.to_string(), false);
    }
    let head: String = graphemes[..max].concat();
    // Prefer the last whitespace boundary inside the kept head, so we cut
    // between words rather than mid-word. If there is no interior whitespace
    // (a single very long token), fall back to the hard grapheme cut.
    match head.rfind(char::is_whitespace) {
        Some(pos) if pos > 0 => (head[..pos].trim_end().to_string(), true),
        _ => (head, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(input: &str) -> NormalizedComponent {
        normalize_component(input)
    }

    /// AC-304.1: illegal Windows characters and trailing dots/spaces are
    /// removed (table-driven).
    #[test]
    fn illegal_chars_and_trailing_dot_space_table() {
        let cases: &[(&str, &str, &[NormalizeFlag])] = &[
            (
                "What: A Title?",
                "What A Title",
                &[NormalizeFlag::IllegalCharsRemoved],
            ),
            (
                "AC/DC Live",
                "ACDC Live",
                &[NormalizeFlag::IllegalCharsRemoved],
            ),
            (
                "Quo Vadis*",
                "Quo Vadis",
                &[NormalizeFlag::IllegalCharsRemoved],
            ),
            (
                "Star <Wars>",
                "Star Wars",
                &[NormalizeFlag::IllegalCharsRemoved],
            ),
            (
                "Some Book.",
                "Some Book",
                &[NormalizeFlag::TrailingDotOrSpaceRemoved],
            ),
            (
                "Some Book ",
                "Some Book",
                // The collapse/trim step already removed the trailing space,
                // so the dedicated trailing-trim step sees nothing to do:
                // no flag, which is correct (the space is gone either way).
                &[],
            ),
            (
                "Trailing Dots...",
                "Trailing Dots",
                &[NormalizeFlag::TrailingDotOrSpaceRemoved],
            ),
            (
                "Mixed End. . ",
                "Mixed End",
                &[NormalizeFlag::TrailingDotOrSpaceRemoved],
            ),
            // A clean name is untouched and carries no flags.
            ("Project Hail Mary", "Project Hail Mary", &[]),
        ];
        for (input, expected, expected_flags) in cases {
            let result = norm(input);
            assert_eq!(result.component, *expected, "input: {input:?}");
            assert_eq!(result.flags, *expected_flags, "flags for input: {input:?}");
        }
    }

    /// AC-304.2: reserved device names are detected and made safe, never
    /// emitted verbatim, including the with-extension forms.
    #[test]
    fn reserved_device_names_table() {
        let cases: &[(&str, &str)] = &[
            ("CON", "CON_"),
            ("con", "con_"),
            ("Con", "Con_"),
            ("CON.mp3", "CON_.mp3"),
            ("con.txt", "con_.txt"),
            ("NUL", "NUL_"),
            ("PRN", "PRN_"),
            ("AUX", "AUX_"),
            ("COM1", "COM1_"),
            ("COM9.m4b", "COM9_.m4b"),
            ("LPT1", "LPT1_"),
            ("lpt9.flac", "lpt9_.flac"),
        ];
        for (input, expected) in cases {
            let result = norm(input);
            assert_eq!(result.component, *expected, "input: {input:?}");
            assert!(
                result
                    .flags
                    .contains(&NormalizeFlag::ReservedDeviceNameSuffixed),
                "expected ReservedDeviceNameSuffixed flag for {input:?}"
            );
        }
    }

    /// AC-304.2 (negative cases): near-misses that are NOT reserved device
    /// names are left alone.
    #[test]
    fn reserved_name_near_misses_are_not_touched() {
        for input in [
            "CONSOLE",
            "Console",
            "COM0",
            "LPT0",
            "COM10",
            "Constantine",
            "AUXiliary",
        ] {
            let result = norm(input);
            assert_eq!(result.component, input, "must not alter {input:?}");
            assert!(
                !result
                    .flags
                    .contains(&NormalizeFlag::ReservedDeviceNameSuffixed),
                "must not flag {input:?} as reserved"
            );
        }
    }

    /// AC-304.3: NFC and NFD forms of the same title normalize to a
    /// byte-identical component. This is the exact P1 fixture twin pair
    /// (`Caf\u{00e9} Society` NFC vs `Cafe\u{0301} Society` NFD).
    #[test]
    fn nfc_and_nfd_forms_normalize_identically() {
        let nfc = "Caf\u{00e9} Society";
        let nfd = "Cafe\u{0301} Society";
        // Precondition: the two inputs really are distinct byte sequences,
        // so the assertion below is not vacuous.
        assert_ne!(
            nfc, nfd,
            "test inputs must be byte-distinct NFC vs NFD forms"
        );

        let a = norm(nfc);
        let b = norm(nfd);
        assert_eq!(a.component, b.component);
        assert_eq!(a.component.as_bytes(), b.component.as_bytes());
        // And the canonical form is the NFC one.
        assert_eq!(a.component, "Caf\u{00e9} Society");
    }

    /// AC-304.4: the component length is capped at the configured default
    /// with word-boundary truncation that never splits a grapheme.
    #[test]
    fn length_cap_truncates_on_a_word_boundary() {
        // 130 graphemes of real words: "word " repeated. The cap is 120, so
        // truncation must fire and land on a space boundary (no partial
        // "word").
        let long = "alpha bravo charlie delta echo foxtrot golf hotel india juliett kilo lima mike november oscar papa quebec romeo sierra tango uniform victor";
        let result = norm(long);
        assert!(
            result.flags.contains(&NormalizeFlag::Truncated),
            "expected Truncated flag"
        );
        assert!(
            result.component.graphemes(true).count() <= DEFAULT_MAX_GRAPHEMES,
            "capped length {} exceeds {DEFAULT_MAX_GRAPHEMES}",
            result.component.graphemes(true).count()
        );
        // Word-boundary: the truncated result is a prefix of the input at a
        // whole-word cut, so it never ends mid-token and has no trailing
        // space.
        assert!(long.starts_with(&result.component));
        assert!(!result.component.ends_with(' '));
        assert!(!result.component.is_empty());
    }

    /// AC-304.4 (grapheme safety): a string of multi-code-point graphemes is
    /// never cut through the middle of a cluster. Uses family-emoji ZWJ
    /// sequences (each several code points) with no interior spaces, forcing
    /// the hard-cut path; the result must still be a whole number of
    /// graphemes.
    #[test]
    fn truncation_never_splits_a_grapheme() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let many = family.repeat(200);
        let opts = NormalizeOptions {
            max_graphemes: 10,
            collapse_whitespace: true,
        };
        let result = normalize_component_with(&many, &opts);
        let count = result.component.graphemes(true).count();
        assert!(count <= 10, "grapheme count {count} exceeds cap 10");
        // Every grapheme in the output is the intact family cluster: joining
        // them back reproduces a whole multiple of the cluster, so none was
        // split.
        for g in result.component.graphemes(true) {
            assert_eq!(g, family, "a grapheme cluster was split: {g:?}");
        }
    }

    /// AC-304.5: normalization never yields an empty component; an
    /// all-illegal or all-trimmed name produces the placeholder.
    #[test]
    fn never_emits_empty_component() {
        let cases = &[
            "<<<>>>", // all illegal
            "///",    // all separators
            "...",    // all trailing dots (trimmed to empty)
            "   ",    // all whitespace
            "",       // empty input
            "*?*?",   // all illegal
        ];
        for input in cases {
            let result = norm(input);
            assert!(!result.component.is_empty(), "empty for input {input:?}");
            assert_eq!(result.component, PLACEHOLDER, "input {input:?}");
            assert!(
                result
                    .flags
                    .contains(&NormalizeFlag::EmptyReplacedWithPlaceholder),
                "expected placeholder flag for {input:?}"
            );
        }
    }

    /// A reserved name that also had a trailing dot is caught: `CON.`
    /// trims to `CON`, which the reserved guard then makes safe.
    #[test]
    fn reserved_name_with_trailing_dot_is_guarded() {
        let result = norm("CON.");
        assert_eq!(result.component, "CON_");
        assert!(result
            .flags
            .contains(&NormalizeFlag::ReservedDeviceNameSuffixed));
    }

    /// Whitespace collapse: internal runs collapse to a single space by
    /// default; disabling the option preserves interior runs but still
    /// trims the ends.
    #[test]
    fn whitespace_collapse_is_optional() {
        let messy = "  The   Long    Way  ";
        assert_eq!(norm(messy).component, "The Long Way");

        let opts = NormalizeOptions {
            collapse_whitespace: false,
            ..Default::default()
        };
        assert_eq!(
            normalize_component_with(messy, &opts).component,
            "The   Long    Way"
        );
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Realistic component fragments (real titles, illegal characters,
        /// reserved names, unicode, trailing dots/spaces) composed with
        /// arbitrary unicode strings, so the invariants below are checked
        /// against inputs no human enumerated (test-strategy.md Section 2.1).
        fn arb_fragment() -> impl Strategy<Value = String> {
            prop_oneof![
                Just("Project Hail Mary".to_string()),
                Just("The Fifth Season".to_string()),
                Just("Caf\u{00e9}".to_string()),
                Just("Cafe\u{0301}".to_string()),
                Just("CON".to_string()),
                Just("COM1".to_string()),
                Just("NUL.mp3".to_string()),
                Just("<illegal>".to_string()),
                Just(":\"/\\|?*".to_string()),
                Just("trailing.".to_string()),
                Just("trailing ".to_string()),
                Just("...".to_string()),
                Just(".".to_string()),
                Just(" ".to_string()),
            ]
        }

        fn arb_component() -> impl Strategy<Value = String> {
            prop_oneof![
                3 => any::<String>(),
                7 => proptest::collection::vec(arb_fragment(), 0..6)
                    .prop_map(|parts| parts.join(" ")),
            ]
        }

        proptest! {
            /// The output invariants (test-strategy.md Section 2.1): no
            /// illegal or control character, no reserved device-name stem,
            /// no trailing dot or space, NFC, within the cap, never empty.
            #[test]
            fn output_satisfies_every_invariant(input in arb_component()) {
                let result = normalize_component(&input);
                let out = &result.component;

                prop_assert!(!out.is_empty(), "output is empty for {input:?}");

                // No illegal or control characters remain.
                for c in out.chars() {
                    prop_assert!(
                        !ILLEGAL_CHARS.contains(&c),
                        "illegal char {c:?} in output {out:?}"
                    );
                    prop_assert!(!c.is_control(), "control char in output {out:?}");
                }

                // No trailing dot or space.
                prop_assert!(!out.ends_with('.'), "trailing dot in {out:?}");
                prop_assert!(!out.ends_with(' '), "trailing space in {out:?}");

                // Reserved device-name stem is guarded away.
                let stem = out.split_once('.').map(|(s, _)| s).unwrap_or(out.as_str());
                prop_assert!(
                    !is_reserved_device_name(stem),
                    "reserved stem {stem:?} in output {out:?}"
                );

                // NFC-stable: normalizing the output again is a no-op.
                let nfc: String = out.nfc().collect();
                prop_assert_eq!(&nfc, out, "output is not NFC-stable");

                // Within the grapheme cap.
                prop_assert!(
                    out.graphemes(true).count() <= DEFAULT_MAX_GRAPHEMES,
                    "output exceeds the grapheme cap: {out:?}"
                );
            }

            /// Idempotence: normalizing an already-normalized component
            /// yields the identical component (the placeholder and every
            /// safe form are fixed points).
            #[test]
            fn normalization_is_idempotent(input in arb_component()) {
                let once = normalize_component(&input);
                let twice = normalize_component(&once.component);
                prop_assert_eq!(once.component, twice.component);
            }
        }
    }
}
