//! F-301 (pattern matcher set) and F-302 (noise strippers): the first half of
//! the v0.2.0 parse stack.
//!
//! This module and its children (`matchers`) are pure string logic: no I/O,
//! no database, no filesystem access, and (per the CFG RULE this phase's
//! brief calls out) nothing here is `cfg`-gated at all, because nothing here
//! is platform-specific. A folder or file name goes in; fields plus
//! provenance come out.
//!
//! Module boundary (v0.2.0 implementation plan, Phase 4): this dispatch
//! (P4a) owns [`matchers`] (F-301) and `strip` (F-302, landing in the next
//! commit on this branch) plus the parser-coverage metric called for by
//! Phase 4 step 5. It does NOT own `extract` (F-303, confidence merge across
//! the folder tree) or `normalize` (F-304, filesystem-safe output names) --
//! those land in a later P4b dispatch on this same branch and consume
//! [`ParsedFields`] and [`matchers::MatchOutcome`] as their input shape.

pub mod matchers;

/// The fields a matcher (or, later, F-303's confidence merge) can produce.
/// Every field is optional: a matcher fills in only what its pattern shape
/// actually exposes, and never fabricates a value it did not read from the
/// string (spec F-303 edge case: "never a fabricated value").
///
/// `series_index` and `year` are kept as their own types rather than folded
/// into a generic string bag, but `series_index` is deliberately a `String`,
/// not a number: real examples include a meaningful leading zero and decimal
/// suffix (`"08.0"`, `"14.0"`) whose exact text a later display/normalize
/// step may care about, so this layer preserves it losslessly rather than
/// guessing a numeric representation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedFields {
    pub title: Option<String>,
    pub author: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub year: Option<u16>,
    pub narrator: Option<String>,
    pub subtitle: Option<String>,
}

/// Strip a known audio-file extension from the end of `name`, if present.
/// Used only where a matcher is specifically for loose FILE names (Pattern
/// 1); folder-shaped patterns never call this, so a folder name that happens
/// to contain a literal `.` (e.g. `N.K. Jemisin`) is never mistaken for
/// having an extension. Deliberately a fixed allowlist, not
/// `Path::file_stem` (which would strip after the LAST `.` unconditionally
/// and mistreat a folder name with a real internal `.`).
pub(crate) fn strip_known_extension(name: &str) -> &str {
    const EXTENSIONS: &[&str] = &[".m4b", ".mp3", ".m4a", ".opus", ".wma", ".flac", ".mp4"];
    let lower = name.to_ascii_lowercase();
    for ext in EXTENSIONS {
        if lower.ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}
