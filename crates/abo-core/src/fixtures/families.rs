//! Fixture family taxonomy: the coverage tags the manifest attaches to every
//! generated node, matching the family list in
//! docs/internal/test-strategy.md Section 3 and the requirements in
//! docs/internal/releases/v0.2.0-understanding/spec.md AC-F2/AC-F3.
//!
//! This is NOT the future `FolderClass` classification taxonomy (F-201,
//! v0.2.0 Phase 5) - that enum does not exist yet and this phase does not
//! invent it (out of scope: no classification, no parsing). A family tag
//! says "this node is the manifest's example of naming pattern 4" or "this
//! node is the Narnia multi-book case"; later phases map family -> expected
//! matcher/classification result in their own golden tests by reading the
//! family tag on [`crate::fixtures::GeneratedEntry`], rather than
//! hand-enumerating paths.

/// One coverage family. [`FixtureFamily::as_str`] is a stable, human-readable
/// label used in coverage checks and test failure messages; it is never
/// persisted to a database or sent across IPC (fixtures never ship at
/// runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixtureFamily {
    /// F-301 pattern 1: `Title by Author.m4b` (loose root files).
    Pattern1TitleByAuthorLooseRoot,
    /// The one loose-root file (of 238 in the real library, FD-18 baseline)
    /// that does not parse cleanly under pattern 1 (AC-301.3): it must fall
    /// through to ambiguous/manual-review, not a wrong parse.
    Pattern1NonCleanOutlier,
    /// F-301 pattern 2: `Author - Title` (genre subfolder).
    Pattern2AuthorDashTitle,
    /// F-301 pattern 3: `Title by Author` folder variant.
    Pattern3TitleByAuthorFolder,
    /// F-301 pattern 4: `Year - Author - Title (Series #N) [noise]`,
    /// including the `^` award-marker variant (Hugo/Nebula).
    Pattern4YearAuthorTitleAward,
    /// F-301 pattern 5: `N - Title - Author - Year` (Top 100 Sci-Fi).
    Pattern5RankTitleAuthorYear,
    /// F-301 pattern 6: `NN.N - Title [noise]` series entries.
    Pattern6SeriesIndexTitle,
    /// F-301 pattern 7: `Author_Name_-_Title` underscored.
    Pattern7Underscored,
    /// F-301 pattern 8: bare `Title`, no author.
    Pattern8BareTitle,
    /// F-301 pattern 9: `Author - Series` containers, including irregular
    /// separators (the `Frank Herbert-Dune-#1-Chronicles[1-8}` case).
    Pattern9IrregularSeriesContainer,
    /// Deep Hugo-style nesting, depth >= 5 below the library root.
    DeepNesting,
    /// A folder holding direct audio files AND child folders.
    Mixed,
    /// The Narnia-style multi-book case: N sibling audio files with distinct
    /// parsed titles, one folder.
    MultiBookNarnia,
    /// The Harry Potter hard case: 11 direct files spanning 7 titles plus
    /// extras.
    MultiBookHarryPotter,
    /// A nonconforming disc folder (Disc/CD/Disk naming variants), the
    /// Verbal Advantage case among them.
    NonconformingDisc,
    /// Parallel-format `0 M4B` case: chapter mp3s plus an m4b sibling.
    ParallelFormatZeroM4b,
    /// One half of an NFC/NFD Unicode twin pair (same visual title, two
    /// distinct byte-level Unicode normalization forms).
    UnicodeNfcNfdTwin,
    /// A near-limit (or over-limit) path length, generated at runtime only,
    /// never committed to git.
    NearLimitPath,
    /// A reserved-device-name near-miss (`CON.mp3`, a folder named `NUL`).
    ReservedNameNearMiss,
    /// A zero-byte placeholder file.
    ZeroByteSample,
    /// One half of an exact-duplicate pair (same basename + size, and in
    /// this generator, identical content).
    ExactDuplicatePair,
    /// An OS-style incremented-suffix copy of a file (the `" (1)"` case):
    /// same visual content as its source but a different basename, so it is
    /// a near-miss for duplicate detection rather than an exact pair.
    SuffixedCopyDuplicate,
    /// A trailing-dot or trailing-space near-miss: a folder or file whose
    /// name literally ends in `.` or a space, only creatable via
    /// extended-length semantics (FD-19), exercising the same normalizer
    /// backstop as [`FixtureFamily::ReservedNameNearMiss`] (F-304).
    TrailingDotOrSpaceNearMiss,
    /// One half of an NFC/NFD Unicode twin pair sharing the SAME parent
    /// directory (as opposed to [`FixtureFamily::UnicodeNfcNfdTwin`], whose
    /// pair lives in separate folders): proves distinct byte-level
    /// normalization forms materialize as distinct directory entries even
    /// when they are siblings.
    UnicodeNfcNfdTwinSameDir,
    /// FD-17 video/course cluster: an mp4 "course" (the Zig Ziglar 52 Sales
    /// Lessons case).
    VideoCourseMp4,
    /// FD-17 video/course cluster: cbr/cbz comic archives.
    VideoCourseComic,
    /// FD-17 video/course cluster: a radio-play folder (non-book audio).
    VideoCourseRadioPlay,
    /// FD-01 pack-provenance shell: the pack container itself.
    PackShell,
    /// FD-01 pack-provenance member: a book folder carrying source-pack
    /// membership (see [`crate::fixtures::manifest::PackProvenance`]).
    PackMember,
}

impl FixtureFamily {
    /// Stable label, used in coverage reports and test failure messages.
    pub fn as_str(self) -> &'static str {
        match self {
            FixtureFamily::Pattern1TitleByAuthorLooseRoot => "pattern-1-title-by-author-loose-root",
            FixtureFamily::Pattern1NonCleanOutlier => "pattern-1-non-clean-outlier",
            FixtureFamily::Pattern2AuthorDashTitle => "pattern-2-author-dash-title",
            FixtureFamily::Pattern3TitleByAuthorFolder => "pattern-3-title-by-author-folder",
            FixtureFamily::Pattern4YearAuthorTitleAward => "pattern-4-year-author-title-award",
            FixtureFamily::Pattern5RankTitleAuthorYear => "pattern-5-rank-title-author-year",
            FixtureFamily::Pattern6SeriesIndexTitle => "pattern-6-series-index-title",
            FixtureFamily::Pattern7Underscored => "pattern-7-underscored",
            FixtureFamily::Pattern8BareTitle => "pattern-8-bare-title",
            FixtureFamily::Pattern9IrregularSeriesContainer => {
                "pattern-9-irregular-series-container"
            }
            FixtureFamily::DeepNesting => "deep-nesting",
            FixtureFamily::Mixed => "mixed",
            FixtureFamily::MultiBookNarnia => "multi-book-narnia",
            FixtureFamily::MultiBookHarryPotter => "multi-book-harry-potter",
            FixtureFamily::NonconformingDisc => "nonconforming-disc",
            FixtureFamily::ParallelFormatZeroM4b => "parallel-format-0-m4b",
            FixtureFamily::UnicodeNfcNfdTwin => "unicode-nfc-nfd-twin",
            FixtureFamily::NearLimitPath => "near-limit-path",
            FixtureFamily::ReservedNameNearMiss => "reserved-name-near-miss",
            FixtureFamily::ZeroByteSample => "zero-byte-sample",
            FixtureFamily::ExactDuplicatePair => "exact-duplicate-pair",
            FixtureFamily::SuffixedCopyDuplicate => "suffixed-copy-duplicate",
            FixtureFamily::TrailingDotOrSpaceNearMiss => "trailing-dot-or-space-near-miss",
            FixtureFamily::UnicodeNfcNfdTwinSameDir => "unicode-nfc-nfd-twin-same-dir",
            FixtureFamily::VideoCourseMp4 => "video-course-mp4",
            FixtureFamily::VideoCourseComic => "video-course-comic",
            FixtureFamily::VideoCourseRadioPlay => "video-course-radio-play",
            FixtureFamily::PackShell => "pack-shell",
            FixtureFamily::PackMember => "pack-member",
        }
    }

    /// Every family the manifest must cover at least once (AC-F2, AC-F3;
    /// test-strategy.md Section 3). The family-coverage test iterates this
    /// list, so a manifest edit that silently drops a family fails loudly
    /// instead of shrinking coverage unnoticed.
    pub const REQUIRED: &'static [FixtureFamily] = &[
        FixtureFamily::Pattern1TitleByAuthorLooseRoot,
        FixtureFamily::Pattern1NonCleanOutlier,
        FixtureFamily::Pattern2AuthorDashTitle,
        FixtureFamily::Pattern3TitleByAuthorFolder,
        FixtureFamily::Pattern4YearAuthorTitleAward,
        FixtureFamily::Pattern5RankTitleAuthorYear,
        FixtureFamily::Pattern6SeriesIndexTitle,
        FixtureFamily::Pattern7Underscored,
        FixtureFamily::Pattern8BareTitle,
        FixtureFamily::Pattern9IrregularSeriesContainer,
        FixtureFamily::DeepNesting,
        FixtureFamily::Mixed,
        FixtureFamily::MultiBookNarnia,
        FixtureFamily::MultiBookHarryPotter,
        FixtureFamily::NonconformingDisc,
        FixtureFamily::ParallelFormatZeroM4b,
        FixtureFamily::UnicodeNfcNfdTwin,
        FixtureFamily::NearLimitPath,
        FixtureFamily::ReservedNameNearMiss,
        FixtureFamily::ZeroByteSample,
        FixtureFamily::ExactDuplicatePair,
        FixtureFamily::SuffixedCopyDuplicate,
        FixtureFamily::TrailingDotOrSpaceNearMiss,
        FixtureFamily::UnicodeNfcNfdTwinSameDir,
        FixtureFamily::VideoCourseMp4,
        FixtureFamily::VideoCourseComic,
        FixtureFamily::VideoCourseRadioPlay,
        FixtureFamily::PackShell,
        FixtureFamily::PackMember,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn required_list_has_no_duplicates_and_every_label_is_stable_kebab() {
        let mut seen = HashSet::new();
        for family in FixtureFamily::REQUIRED {
            assert!(
                seen.insert(*family),
                "REQUIRED lists {:?} more than once",
                family
            );
            let s = family.as_str();
            assert!(!s.is_empty(), "label must be non-empty");
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
                "label must be kebab-case: {s}"
            );
        }
    }
}
