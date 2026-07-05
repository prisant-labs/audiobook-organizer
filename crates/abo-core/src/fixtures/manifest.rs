//! The declarative fixture manifest.
//!
//! A [`FixtureManifest`] is a tree of folders and files; each node may carry
//! zero or more [`FixtureFamily`] tags (which coverage case(s) it
//! exemplifies) and, for pack members, a [`PackProvenance`] annotation
//! (FD-01). [`standard_library_manifest`] is the one manifest this release
//! ships: it is the single source of truth every downstream golden test
//! reads its expectations from (test-strategy.md Section 3), built from real
//! naming examples lifted from
//! `_local/initial-discovery/audio-books-audiobookshelf_claude.md`.
//!
//! Declared file sizes are deliberately KB-scale, not the real library's
//! GB-scale sizes: the ripper-tag / size-marker TEXT baked into a folder name
//! (e.g. `[320k 18;48;03 2.52GB]`) is a naming-pattern fixture (what F-302's
//! noise strippers must remove from the STRING), completely independent of
//! the declared byte SIZE of the file inside (what F-202's health metrics sum
//! on disk). Keeping declared sizes small means the full manifest
//! materializes and regenerates in milliseconds and costs single-digit
//! megabytes of temp disk, which matters because AC-F4's determinism test
//! generates the whole tree twice per run, on every CI job. AC-F5 only
//! requires that whatever size IS declared is honored exactly; it does not
//! require GB-scale fixtures.
//!
//! Names are validated at generation time, not here:
//! [`crate::fixtures::generate::generate`] rejects any component containing a
//! path separator, a `:`, or a `.`/`..` traversal segment, so a manifest typo
//! cannot escape the generator's temp root.

use crate::fixtures::families::FixtureFamily;

/// FD-01 source-pack membership recorded on a member book. `pack_id` is a
/// short stable key used only inside this manifest/test-expectation index
/// (never persisted to a database); `pack_title` is the human-readable
/// collection name a later provenance report would show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackProvenance {
    pub pack_id: &'static str,
    pub pack_title: &'static str,
}

/// A duplicate-group key: two file nodes sharing a `dup_group` are given
/// byte-identical generated content (same content seed) as well as the same
/// declared size, so an "exact-duplicate pair" fixture is exact all the way
/// down, not merely same-size.
pub type DupGroup = &'static str;

/// A folder node: a name, the families it exemplifies, optional pack
/// provenance, and its children in manifest order. Generation does not sort
/// children; determinism comes from the manifest being static data plus
/// deterministic per-file content generation (see
/// [`crate::fixtures::generate`]), not from a run-time sort.
#[derive(Debug, Clone)]
pub struct FolderNode {
    pub name: String,
    pub families: Vec<FixtureFamily>,
    pub provenance: Option<PackProvenance>,
    pub children: Vec<FixtureNode>,
}

/// A file node: a name, a declared size in bytes, the families it
/// exemplifies, and an optional duplicate-group key.
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub size: u64,
    pub families: Vec<FixtureFamily>,
    pub dup_group: Option<DupGroup>,
}

/// One manifest tree node.
#[derive(Debug, Clone)]
pub enum FixtureNode {
    Folder(FolderNode),
    File(FileNode),
}

/// The full manifest. Every top-level node is generated directly under the
/// caller-supplied root (no extra top-level wrapper folder), matching
/// test-strategy.md Section 3's "writes the tree into a caller-supplied
/// root".
#[derive(Debug, Clone)]
pub struct FixtureManifest {
    pub top_level: Vec<FixtureNode>,
}

impl FixtureManifest {
    /// Every family declared anywhere in this manifest, deduplicated. Used by
    /// the family-coverage test (AC-F2, AC-F3) and available to later phases
    /// that want to assert "every X-family fixture also has property Y"
    /// without a hand-typed path list.
    pub fn declared_families(&self) -> std::collections::HashSet<FixtureFamily> {
        let mut set = std::collections::HashSet::new();
        fn walk(node: &FixtureNode, set: &mut std::collections::HashSet<FixtureFamily>) {
            match node {
                FixtureNode::Folder(f) => {
                    set.extend(f.families.iter().copied());
                    for child in &f.children {
                        walk(child, set);
                    }
                }
                FixtureNode::File(file) => {
                    set.extend(file.families.iter().copied());
                }
            }
        }
        for node in &self.top_level {
            walk(node, &mut set);
        }
        set
    }
}

// ---- small builder helpers (keep the manifest body below declarative) ----

fn folder(name: impl Into<String>, children: Vec<FixtureNode>) -> FixtureNode {
    FixtureNode::Folder(FolderNode {
        name: name.into(),
        families: Vec::new(),
        provenance: None,
        children,
    })
}

fn folder_fam(
    name: impl Into<String>,
    families: &[FixtureFamily],
    children: Vec<FixtureNode>,
) -> FixtureNode {
    FixtureNode::Folder(FolderNode {
        name: name.into(),
        families: families.to_vec(),
        provenance: None,
        children,
    })
}

fn folder_pack(
    name: impl Into<String>,
    families: &[FixtureFamily],
    provenance: PackProvenance,
    children: Vec<FixtureNode>,
) -> FixtureNode {
    FixtureNode::Folder(FolderNode {
        name: name.into(),
        families: families.to_vec(),
        provenance: Some(provenance),
        children,
    })
}

fn file(name: impl Into<String>, size: u64) -> FixtureNode {
    FixtureNode::File(FileNode {
        name: name.into(),
        size,
        families: Vec::new(),
        dup_group: None,
    })
}

fn file_fam(name: impl Into<String>, size: u64, families: &[FixtureFamily]) -> FixtureNode {
    FixtureNode::File(FileNode {
        name: name.into(),
        size,
        families: families.to_vec(),
        dup_group: None,
    })
}

fn file_dup(
    name: impl Into<String>,
    size: u64,
    families: &[FixtureFamily],
    dup_group: DupGroup,
) -> FixtureNode {
    FixtureNode::File(FileNode {
        name: name.into(),
        size,
        families: families.to_vec(),
        dup_group: Some(dup_group),
    })
}

/// Build the near-limit path chain (AC-F2): nested folders whose combined
/// path length, added to any real temp-root prefix, reliably exceeds the
/// legacy Windows `MAX_PATH` (260 chars), proving the generator needs (and
/// exercises) extended-length semantics (FD-19,
/// [`crate::paths::to_extended_length`]). Generated at runtime only, per the
/// hard rule in test-strategy.md Section 3: this subtree must never be
/// committed, so a Windows checkout never breaks and no `core.longpaths` git
/// config is ever needed.
fn near_limit_chain() -> FixtureNode {
    const SEGMENT: &str = "near-limit-path-segment-of-forty-chars-x"; // 40 ASCII chars
    debug_assert_eq!(SEGMENT.chars().count(), 40);
    let mut node = file_fam("audio.m4b", 1_024, &[FixtureFamily::NearLimitPath]);
    // 7 nested ~41-char segments (plus separators) comfortably pushes the
    // subtree past 260 chars on its own, before any temp-root prefix is even
    // added.
    for i in (0..7).rev() {
        node = folder_fam(
            format!("{SEGMENT}{i:02}"),
            &[FixtureFamily::NearLimitPath],
            vec![node],
        );
    }
    node
}

/// Build the reserved-device-name near-miss fixtures (AC-F2): a file whose
/// base name is a reserved DOS device name plus an extension, and a folder
/// literally named after a reserved device name. Both are legal to create
/// only via extended-length (`\\?\`) semantics on Windows; the generator
/// attempts them with that seam and records a skip-with-note if the host
/// still refuses (test-strategy.md Section 3 edge case).
fn reserved_name_examples() -> Vec<FixtureNode> {
    vec![
        file_fam("CON.mp3", 512, &[FixtureFamily::ReservedNameNearMiss]),
        file_fam("COM1.m4b", 512, &[FixtureFamily::ReservedNameNearMiss]),
        folder_fam(
            "NUL",
            &[FixtureFamily::ReservedNameNearMiss],
            vec![file("inside.mp3", 512)],
        ),
    ]
}

/// Build the trailing-dot and trailing-space near-miss fixtures (F-304's
/// normalizer backstop): a folder whose name literally ends in `.` and a
/// file whose name literally ends in a space. Windows silently strips a
/// trailing dot or space from a component name unless the generator uses
/// extended-length (`\\?\`) semantics, the same seam
/// [`reserved_name_examples`] relies on; the generator attempts both
/// through that seam and records a skip-with-note if the host still
/// refuses (test-strategy.md Section 3 edge case).
fn trailing_dot_or_space_examples() -> Vec<FixtureNode> {
    vec![
        folder_fam(
            "Some Book.",
            &[FixtureFamily::TrailingDotOrSpaceNearMiss],
            vec![file("inside.mp3", 512)],
        ),
        file_fam(
            "Some Book ",
            512,
            &[FixtureFamily::TrailingDotOrSpaceNearMiss],
        ),
    ]
}

/// The standard fixture library manifest (v0.2.0 Phase 1). Covers every
/// family in [`FixtureFamily::REQUIRED`] at least once, using real example
/// strings lifted from
/// `_local/initial-discovery/audio-books-audiobookshelf_claude.md` Section 4
/// (naming patterns), Section 5 (inconsistencies), Section 6 (series), and
/// Section 7 (collections).
pub fn standard_library_manifest() -> FixtureManifest {
    let hugo_pack = PackProvenance {
        pack_id: "hugo-2014-2024",
        pack_title: "Hugo Best Novel Nominees (2014-2024)",
    };

    FixtureManifest {
        top_level: vec![
            // ---- Pattern 1: loose root `Title by Author.m4b` ----
            file_fam(
                "Sapiens by Yuval Noah Harari.m4b",
                180_000,
                &[FixtureFamily::Pattern1TitleByAuthorLooseRoot],
            ),
            file_fam(
                "Atomic Habits by James Clear.m4b",
                140_000,
                &[FixtureFamily::Pattern1TitleByAuthorLooseRoot],
            ),
            file_fam(
                "The Phoenix Project by Gene Kim.m4b",
                150_000,
                &[FixtureFamily::Pattern1TitleByAuthorLooseRoot],
            ),
            // The 1-of-238 non-clean outlier (AC-301.3): a "FREE SAMPLE_"
            // prefix defeats the clean `Title by Author` parse.
            file_fam(
                "FREE SAMPLE_ The Martian by Andy Weir.m4b",
                2_700,
                &[FixtureFamily::Pattern1NonCleanOutlier],
            ),
            // Exact-duplicate pair: the family is defined as same basename
            // AND same size, so the canonical pair must share one literal
            // filename. The second copy lives in a different folder (a
            // genre subfolder below) rather than the same one, but keeps
            // the identical `dup_group` content seed so the pair is exact
            // all the way down (byte-identical), not merely same-name.
            file_dup(
                "Island by Aldous Huxley.m4b",
                120_000,
                &[FixtureFamily::ExactDuplicatePair],
                "island-huxley",
            ),
            // A same-folder OS-style incremented-suffix copy (the `" (1)"`
            // case): visually the same book but a DIFFERENT basename, so it
            // is deliberately NOT part of the exact-duplicate-pair family
            // (that family requires basename equality). Tagged instead as
            // its own suffixed-copy-duplicate family, and given no
            // dup_group of its own (its content is the ordinary path-seeded
            // placeholder, not a claim of byte-identity with anything).
            file_fam(
                "Island by Aldous Huxley (1).m4b",
                120_000,
                &[FixtureFamily::SuffixedCopyDuplicate],
            ),
            // ---- Genre - SciFI ----
            folder(
                "Genre - SciFI",
                vec![
                    // Pattern 2: `Author - Title`.
                    folder_fam(
                        "Jim Butcher - Dresden Files 22GB",
                        &[FixtureFamily::Pattern2AuthorDashTitle],
                        vec![file("Jim Butcher - Dresden Files 22GB.m4b", 210_000)],
                    ),
                    folder_fam(
                        "Andy Weir - Project Hail Mary",
                        &[FixtureFamily::Pattern2AuthorDashTitle],
                        vec![file("Andy Weir - Project Hail Mary.m4b", 175_000)],
                    ),
                    // Pattern 8: bare `Title`.
                    folder_fam(
                        "Out of the Silent Planet",
                        &[FixtureFamily::Pattern8BareTitle],
                        vec![file("Out of the Silent Planet.m4b", 95_000)],
                    ),
                    // Pattern 9: irregular-separator series container.
                    folder_fam(
                        "Frank Herbert-Dune-#1-Chronicles[1-8}",
                        &[FixtureFamily::Pattern9IrregularSeriesContainer],
                        vec![
                            file("Book 1 - Dune.m4b", 200_000),
                            file("Book 2 - Dune Messiah.m4b", 130_000),
                        ],
                    ),
                    // Top 100 Sci-Fi Books: Pattern 5.
                    folder(
                        "Top 100 Sci-Fi Books",
                        vec![
                            folder_fam(
                                "1 - Ender's Game - Orson Scott Card - 1985",
                                &[FixtureFamily::Pattern5RankTitleAuthorYear],
                                vec![file(
                                    "1 - Ender's Game - Orson Scott Card - 1985.m4b",
                                    98_000,
                                )],
                            ),
                            folder_fam(
                                "16 - Brave New World - Aldous Huxley - 1932",
                                &[FixtureFamily::Pattern5RankTitleAuthorYear],
                                vec![file(
                                    "16 - Brave New World - Aldous Huxley - 1932.m4b",
                                    99_000,
                                )],
                            ),
                        ],
                    ),
                    // Hugo Collection: FD-01 pack shell + deep nesting (>=5)
                    // + patterns 4 & 6.
                    folder_pack(
                        "Hugo Collection",
                        &[FixtureFamily::PackShell],
                        hugo_pack,
                        vec![
                            folder_pack(
                                "2016^ - N.K. Jemisin - The Fifth Season (The Broken Earth Book 1) [64k 15;27;29 426MB]",
                                &[FixtureFamily::Pattern4YearAuthorTitleAward, FixtureFamily::PackMember],
                                hugo_pack,
                                vec![file("The Fifth Season.m4b", 187_000)],
                            ),
                            folder_pack(
                                "2014 - Charles Stross - Neptune's Brood (Freyaverse #2) [64k 12;00;34 660MB]",
                                &[FixtureFamily::Pattern4YearAuthorTitleAward, FixtureFamily::PackMember],
                                hugo_pack,
                                vec![file("Neptune's Brood.m4b", 165_000)],
                            ),
                            // "0 Series Books": the real library's
                            // collection-within-a-collection. Depth below
                            // root here is 1 (SciFI) + 1 (Hugo) + 1 (this) +
                            // 1 (series folder) + 1 (series-entry folder) = 5.
                            folder(
                                "0 Series Books",
                                vec![
                                    folder_pack(
                                        "Jim Butcher - Dresden Files 22GB",
                                        &[FixtureFamily::PackMember],
                                        hugo_pack,
                                        vec![
                                            folder_fam(
                                                "14.0 - Cold Days [320k 18;48;03 2.52GB]",
                                                &[FixtureFamily::Pattern6SeriesIndexTitle, FixtureFamily::DeepNesting],
                                                vec![file("14.0 - Cold Days.m4b", 210_000)],
                                            ),
                                            folder_fam(
                                                "08.0 - Proven Guilty [320k 16;15;24 2.19GB]",
                                                &[FixtureFamily::Pattern6SeriesIndexTitle, FixtureFamily::DeepNesting],
                                                vec![file("08.0 - Proven Guilty.m4b", 182_000)],
                                            ),
                                        ],
                                    ),
                                    folder(
                                        "Robert Jordan - Wheel of Time",
                                        vec![folder_fam(
                                            "(1) The Eye of the World [64k 29;57;14 826MB]",
                                            &[FixtureFamily::Pattern6SeriesIndexTitle, FixtureFamily::DeepNesting],
                                            vec![file("The Eye of the World.m4b", 220_000)],
                                        )],
                                    ),
                                ],
                            ),
                            // FD-31: pack-internal clutter. An .nfo sitting
                            // directly in the collection bundle's shell (not in
                            // any member) is set aside as part of unpacking, so
                            // its set-aside op rides the Bundles group.
                            file("release.nfo", 500),
                        ],
                    ),
                    // Parallel-format `0 M4B`: chapter mp3s plus an m4b
                    // sibling in a child folder.
                    folder(
                        "Parallel Format Example",
                        vec![
                            file_fam("track01.mp3", 40_000, &[FixtureFamily::ParallelFormatZeroM4b]),
                            file_fam("track02.mp3", 40_000, &[FixtureFamily::ParallelFormatZeroM4b]),
                            folder_fam(
                                "0 M4B",
                                &[FixtureFamily::ParallelFormatZeroM4b],
                                vec![file("Parallel Format Example.m4b", 80_000)],
                            ),
                        ],
                    ),
                    // The exact-duplicate pair's second copy (same basename
                    // AND size as the loose-root copy above, plus the same
                    // `dup_group` seed for byte-identical content), placed
                    // in a different folder so the pair is a genuine
                    // cross-folder duplicate, not a same-folder rename.
                    folder_fam(
                        "Backup Copy",
                        &[],
                        vec![file_dup(
                            "Island by Aldous Huxley.m4b",
                            120_000,
                            &[FixtureFamily::ExactDuplicatePair],
                            "island-huxley",
                        )],
                    ),
                    // Mixed folder: direct audio plus a child folder.
                    folder_fam(
                        "Mixed Example Folder",
                        &[FixtureFamily::Mixed],
                        vec![
                            file("loose-track.mp3", 30_000),
                            folder("Nested Book", vec![file("Nested Book.m4b", 150_000)]),
                        ],
                    ),
                ],
            ),
            // ---- Genre - Non-Fiction ----
            folder(
                "Genre - Non-Fiction",
                vec![
                    // Pattern 3: `Title by Author` folder variant.
                    folder_fam(
                        "How to Think About AI - A Guide for the Perplexe by Richard Susskind",
                        &[FixtureFamily::Pattern3TitleByAuthorFolder],
                        vec![file("How to Think About AI.m4b", 160_000)],
                    ),
                    folder_fam(
                        "Every Tool's a Hammer - Life Is What You Make It by Adam Savage",
                        &[FixtureFamily::Pattern3TitleByAuthorFolder],
                        vec![file("Every Tool's a Hammer.m4b", 170_000)],
                    ),
                    folder_fam(
                        "Rework",
                        &[FixtureFamily::Pattern8BareTitle],
                        vec![file("Rework.m4b", 90_000)],
                    ),
                    // Nonconforming disc folders (the Verbal Advantage case),
                    // Disc/CD/Disk spelling variants.
                    folder_fam(
                        "Verbal Advantage",
                        &[FixtureFamily::NonconformingDisc],
                        vec![
                            folder("Disc 1", vec![file("track01.mp3", 45_000), file("track02.mp3", 45_000)]),
                            folder("Disc 2", vec![file("track01.mp3", 45_000)]),
                            folder_fam("CD3", &[FixtureFamily::NonconformingDisc], vec![file("track01.mp3", 45_000)]),
                            folder_fam("Disk_04", &[FixtureFamily::NonconformingDisc], vec![file("track01.mp3", 45_000)]),
                        ],
                    ),
                ],
            ),
            // ---- Genre - Business ----
            folder(
                "Genre - Business",
                vec![
                    // Pattern 7: underscored.
                    folder_fam(
                        "Chris_Dixon_-_Read_Write_Own",
                        &[FixtureFamily::Pattern7Underscored],
                        vec![file("Chris_Dixon_-_Read_Write_Own.m4b", 120_000)],
                    ),
                    folder_fam(
                        "Michael_Easter_-_Scarcity_Brain",
                        &[FixtureFamily::Pattern7Underscored],
                        vec![file("Michael_Easter_-_Scarcity_Brain.m4b", 125_000)],
                    ),
                ],
            ),
            // ---- Genre - Children ----
            folder(
                "Genre - Children",
                vec![
                    // Multi-book Narnia: 7 sibling files, distinct titles.
                    folder_fam(
                        "Chronicles of Narnia",
                        &[FixtureFamily::MultiBookNarnia],
                        vec![
                            file("The Lion, the Witch and the Wardrobe.m4b", 90_000),
                            file("Prince Caspian.m4b", 75_000),
                            file("The Voyage of the Dawn Treader.m4b", 80_000),
                            file("The Silver Chair.m4b", 78_000),
                            file("The Horse and His Boy.m4b", 70_000),
                            file("The Magician's Nephew.m4b", 72_000),
                            file("The Last Battle.m4b", 76_000),
                        ],
                    ),
                    // Multi-book Harry Potter: 11 direct files spanning 7
                    // titles plus 2 extras (the hard case, AC-203.2).
                    folder_fam(
                        "Harry Potter",
                        &[FixtureFamily::MultiBookHarryPotter],
                        vec![
                            file("Harry Potter and the Philosopher's Stone.mp3", 100_000),
                            file("Harry Potter and the Chamber of Secrets.mp3", 105_000),
                            file("Harry Potter and the Prisoner of Azkaban.mp3", 110_000),
                            file("Harry Potter and the Goblet of Fire - Part 1.mp3", 130_000),
                            file("Harry Potter and the Goblet of Fire - Part 2.mp3", 130_000),
                            file("Harry Potter and the Order of the Phoenix.mp3", 150_000),
                            file("Harry Potter and the Half-Blood Prince.mp3", 140_000),
                            file("Harry Potter and the Deathly Hallows - Part 1.mp3", 120_000),
                            file("Harry Potter and the Deathly Hallows - Part 2.mp3", 120_000),
                            file("Extra - JK Rowling Interview.mp3", 20_000),
                            file("Extra - Pottermore Bonus Chapter.mp3", 15_000),
                        ],
                    ),
                    folder_fam(
                        "Roald Dahl - Charlie and the Chocolate Factory",
                        &[FixtureFamily::Pattern9IrregularSeriesContainer],
                        vec![file("Charlie and the Chocolate Factory.m4b", 60_000)],
                    ),
                    // Multi-book Wings of Fire: 5 sibling files, one series,
                    // distinguished PRIMARILY by a leading series index. Real
                    // per-book titles still differ (F-301 pattern 2 splits
                    // them off after the dash), so this is the case where
                    // title-text counting already works; it exists alongside
                    // the F-203 unit tests that separately prove the
                    // distinct-(title, series index) heuristic also survives
                    // a naming style where the title text alone would
                    // collapse (see crate::classify::multibook module doc).
                    folder_fam(
                        "Wings of Fire",
                        &[FixtureFamily::MultiBookWingsOfFire],
                        vec![
                            file("Wings of Fire 01 - The Dragonet Prophecy.mp3", 90_000),
                            file("Wings of Fire 02 - The Lost Heir.mp3", 88_000),
                            file("Wings of Fire 03 - The Hidden Kingdom.mp3", 85_000),
                            file("Wings of Fire 04 - The Dark Secret.mp3", 82_000),
                            file("Wings of Fire 05 - The Brightest Night.mp3", 91_000),
                        ],
                    ),
                ],
            ),
            // ---- Unicode NFC/NFD twin pair ----
            folder(
                "Unicode Twins",
                vec![
                    folder_fam(
                        "Caf\u{00e9} Society (NFC)",
                        &[FixtureFamily::UnicodeNfcNfdTwin],
                        vec![file("Caf\u{00e9} Society.m4b", 100_000)],
                    ),
                    folder_fam(
                        "Cafe\u{0301} Society (NFD)",
                        &[FixtureFamily::UnicodeNfcNfdTwin],
                        vec![file("Cafe\u{0301} Society.m4b", 100_000)],
                    ),
                ],
            ),
            // ---- Unicode NFC/NFD twins sharing ONE parent directory, in
            // addition to the separate-folder pair above: same visual name,
            // two distinct Unicode normalization forms, both siblings under
            // this folder. The generator verifies post-write that both
            // materialize as distinct directory entries and demotes both to
            // skipped-with-note if this host's filesystem folds them
            // together.
            folder(
                "Unicode Twins Same Dir",
                vec![
                    file_fam(
                        "Caf\u{00e9} Society.m4b",
                        100_000,
                        &[FixtureFamily::UnicodeNfcNfdTwinSameDir],
                    ),
                    file_fam(
                        "Cafe\u{0301} Society.m4b",
                        100_000,
                        &[FixtureFamily::UnicodeNfcNfdTwinSameDir],
                    ),
                ],
            ),
            // ---- Near-limit path (runtime-generated only, AC-F2) ----
            folder("Near Limit", vec![near_limit_chain()]),
            // ---- Reserved-name near-misses (AC-F2) ----
            folder("Reserved Names", reserved_name_examples()),
            // ---- Trailing-dot / trailing-space near-misses (F-304) ----
            folder("Trailing Dot Or Space", trailing_dot_or_space_examples()),
            // ---- Zero-byte samples (AC-F2) ----
            folder(
                "Zero Byte Samples",
                vec![
                    file_fam("Empty Track.mp3", 0, &[FixtureFamily::ZeroByteSample]),
                    file_fam("placeholder.m4b", 0, &[FixtureFamily::ZeroByteSample]),
                ],
            ),
            // ---- FD-17 video/course cluster ----
            folder(
                "Video Course Cluster",
                vec![
                    folder_fam(
                        "52 Sales Lessons",
                        &[FixtureFamily::VideoCourseMp4],
                        vec![
                            file("Lesson 01.mp4", 45_000),
                            file("Lesson 02.mp4", 45_000),
                            file("Lesson 03.mp4", 45_000),
                            file("Lesson 04.mp4", 45_000),
                            file("Lesson 05.mp4", 45_000),
                        ],
                    ),
                    folder_fam(
                        "Comics",
                        &[FixtureFamily::VideoCourseComic],
                        vec![file("Issue 01.cbz", 30_000), file("Issue 02.cbr", 32_000)],
                    ),
                    folder_fam(
                        "Radio Play",
                        &[FixtureFamily::VideoCourseRadioPlay],
                        vec![
                            file("The War of the Worlds Radio Play - Part 1.mp3", 60_000),
                            file("The War of the Worlds Radio Play - Part 2.mp3", 60_000),
                        ],
                    ),
                ],
            ),
            // ---- Golden-path completeness (P5 concern 2): the three
            // remaining F-201 leaf classes the manifest otherwise never
            // exercises directly (staging, docs-resources, empty). ----
            //
            // A folder with no files anywhere beneath it: no children at
            // all, so F-201 rule 5 places it `empty` (AC-201.6).
            folder_fam("Empty Folder", &[FixtureFamily::EmptyFolder], vec![]),
            // A folder holding only non-audio content (a pdf and a txt),
            // no audio anywhere beneath: F-201 rule 5 places it
            // `docs-resources`.
            folder_fam(
                "Docs And Resources",
                &[FixtureFamily::DocsResourcesOnly],
                vec![
                    file("Owner's Manual.pdf", 2_000),
                    file("read me.txt", 500),
                ],
            ),
            // A staging area: F-201 rule 1 routes a `_sort`-named folder to
            // `staging` by name alone, regardless of the loose audio files
            // sitting directly inside it.
            folder_fam(
                "_sort",
                &[FixtureFamily::StagingArea],
                vec![
                    file("New Audiobook 1.mp3", 50_000),
                    file("New Audiobook 2.mp3", 50_000),
                ],
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_manifest_declares_every_required_family() {
        let manifest = standard_library_manifest();
        let declared = manifest.declared_families();
        let missing: Vec<&str> = FixtureFamily::REQUIRED
            .iter()
            .filter(|f| !declared.contains(f))
            .map(|f| f.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "manifest is missing required families: {missing:?}"
        );
    }

    #[test]
    fn duplicate_pair_files_share_size_and_dup_group() {
        // The exact-duplicate-pair family (AC-F2) requires same basename+size;
        // this generator additionally gives them identical content via the
        // shared dup_group seed. Assert the manifest actually wires that up,
        // including basename equality for groups whose members are all
        // tagged ExactDuplicatePair (a non-exact dup_group use, if one is
        // ever added, would not be held to the basename rule).
        let manifest = standard_library_manifest();
        let mut records = Vec::new();
        fn walk(node: &FixtureNode, out: &mut Vec<(DupGroup, u64, String, Vec<FixtureFamily>)>) {
            match node {
                FixtureNode::Folder(f) => {
                    for c in &f.children {
                        walk(c, out);
                    }
                }
                FixtureNode::File(file) => {
                    if let Some(g) = file.dup_group {
                        out.push((g, file.size, file.name.clone(), file.families.clone()));
                    }
                }
            }
        }
        for node in &manifest.top_level {
            walk(node, &mut records);
        }
        assert!(!records.is_empty(), "expected at least one dup_group pair");

        // group by key and assert every group has >= 2 members, all same size
        use std::collections::HashMap;
        let mut groups: HashMap<DupGroup, Vec<(u64, String, Vec<FixtureFamily>)>> = HashMap::new();
        for (g, size, name, families) in records {
            groups.entry(g).or_default().push((size, name, families));
        }
        for (g, members) in groups {
            assert!(members.len() >= 2, "dup group {g} has fewer than 2 members");
            assert!(
                members.windows(2).all(|w| w[0].0 == w[1].0),
                "dup group {g} members must share the same declared size"
            );
            let is_exact_pair = members
                .iter()
                .all(|(_, _, fams)| fams.contains(&FixtureFamily::ExactDuplicatePair));
            if is_exact_pair {
                assert!(
                    members.windows(2).all(|w| w[0].1 == w[1].1),
                    "exact-duplicate-pair dup group {g} members must share the same basename"
                );
            }
        }
    }

    #[test]
    fn suffixed_copy_is_not_tagged_as_the_exact_pair() {
        // FIX 1 regression guard: the " (1)" same-folder copy must be a
        // distinct family from ExactDuplicatePair, since it deliberately has
        // a DIFFERENT basename than its source.
        let manifest = standard_library_manifest();
        fn walk(node: &FixtureNode, out: &mut Vec<FileNode>) {
            match node {
                FixtureNode::Folder(f) => {
                    for c in &f.children {
                        walk(c, out);
                    }
                }
                FixtureNode::File(file) => out.push(file.clone()),
            }
        }
        let mut files = Vec::new();
        for node in &manifest.top_level {
            walk(node, &mut files);
        }
        let suffixed = files
            .iter()
            .find(|f| f.families.contains(&FixtureFamily::SuffixedCopyDuplicate))
            .expect("expected a SuffixedCopyDuplicate fixture");
        assert!(
            !suffixed
                .families
                .contains(&FixtureFamily::ExactDuplicatePair),
            "the suffixed-copy fixture must not also claim to be the exact pair"
        );
        assert!(
            suffixed.name.ends_with(" (1).m4b"),
            "expected the suffixed-copy fixture to be the \" (1)\" case, got {:?}",
            suffixed.name
        );
    }
}
