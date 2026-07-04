//! F-301 (pattern matcher set) and F-302 (noise strippers): the first half of
//! the v0.2.0 parse stack.
//!
//! This module and its children (`matchers`, `strip`) are pure string logic:
//! no I/O, no database, no filesystem access, and (per the CFG RULE this
//! phase's brief calls out) nothing here is `cfg`-gated at all, because
//! nothing here is platform-specific. A folder or file name goes in; fields
//! plus provenance come out.
//!
//! Module boundary (v0.2.0 implementation plan, Phase 4): P4a landed
//! [`matchers`] (F-301) and [`strip`] (F-302) plus the parser-coverage
//! metric ([`coverage`]) called for by Phase 4 step 5. P4b adds [`normalize`]
//! (F-304, filesystem-safe output components) and, in the following commit,
//! `extract` (F-303, the confidence merge across the folder tree); both
//! consume [`ParsedFields`] and [`matchers::MatchOutcome`] as their input
//! shape and, like the rest of this module, are pure and ungated.
//!
//! [`parse_preview`] is a small compositional helper (strip general noise,
//! then run the matchers) used by this module's own tests and by the
//! coverage metric. It is deliberately NOT `extract.rs`: it does no
//! tree-walking, no confidence scoring, and no inheritance. P4b's extract
//! module decides the real production orchestration (in particular, how
//! [`strip::strip_year_prefix`]'s and [`strip::strip_rank_prefix`]'s
//! independently-captured values are merged into fields for patterns that do
//! not capture their own year/rank); `parse_preview` exists only to prove
//! the two P4a modules compose correctly and to feed the coverage metric.

pub mod matchers;
pub mod normalize;
pub mod strip;

use matchers::{MatchOutcome, MatcherId};

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

/// The general-noise bundle [`parse_preview`] applies before matching:
/// trailing bracket tags, bracketed/bare release-group suffixes, bitrate,
/// and size markers. Deliberately EXCLUDES `rank_prefix`, `year_prefix`, and
/// `underscores_to_spaces`: patterns 4, 5, 6, and 9 read their own leading
/// year/rank/index directly out of the raw prefix as part of self-
/// identifying their shape, and pattern 7 reads its own literal underscores
/// to self-identify (see `matchers::pattern7`); stripping any of those three
/// ahead of matching would erase the very signal those matchers key on.
fn general_noise_options() -> strip::StripOptions {
    strip::StripOptions {
        bracket_tags: true,
        release_group_suffix: true,
        bitrate: true,
        size: true,
        rank_prefix: false,
        year_prefix: false,
        underscores_to_spaces: false,
    }
}

/// Compose the two P4a modules: strip general noise, then run the matchers
/// on what remains. See the module doc comment for why this is not
/// `extract.rs`.
pub fn parse_preview(raw: &str) -> MatchOutcome {
    let cleaned = strip::strip(raw, general_noise_options());
    matchers::match_name(&cleaned.text)
}

/// A parser-coverage report (v0.2.0 implementation plan, Phase 4 step 5;
/// AC-301.4): the fraction of a name set that [`parse_preview`] resolves to
/// a confident match. A degraded [`matchers::MatcherId::Pattern8BareTitle`]
/// fallback (see `matchers::pattern8`'s two score tiers) does NOT count as
/// "clean": that is exactly the kind of name the ~90% freeze decision
/// (AC-301.4, release gate G-05) is watching for. A genuine bare-title
/// match (pattern 8's other tier) DOES count as clean: it is a complete,
/// confident parse for that shape, not a fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageReport {
    pub total: usize,
    pub clean: usize,
    pub non_parsing: Vec<String>,
}

impl CoverageReport {
    /// Fraction of names that parsed cleanly, in `[0.0, 1.0]`. An empty
    /// input set reports full coverage (vacuously true) rather than NaN.
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.clean as f64 / self.total as f64
        }
    }
}

/// Compute the coverage report for a set of names (folder or file names,
/// extension included where relevant). Pure: no I/O, so the caller supplies
/// whatever name set it wants measured (the fixture library, a subset of it,
/// or the real library's names once scanned).
pub fn coverage<'a, I: IntoIterator<Item = &'a str>>(names: I) -> CoverageReport {
    let mut total = 0usize;
    let mut clean = 0usize;
    let mut non_parsing = Vec::new();
    for name in names {
        total += 1;
        match parse_preview(name) {
            MatchOutcome::Matched(m)
                if m.id == MatcherId::Pattern8BareTitle
                    && m.score == matchers::DEGRADED_FALLBACK_SCORE =>
            {
                non_parsing.push(name.to_string());
            }
            MatchOutcome::Matched(_) => clean += 1,
            _ => non_parsing.push(name.to_string()),
        }
    }
    CoverageReport {
        total,
        clean,
        non_parsing,
    }
}

#[cfg(test)]
mod tests;
