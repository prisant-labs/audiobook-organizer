//! FD-14 tag-quality probe (release gate G-08): a bounded, deterministic,
//! READ-ONLY sample of embedded audio tags measured against the folder-first
//! parse, producing a field-completeness report and the numbers behind the G-08
//! verdict on whether the folder-first assumption (FD-14) holds.
//!
//! # Why this is feature-gated and not product code
//!
//! The whole point of FD-14 / D-15 / FD-03 is that v1 extraction is FOLDER-FIRST
//! and reads NO embedded tags (the tag reader, F-1101, is deferred past v1).
//! This probe exists solely to TEST that decision on the real library: it reads
//! a bounded sample of tags once, records how complete they are and how often
//! they agree with the folder-derived fields, and the recorded verdict tunes the
//! F-303 [`ConfidenceWeights`](crate::parse::extract::ConfidenceWeights). It is
//! therefore compiled only under the `probe` Cargo feature, never in the default
//! build or the shell, so no shipped code path ever reads a tag in v1.
//!
//! # The pieces
//!
//! - [`tags`] - the lofty read-only reader ([`read_tag_fields`]).
//! - [`deterministic_sample_indices`] - every-Nth index selection over a stable
//!   ordering, capped, so the sample is repeatable run to run.
//! - [`run_probe`] - reads tags for each [`ProbeInput`] and aggregates a
//!   [`TagProbeReport`]: per-field tag completeness plus the tag-vs-folder
//!   agreement rates.
//!
//! The caller (the ignored `tests/fd14_tag_probe.rs`) owns the scan and the
//! folder-first parse: it hands this module each sampled audio file's path and
//! the folder-derived title/author already computed by
//! [`crate::parse::extract`], so the comparison is against the exact fields the
//! product would use, not a re-derivation.

pub mod tags;

use std::path::PathBuf;

pub use tags::{read_tag_fields, TagFields, TagReadOutcome};

/// Pick every-Nth index over `total` items, capped at `cap`, deterministically.
///
/// With `total <= cap` every index is taken; otherwise the stride is
/// `total / cap` (at least 1) and indices `0, stride, 2*stride, ...` are taken
/// until `cap` is reached. Given a stable input ordering (the probe uses the
/// snapshot's path-sorted entry order), the selection is identical every run,
/// which is what makes the recorded FD-14 numbers reproducible.
pub fn deterministic_sample_indices(total: usize, cap: usize) -> Vec<usize> {
    if total == 0 || cap == 0 {
        return Vec::new();
    }
    if total <= cap {
        return (0..total).collect();
    }
    let stride = total / cap; // >= 1 because total > cap
    let mut out = Vec::with_capacity(cap);
    let mut i = 0;
    while i < total && out.len() < cap {
        out.push(i);
        i += stride;
    }
    out
}

/// One sampled audio entry handed to the probe: the file to read tags from, and
/// the folder-first fields the product parsed for that same entry (either may be
/// [`None`] when the folder name did not yield that field).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeInput {
    pub path: PathBuf,
    pub folder_title: Option<String>,
    pub folder_author: Option<String>,
}

/// The FD-14 field-completeness + agreement report over one sample.
///
/// Counts are all in FILES (of the `sampled` files). The `*_present` counts are
/// how many sampled files carried a usable tag of that field; the `*_comparable`
/// counts are how many files had BOTH a folder-derived value and a tag value for
/// that field (only those can agree or disagree), and `*_agree` how many of
/// those matched. Percentages are derived by the helper methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagProbeReport {
    /// Files sampled and attempted.
    pub sampled: usize,
    /// Files lofty opened and parsed (whether or not a tag was present).
    pub readable: usize,
    /// Files that carried at least one usable measured tag field.
    pub with_any_tag: usize,
    /// Files lofty could not parse at all.
    pub unreadable: usize,

    pub title_present: usize,
    pub author_present: usize,
    pub album_present: usize,
    pub narrator_present: usize,

    /// Folder-first coverage over the same sample, for context (how often the
    /// folder name yielded the field the tag is being compared against).
    pub folder_title_present: usize,
    pub folder_author_present: usize,

    pub title_comparable: usize,
    pub title_agree: usize,
    pub author_comparable: usize,
    pub author_agree: usize,
}

/// A percentage `n / d * 100`, or `0.0` when `d == 0` (no false precision on an
/// empty denominator).
pub fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 / d as f64 * 100.0
    }
}

impl TagProbeReport {
    pub fn title_present_pct(&self) -> f64 {
        pct(self.title_present, self.sampled)
    }
    pub fn author_present_pct(&self) -> f64 {
        pct(self.author_present, self.sampled)
    }
    pub fn album_present_pct(&self) -> f64 {
        pct(self.album_present, self.sampled)
    }
    pub fn narrator_present_pct(&self) -> f64 {
        pct(self.narrator_present, self.sampled)
    }
    pub fn folder_title_present_pct(&self) -> f64 {
        pct(self.folder_title_present, self.sampled)
    }
    pub fn folder_author_present_pct(&self) -> f64 {
        pct(self.folder_author_present, self.sampled)
    }
    /// Agreement rate on title over the files where both a folder title and a
    /// tag title exist.
    pub fn title_agreement_pct(&self) -> f64 {
        pct(self.title_agree, self.title_comparable)
    }
    /// Agreement rate on author over the files where both exist.
    pub fn author_agreement_pct(&self) -> f64 {
        pct(self.author_agree, self.author_comparable)
    }
}

/// Read tags for each sampled entry and aggregate the FD-14 report. READ-ONLY:
/// this only calls [`read_tag_fields`], which never writes.
pub fn run_probe(inputs: &[ProbeInput]) -> TagProbeReport {
    let mut r = TagProbeReport {
        sampled: inputs.len(),
        ..Default::default()
    };

    for input in inputs {
        if input.folder_title.is_some() {
            r.folder_title_present += 1;
        }
        if input.folder_author.is_some() {
            r.folder_author_present += 1;
        }

        let fields = match read_tag_fields(&input.path) {
            TagReadOutcome::Read(f) => {
                r.readable += 1;
                if f.has_any() {
                    r.with_any_tag += 1;
                }
                f
            }
            TagReadOutcome::NoTags => {
                r.readable += 1;
                TagFields::default()
            }
            TagReadOutcome::Unreadable => {
                r.unreadable += 1;
                TagFields::default()
            }
        };

        if fields.title.is_some() {
            r.title_present += 1;
        }
        if fields.author.is_some() {
            r.author_present += 1;
        }
        if fields.album.is_some() {
            r.album_present += 1;
        }
        if fields.narrator.is_some() {
            r.narrator_present += 1;
        }

        if let (Some(ft), Some(tt)) = (&input.folder_title, &fields.title) {
            r.title_comparable += 1;
            if fields_agree(ft, tt) {
                r.title_agree += 1;
            }
        }
        if let (Some(fa), Some(ta)) = (&input.folder_author, &fields.author) {
            r.author_comparable += 1;
            if fields_agree(fa, ta) {
                r.author_agree += 1;
            }
        }
    }

    r
}

/// Normalize a field for a forgiving comparison: lowercase, every run of
/// non-alphanumeric characters collapsed to a single space, trimmed. So
/// `"Project Hail Mary"` and `"project hail mary"` compare equal, and
/// punctuation / separator noise does not cause a spurious disagreement.
fn normalize_for_compare(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true; // trims leading separators
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Whether a folder-derived value and a tag value agree. Exact after
/// normalization, or one meaningfully CONTAINS the other (a tag title
/// `"Project Hail Mary A Novel"` still agrees with a folder title
/// `"Project Hail Mary"`). The containment side requires the shorter string to
/// be at least 4 characters so a stray one-word tag does not match everything.
fn fields_agree(folder: &str, tag: &str) -> bool {
    let a = normalize_for_compare(folder);
    let b = normalize_for_compare(tag);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (shorter, longer) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    shorter.len() >= 4 && longer.contains(shorter.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_takes_all_when_under_cap() {
        assert_eq!(deterministic_sample_indices(3, 300), vec![0, 1, 2]);
        assert_eq!(deterministic_sample_indices(0, 300), Vec::<usize>::new());
        assert_eq!(deterministic_sample_indices(10, 0), Vec::<usize>::new());
    }

    #[test]
    fn sample_strides_and_caps_and_is_repeatable() {
        // 1000 items, cap 300 -> stride 3, first indices 0,3,6,...
        let a = deterministic_sample_indices(1000, 300);
        let b = deterministic_sample_indices(1000, 300);
        assert_eq!(a, b, "deterministic: same inputs, same indices");
        assert!(a.len() <= 300, "never exceeds the cap");
        assert_eq!(&a[..3], &[0, 3, 6]);
        assert!(a.iter().all(|&i| i < 1000), "every index is in range");
    }

    #[test]
    fn agreement_is_case_and_punctuation_insensitive() {
        assert!(fields_agree("Project Hail Mary", "project hail mary"));
        assert!(fields_agree("Atomic Habits", "Atomic  Habits!"));
        // containment (tag carries a subtitle the folder omitted)
        assert!(fields_agree(
            "Project Hail Mary",
            "Project Hail Mary: A Novel"
        ));
        // genuine disagreement
        assert!(!fields_agree("Dune", "Project Hail Mary"));
        // too-short containment does not match everything
        assert!(!fields_agree("It", "Project Hail Mary"));
    }

    #[test]
    fn run_probe_counts_folder_presence_even_for_unreadable_paths() {
        // A path that cannot be read still contributes its folder-side coverage.
        let inputs = vec![ProbeInput {
            path: PathBuf::from("does-not-exist-xyz.m4b"),
            folder_title: Some("Some Title".to_string()),
            folder_author: Some("Some Author".to_string()),
        }];
        let r = run_probe(&inputs);
        assert_eq!(r.sampled, 1);
        assert_eq!(r.unreadable, 1);
        assert_eq!(r.readable, 0);
        assert_eq!(r.folder_title_present, 1);
        assert_eq!(r.folder_author_present, 1);
        assert_eq!(r.title_present, 0);
        assert_eq!(r.title_comparable, 0);
    }
}
