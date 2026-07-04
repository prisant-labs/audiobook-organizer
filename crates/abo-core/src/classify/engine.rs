//! F-201 (folder classification engine): assign exactly one [`FolderClass`] to
//! every folder, deterministically and bottom-up (children before parents),
//! recording a rule id plus evidence for each decision so the later UI can
//! explain itself (F-504, v0.4.0).
//!
//! Like the rest of `crate::classify`, this module is pure logic: no I/O, no
//! filesystem, nothing `cfg`-gated (the CFG RULE). Its input is a flat slice of
//! [`ClassifyInput`] (id, parent, name, kind, and - for files - the F-103
//! [`FileClass`]); its output is one [`FolderClassification`] per FOLDER entry,
//! in input order. Files are not classified (only folders carry a class); a
//! file's bytes are attributed to its containing folder's class by
//! [`crate::classify::metrics`].
//!
//! # The nine classes and the order rules are checked
//!
//! Every folder becomes one of: `book`, `series-container`, `pack-container`,
//! `staging`, `mixed`, `multi-book-suspect`, `empty`, `docs-resources`,
//! `manual-review`. Rules are evaluated per folder in a fixed order (the first
//! that fires wins), which is what makes the outcome deterministic:
//!
//! 1. **staging** - the folder name is a staging area (`_sort`, `_process`).
//! 2. **FD-17 non-book media** - direct files are dominated by video (`52 Sales
//!    Lessons`), comics (cbr/cbz), or the folder is a radio play; these route
//!    to `manual-review` and are never auto-planned (AC-201.3, gate G-06).
//! 3. **direct audio present** - with child folders it is `mixed`; as a leaf it
//!    is `book`, unless F-203 finds several distinct book titles, making it a
//!    `multi-book-suspect`.
//! 4. **no direct audio, with child folders** - a disc-split single book (all
//!    children are disc parts) folds to one `book`; a folder of book-like child
//!    folders is a `series-container` or `pack-container` (see below); a cluster
//!    of only non-book (`manual-review`) children is `manual-review`; otherwise
//!    `docs-resources` (non-audio content beneath) or `empty` (nothing beneath).
//! 5. **no direct audio, no child folders** - `docs-resources` if it holds
//!    non-audio content, else `empty` (AC-201.6).
//!
//! A folder no rule can place is `manual-review`, a first-class outcome, never
//! an error (AC-201.4).
//!
//! # series-container vs pack-container (the author-container distinction)
//!
//! A container whose book-like children cohere into ONE author/series is a
//! `series-container` (a real author container: its parsed author/series SHOULD
//! flow down to its books). A container that groups DISTINCT books - a genre
//! shelf (`Genre - SciFI`), a source pack (`Hugo Collection`), a themed list
//! (`Top 100 Sci-Fi Books`) - is a `pack-container` (a non-author container:
//! its own parsed "author" is spurious, e.g. `Genre - SciFI` parses author
//! `Genre` via F-301 pattern 2). The decision uses only structural + F-303
//! signals: children carrying a series index or a shared series (one series),
//! versus multiple distinct child authors (a collection). See
//! [`decide_container`].
//!
//! # Consuming P4: shelf-inheritance suppression (Field::inherited_from)
//!
//! F-303 inherits author/series down the tree by NAME alone; it cannot tell a
//! real author container from an organizational shelf, so a book under `Genre -
//! SciFI` inherits author `Genre` at medium. This engine is where that is
//! fixed: after every folder is classified, a book's effective author/series
//! ([`Evidence::clean_author`] / [`Evidence::clean_series`]) DROPS any inherited
//! field whose recorded `inherited_from` ancestor this engine classified as a
//! non-author container (`pack-container` or `staging`). Own-name (high) values
//! and values inherited from a real `series-container` are kept. This uses the
//! provenance F-303 already recorded on each field, with no re-walk.
//!
//! # Consuming P4: the pattern-2 vs pattern-9 ambiguity
//!
//! A pattern-9-shaped name like `Frank Herbert-Dune-#1-Chronicles[1-8}` parses
//! as a series container by NAME, but whether it IS one is a content question
//! this engine owns (brief note 2): audio directly inside makes it a book
//! candidate (then F-203 decides book vs multi-book by its files' titles); only
//! book-CHILD folders make it a series/pack container. So the fixture's
//! `Frank Herbert-Dune...` (two distinct-title files inside) is a
//! `multi-book-suspect`, and `Roald Dahl - Charlie...` (one file inside) is a
//! `book` - neither is a container, decided by content, not by the name shape.

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use regex::Regex;

use crate::classify::multibook::{detect_multibook, is_disc_folder_name, BookFile};
use crate::parse::extract::{extract, EntryInput, FieldSource, MergedEntry, NodeKind};
use crate::scan::typing::FileClass;

/// The nine F-201 folder classes (the Codex taxonomy plus `docs-resources`).
/// [`FolderClass::as_str`] is the stable kebab-case string used in evidence,
/// snapshots, and (later) IPC; serialization uses the same string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FolderClass {
    /// One book's audio (leaf folder of audio, or a disc-split single book).
    Book,
    /// A folder whose book-like children cohere into one author/series.
    SeriesContainer,
    /// A collection of distinct books: a genre shelf, source pack, or list.
    PackContainer,
    /// A staging area (`_sort`, `_process`).
    Staging,
    /// Direct audio files AND child folders together.
    Mixed,
    /// Several complete books sitting as sibling audio files (F-203).
    MultiBookSuspect,
    /// No audio anywhere beneath and no non-audio content either.
    Empty,
    /// Non-audio content only (ebooks / images / release info), no audio.
    DocsResources,
    /// Not auto-classifiable, or FD-17 non-book media (video / comics / radio
    /// play). A first-class outcome, never an error.
    ManualReview,
}

impl FolderClass {
    /// Stable kebab-case string (contract for evidence, snapshots, IPC).
    pub fn as_str(self) -> &'static str {
        match self {
            FolderClass::Book => "book",
            FolderClass::SeriesContainer => "series-container",
            FolderClass::PackContainer => "pack-container",
            FolderClass::Staging => "staging",
            FolderClass::Mixed => "mixed",
            FolderClass::MultiBookSuspect => "multi-book-suspect",
            FolderClass::Empty => "empty",
            FolderClass::DocsResources => "docs-resources",
            FolderClass::ManualReview => "manual-review",
        }
    }

    /// Every class, for exhaustive iteration in metrics and tests.
    pub const ALL: &'static [FolderClass] = &[
        FolderClass::Book,
        FolderClass::SeriesContainer,
        FolderClass::PackContainer,
        FolderClass::Staging,
        FolderClass::Mixed,
        FolderClass::MultiBookSuspect,
        FolderClass::Empty,
        FolderClass::DocsResources,
        FolderClass::ManualReview,
    ];
}

impl serde::Serialize for FolderClass {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// One entry the classifier operates over: a stable `id`, its containing
/// folder's id (`parent`, [`None`] for a top-level entry), the raw single-
/// component `name`, its `kind`, the F-103 [`FileClass`] for a file ([`None`]
/// for a folder), and the declared `size` in bytes (0 for folders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifyInput {
    pub id: usize,
    pub parent: Option<usize>,
    pub name: String,
    pub kind: NodeKind,
    pub file_class: Option<FileClass>,
    pub size: u64,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

/// The evidence for one classification: a structured, JSON-serializable record
/// of the signals a rule keyed on. Fields are omitted from JSON when empty /
/// zero / absent, so each rule's evidence shows only what it used. This is the
/// F-504 explainability payload; string lists are sorted for a deterministic
/// surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Evidence {
    /// A short semantic tag for a rule that keys on content kind
    /// (`video-course`, `comic`, `radio-play`, `disc-split`, `genre-shelf`,
    /// `non-book-cluster`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<&'static str>,
    #[serde(skip_serializing_if = "is_zero")]
    pub audio_files: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub video_files: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub comic_files: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub non_audio_files: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub child_folders: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub book_children: u64,
    /// The distinct book titles F-203 counted (multi-book-suspect only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub distinct_titles: Vec<String>,
    /// How many book-like children carried a series index or series field
    /// (container decisions).
    #[serde(skip_serializing_if = "is_zero")]
    pub series_signal: u64,
    /// The distinct own-name authors among book-like children (container
    /// decisions).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub distinct_child_authors: Vec<String>,
    /// The container's own parsed author, when it read one from its own name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own_author: Option<String>,
    /// The container's own parsed series, when it read one from its own name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own_series: Option<String>,
    /// A book's effective author AFTER shelf-inheritance suppression (own-name
    /// or real-series-container-inherited only; a shelf-derived inherited
    /// author is dropped). [`None`] when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean_author: Option<String>,
    /// A book's effective series after the same suppression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clean_series: Option<String>,
}

/// One folder's classification: its id, class, the rule id that placed it, and
/// the evidence for that decision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FolderClassification {
    pub id: usize,
    pub class: FolderClass,
    pub rule_id: &'static str,
    pub evidence: Evidence,
}

// ---- name heuristics (pure) ----

static STAGING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^_?(?:sort|process|processing|incoming|inbox|staging|unsorted|new|to[\s_-]?sort|to[\s_-]?process)$")
        .expect("valid staging regex")
});

/// Whether a folder name is a staging area (`_sort`, `_process`, `staging`).
fn is_staging_name(name: &str) -> bool {
    STAGING_RE.is_match(name.trim())
}

/// Genre / shelf stopwords: an own-name "author" equal to one of these is an
/// organizational label, not a real author, so it must not qualify a container
/// as a real author (series) container.
const GENRE_WORDS: &[&str] = &[
    "genre",
    "fiction",
    "nonfiction",
    "non-fiction",
    "sci-fi",
    "scifi",
    "science fiction",
    "fantasy",
    "mystery",
    "romance",
    "thriller",
    "horror",
    "children",
    "childrens",
    "kids",
    "young adult",
    "business",
    "history",
    "biography",
    "self-help",
    "selfhelp",
    "audiobooks",
    "books",
    "misc",
    "unsorted",
];

/// Whether a container is an organizational shelf (a `Genre - X` name, or an
/// own-name author that is a genre stopword) rather than a real author
/// container.
fn is_genre_shelf(name: &str, own_author: Option<&str>) -> bool {
    let nl = name.to_lowercase();
    if nl == "genre" || nl.starts_with("genre ") || nl.starts_with("genre-") {
        return true;
    }
    if let Some(a) = own_author {
        if GENRE_WORDS.contains(&a.trim().to_lowercase().as_str()) {
            return true;
        }
    }
    false
}

/// Classify every folder in `entries`, bottom-up and deterministically.
/// Returns one [`FolderClassification`] per FOLDER entry, in input order.
///
/// This is the pure function; production callers should go through
/// [`crate::classify::run::run_classify`] instead, which wraps this same
/// logic with the `db::activity` audit row (F-1001). Calling `classify`
/// directly skips that audit trail.
pub fn classify(entries: &[ClassifyInput]) -> Vec<FolderClassification> {
    let n = entries.len();

    let index_of: HashMap<usize, usize> =
        entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let parent_pos: Vec<Option<usize>> = entries
        .iter()
        .map(|e| e.parent.and_then(|p| index_of.get(&p).copied()))
        .collect();

    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, pp) in parent_pos.iter().enumerate() {
        if let Some(p) = *pp {
            children[p].push(i);
        }
    }

    // Depth via the parent chain (top-level = 1), bounded so a malformed cyclic
    // chain cannot loop forever.
    let depth: Vec<usize> = (0..n).map(|i| compute_depth(i, &parent_pos, n)).collect();

    // F-303 merged fields for every entry, keyed by id.
    let merged_by_id: HashMap<usize, MergedEntry> = {
        let inputs: Vec<EntryInput> = entries
            .iter()
            .map(|e| EntryInput {
                id: e.id,
                parent: e.parent,
                name: e.name.clone(),
                kind: e.kind,
            })
            .collect();
        extract(&inputs).into_iter().map(|m| (m.id, m)).collect()
    };

    let mut class: Vec<Option<FolderClass>> = vec![None; n];
    let mut rule: Vec<&'static str> = vec![""; n];
    let mut evid: Vec<Evidence> = vec![Evidence::default(); n];
    let mut is_disc: Vec<bool> = vec![false; n];
    let mut audio_beneath: Vec<bool> = vec![false; n];
    let mut content_beneath: Vec<bool> = vec![false; n];

    // Bottom-up: deepest folders first, ties broken by position for stability.
    let mut order: Vec<usize> = (0..n)
        .filter(|&i| entries[i].kind == NodeKind::Folder)
        .collect();
    order.sort_by(|&a, &b| depth[b].cmp(&depth[a]).then(a.cmp(&b)));

    for &i in &order {
        classify_folder(
            i,
            entries,
            &children,
            &merged_by_id,
            &mut class,
            &mut rule,
            &mut evid,
            &mut is_disc,
            &mut audio_beneath,
            &mut content_beneath,
        );
    }

    // id -> class, for the shelf-suppression post-pass.
    let class_by_id: HashMap<usize, FolderClass> = (0..n)
        .filter_map(|i| class[i].map(|c| (entries[i].id, c)))
        .collect();

    // Shelf-inheritance suppression: fill each book / multi-book folder's
    // effective author + series, dropping inherited values whose ancestor is a
    // non-author container (pack-container / staging).
    for i in 0..n {
        if !matches!(
            class[i],
            Some(FolderClass::Book) | Some(FolderClass::MultiBookSuspect)
        ) {
            continue;
        }
        if let Some(m) = merged_by_id.get(&entries[i].id) {
            evid[i].clean_author = clean_inheritable(
                m.fields
                    .author
                    .as_ref()
                    .map(|f| (f.source, f.value.as_str(), f.inherited_from)),
                &class_by_id,
            );
            evid[i].clean_series = clean_inheritable(
                m.fields
                    .series
                    .as_ref()
                    .map(|f| (f.source, f.value.as_str(), f.inherited_from)),
                &class_by_id,
            );
        }
    }

    (0..n)
        .filter(|&i| entries[i].kind == NodeKind::Folder)
        .map(|i| FolderClassification {
            id: entries[i].id,
            class: class[i].expect("every folder is classified"),
            rule_id: rule[i],
            evidence: std::mem::take(&mut evid[i]),
        })
        .collect()
}

/// Resolve a book's effective author/series value: keep an own-name value, keep
/// an inherited value only when its ancestor is a real author container (not a
/// pack-container / staging shelf), drop a derived value.
fn clean_inheritable(
    field: Option<(FieldSource, &str, Option<usize>)>,
    class_by_id: &HashMap<usize, FolderClass>,
) -> Option<String> {
    let (source, value, from) = field?;
    match source {
        FieldSource::OwnName => Some(value.to_string()),
        FieldSource::Inherited => {
            let ancestor_is_shelf = from
                .and_then(|id| class_by_id.get(&id))
                .map(|c| matches!(c, FolderClass::PackContainer | FolderClass::Staging))
                .unwrap_or(false);
            if ancestor_is_shelf {
                None
            } else {
                Some(value.to_string())
            }
        }
        FieldSource::Derived => None,
    }
}

fn compute_depth(mut i: usize, parent_pos: &[Option<usize>], bound: usize) -> usize {
    let mut d = 1;
    let mut steps = 0;
    while let Some(p) = parent_pos[i] {
        if steps > bound {
            break;
        }
        steps += 1;
        d += 1;
        i = p;
    }
    d
}

#[allow(clippy::too_many_arguments)]
fn classify_folder(
    i: usize,
    entries: &[ClassifyInput],
    children: &[Vec<usize>],
    merged_by_id: &HashMap<usize, MergedEntry>,
    class: &mut [Option<FolderClass>],
    rule: &mut [&'static str],
    evid: &mut [Evidence],
    is_disc: &mut [bool],
    audio_beneath: &mut [bool],
    content_beneath: &mut [bool],
) {
    let name = entries[i].name.as_str();

    // Partition direct children.
    let mut direct_audio: Vec<usize> = Vec::new();
    let mut direct_video = 0u64;
    let mut direct_comic = 0u64;
    let mut direct_other = 0u64; // non-audio content (ebook/image/release-info/...)
    let mut child_folders: Vec<usize> = Vec::new();
    for &c in &children[i] {
        match entries[c].kind {
            NodeKind::Folder => child_folders.push(c),
            NodeKind::File => match entries[c].file_class {
                Some(FileClass::Audio) => direct_audio.push(c),
                Some(FileClass::Video) => direct_video += 1,
                Some(FileClass::Comic) => direct_comic += 1,
                _ => direct_other += 1,
            },
        }
    }

    // Beneath-flags (children already processed; safe to read their flags).
    audio_beneath[i] = !direct_audio.is_empty() || child_folders.iter().any(|&c| audio_beneath[c]);
    content_beneath[i] = direct_other > 0
        || child_folders
            .iter()
            .any(|&c| content_beneath[c] || class[c] == Some(FolderClass::DocsResources));

    macro_rules! set {
        ($fc:expr, $r:expr) => {{
            class[i] = Some($fc);
            rule[i] = $r;
        }};
    }

    // Rule 1: staging.
    if is_staging_name(name) {
        evid[i].detected = Some("staging");
        set!(FolderClass::Staging, "staging-area");
        return;
    }

    // Rule 2: FD-17 non-book media (radio play, then video, then comics).
    let name_lc = name.to_lowercase();
    let radio_play = name_lc.contains("radio play")
        || direct_audio
            .iter()
            .any(|&c| entries[c].name.to_lowercase().contains("radio play"));
    if radio_play {
        evid[i].detected = Some("radio-play");
        evid[i].audio_files = direct_audio.len() as u64;
        set!(FolderClass::ManualReview, "fd17-radio-play");
        return;
    }
    if direct_video > 0 && direct_video >= direct_audio.len() as u64 {
        evid[i].detected = Some("video-course");
        evid[i].video_files = direct_video;
        evid[i].audio_files = direct_audio.len() as u64;
        set!(FolderClass::ManualReview, "fd17-video-course");
        return;
    }
    if direct_comic > 0 && direct_comic >= direct_audio.len() as u64 {
        evid[i].detected = Some("comic");
        evid[i].comic_files = direct_comic;
        evid[i].audio_files = direct_audio.len() as u64;
        set!(FolderClass::ManualReview, "fd17-comic");
        return;
    }

    // Rule 3: direct audio present.
    if !direct_audio.is_empty() {
        evid[i].audio_files = direct_audio.len() as u64;
        if !child_folders.is_empty() {
            evid[i].child_folders = child_folders.len() as u64;
            set!(FolderClass::Mixed, "mixed-audio-and-folders");
            return;
        }
        // Audio leaf: book vs multi-book by content (F-203).
        let files: Vec<BookFile> = direct_audio
            .iter()
            .map(|&c| BookFile {
                name: entries[c].name.as_str(),
                title: merged_by_id
                    .get(&entries[c].id)
                    .and_then(|m| m.fields.title.as_ref())
                    .map(|f| f.value.as_str()),
                series_index: merged_by_id
                    .get(&entries[c].id)
                    .and_then(|m| m.fields.series_index.as_ref())
                    .map(|f| f.value.as_str()),
                size: entries[c].size,
            })
            .collect();
        let verdict = detect_multibook(&files);
        if verdict.is_multi {
            evid[i].distinct_titles = verdict.distinct_titles;
            set!(FolderClass::MultiBookSuspect, "multibook-distinct-titles");
            return;
        }
        // A single book. Mark disc-part folders so a disc-split parent can fold
        // them (they still classify as `book`; F-204 refines discs in v0.3.0).
        if is_disc_folder_name(name) {
            is_disc[i] = true;
            evid[i].detected = Some("disc-part");
            set!(FolderClass::Book, "disc-part");
        } else {
            set!(FolderClass::Book, "book-audio-leaf");
        }
        return;
    }

    // Rule 4: no direct audio.
    if child_folders.is_empty() {
        // Rule 5: leaf with no audio.
        if direct_other > 0 {
            evid[i].non_audio_files = direct_other;
            set!(FolderClass::DocsResources, "docs-resources-leaf");
        } else {
            set!(FolderClass::Empty, "empty-leaf");
        }
        return;
    }

    evid[i].child_folders = child_folders.len() as u64;

    // Disc-split single book: every child folder is a disc part (AC-203.3).
    if child_folders.iter().all(|&c| is_disc[c]) {
        evid[i].detected = Some("disc-split");
        set!(FolderClass::Book, "disc-split-single-book");
        return;
    }

    let book_like: Vec<usize> = child_folders
        .iter()
        .copied()
        .filter(|&c| {
            matches!(
                class[c],
                Some(FolderClass::Book)
                    | Some(FolderClass::MultiBookSuspect)
                    | Some(FolderClass::SeriesContainer)
                    | Some(FolderClass::PackContainer)
                    | Some(FolderClass::Mixed)
            )
        })
        .collect();

    if !book_like.is_empty() {
        evid[i].book_children = book_like.len() as u64;
        decide_container(
            i,
            entries,
            &book_like,
            merged_by_id,
            name,
            class,
            rule,
            evid,
        );
        return;
    }

    if child_folders
        .iter()
        .any(|&c| class[c] == Some(FolderClass::ManualReview))
    {
        evid[i].detected = Some("non-book-cluster");
        set!(FolderClass::ManualReview, "manual-review-cluster");
        return;
    }

    // Only empty / docs-resources children remain.
    if content_beneath[i] {
        set!(FolderClass::DocsResources, "docs-resources-container");
    } else {
        set!(FolderClass::Empty, "empty-container");
    }
}

/// Decide a book-like container between `series-container` (a real author /
/// single-series grouping) and `pack-container` (a collection of distinct
/// books: genre shelf, source pack, or list).
#[allow(clippy::too_many_arguments)]
fn decide_container(
    i: usize,
    entries: &[ClassifyInput],
    book_like: &[usize],
    merged_by_id: &HashMap<usize, MergedEntry>,
    name: &str,
    class: &mut [Option<FolderClass>],
    rule: &mut [&'static str],
    evid: &mut [Evidence],
) {
    let mut child_authors: BTreeSet<String> = BTreeSet::new();
    let mut series_values: Vec<String> = Vec::new();
    let mut series_signal = 0u64;
    for &c in book_like {
        if let Some(m) = merged_by_id.get(&entries[c].id) {
            if let Some(a) = &m.fields.author {
                if a.source == FieldSource::OwnName {
                    child_authors.insert(a.value.clone());
                }
            }
            let has_own_series = m
                .fields
                .series
                .as_ref()
                .is_some_and(|s| s.source == FieldSource::OwnName);
            let has_own_index = m
                .fields
                .series_index
                .as_ref()
                .is_some_and(|s| s.source == FieldSource::OwnName);
            if let Some(s) = &m.fields.series {
                if s.source == FieldSource::OwnName {
                    series_values.push(s.value.clone());
                }
            }
            if has_own_series || has_own_index {
                series_signal += 1;
            }
        }
    }

    // A series value shared by two or more children is a strong single-series
    // signal even without indices.
    let shared_series = {
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for v in &series_values {
            *counts.entry(v.as_str()).or_default() += 1;
        }
        counts.values().any(|&c| c >= 2)
    };

    let own = merged_by_id.get(&entries[i].id);
    let own_author = own.and_then(|m| {
        m.fields
            .author
            .as_ref()
            .filter(|a| a.source == FieldSource::OwnName)
            .map(|a| a.value.clone())
    });
    let own_series = own.and_then(|m| {
        m.fields
            .series
            .as_ref()
            .filter(|s| s.source == FieldSource::OwnName)
            .map(|s| s.value.clone())
    });
    let genre_shelf = is_genre_shelf(name, own_author.as_deref());
    let own_author_real = if genre_shelf {
        None
    } else {
        own_author.clone()
    };

    evid[i].series_signal = series_signal;
    evid[i].distinct_child_authors = child_authors.iter().cloned().collect();
    evid[i].own_author = own_author;
    evid[i].own_series = own_series;

    let series_container =
        |class: &mut [Option<FolderClass>], rule: &mut [&'static str], r: &'static str| {
            class[i] = Some(FolderClass::SeriesContainer);
            rule[i] = r;
        };

    if series_signal >= 1 && child_authors.len() <= 1 {
        series_container(class, rule, "series-container-indexed");
    } else if shared_series {
        series_container(class, rule, "series-container-shared-series");
    } else if own_author_real.is_some() && child_authors.len() <= 1 {
        series_container(class, rule, "series-container-author-named");
    } else {
        if genre_shelf {
            evid[i].detected = Some("genre-shelf");
        }
        class[i] = Some(FolderClass::PackContainer);
        rule[i] = "pack-container-collection";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: usize, parent: Option<usize>, name: &str) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }
    }

    fn audio(id: usize, parent: Option<usize>, name: &str, size: u64) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::File,
            file_class: Some(FileClass::Audio),
            size,
        }
    }

    fn file_of(id: usize, parent: Option<usize>, name: &str, fc: FileClass) -> ClassifyInput {
        ClassifyInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::File,
            file_class: Some(fc),
            size: 1_000,
        }
    }

    fn class_of(cs: &[FolderClassification], id: usize) -> FolderClass {
        cs.iter().find(|c| c.id == id).expect("id classified").class
    }

    fn rule_of(cs: &[FolderClassification], id: usize) -> &'static str {
        cs.iter()
            .find(|c| c.id == id)
            .expect("id classified")
            .rule_id
    }

    /// AC-201.6: a folder with only non-audio content is docs-resources; a
    /// folder with nothing (or only empty subfolders) is empty. They are
    /// distinct classes.
    #[test]
    fn empty_is_distinct_from_docs_resources() {
        let entries = vec![
            // docs-resources leaf: only an ebook + a cover image.
            folder(0, None, "Reference Shelf"),
            file_of(1, Some(0), "notes.epub", FileClass::Ebook),
            file_of(2, Some(0), "cover.jpg", FileClass::Image),
            // empty leaf: no files at all.
            folder(3, None, "Nothing Here"),
            // empty container: only an empty subfolder beneath.
            folder(4, None, "Outer"),
            folder(5, Some(4), "Inner Empty"),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::DocsResources);
        assert_eq!(class_of(&cs, 3), FolderClass::Empty);
        assert_eq!(class_of(&cs, 5), FolderClass::Empty);
        assert_eq!(class_of(&cs, 4), FolderClass::Empty);
    }

    /// A docs-resources signal propagates up: a container whose only content
    /// beneath is non-audio is docs-resources, not empty.
    #[test]
    fn docs_resources_container_over_empty() {
        let entries = vec![
            folder(0, None, "Manuals"),
            folder(1, Some(0), "PDFs"),
            file_of(2, Some(1), "guide.pdf", FileClass::Ebook),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 1), FolderClass::DocsResources);
        assert_eq!(class_of(&cs, 0), FolderClass::DocsResources);
    }

    /// AC-201.4: an unclassifiable non-book cluster routes to manual-review as
    /// a first-class outcome (no panic, no error).
    #[test]
    fn non_book_cluster_is_manual_review() {
        let entries = vec![
            folder(0, None, "Video Course Cluster"),
            folder(1, Some(0), "52 Sales Lessons"),
            file_of(2, Some(1), "Lesson 01.mp4", FileClass::Video),
            file_of(3, Some(1), "Lesson 02.mp4", FileClass::Video),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 1), FolderClass::ManualReview);
        assert_eq!(rule_of(&cs, 1), "fd17-video-course");
        assert_eq!(class_of(&cs, 0), FolderClass::ManualReview);
        assert_eq!(rule_of(&cs, 0), "manual-review-cluster");
    }

    /// AC-201.3 / gate G-06: a radio play routes to manual-review even though
    /// it contains audio.
    #[test]
    fn radio_play_is_manual_review() {
        let entries = vec![
            folder(0, None, "Radio Play"),
            audio(
                1,
                Some(0),
                "The War of the Worlds Radio Play - Part 1.mp3",
                60_000,
            ),
            audio(
                2,
                Some(0),
                "The War of the Worlds Radio Play - Part 2.mp3",
                60_000,
            ),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::ManualReview);
        assert_eq!(rule_of(&cs, 0), "fd17-radio-play");
    }

    /// Staging areas classify as staging.
    #[test]
    fn staging_area_detected() {
        let entries = vec![
            folder(0, None, "_sort"),
            audio(1, Some(0), "Some Book by Some Author.m4b", 100_000),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::Staging);
    }

    /// AC-203.3: a disc-split single book is one `book`, not a container and not
    /// multi-book. The disc parts are marked but the parent folds to one book.
    #[test]
    fn disc_split_single_book() {
        let entries = vec![
            folder(0, None, "Verbal Advantage"),
            folder(1, Some(0), "Disc 1"),
            audio(2, Some(1), "track01.mp3", 45_000),
            audio(3, Some(1), "track02.mp3", 45_000),
            folder(4, Some(0), "CD2"),
            audio(5, Some(4), "track01.mp3", 45_000),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::Book);
        assert_eq!(rule_of(&cs, 0), "disc-split-single-book");
        // The disc children are books too (audio leaves), never multi-book.
        assert_eq!(class_of(&cs, 1), FolderClass::Book);
        assert_eq!(class_of(&cs, 4), FolderClass::Book);
        assert_ne!(class_of(&cs, 0), FolderClass::MultiBookSuspect);
    }

    /// A real author/series container: children carry series indices under one
    /// author, so it is a series-container, not a pack-container.
    #[test]
    fn indexed_children_make_a_series_container() {
        let entries = vec![
            folder(0, None, "Jim Butcher - Dresden Files"),
            folder(1, Some(0), "14.0 - Cold Days"),
            audio(2, Some(1), "14.0 - Cold Days.m4b", 100_000),
            folder(3, Some(0), "08.0 - Proven Guilty"),
            audio(4, Some(3), "08.0 - Proven Guilty.m4b", 100_000),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::SeriesContainer);
        // The book under the series container keeps its inherited author.
        let cold = cs.iter().find(|c| c.id == 1).unwrap();
        assert_eq!(cold.evidence.clean_author.as_deref(), Some("Jim Butcher"));
    }

    /// A genre shelf (author parses to `Genre`, children have distinct authors)
    /// is a pack-container, and its books' shelf-derived author is suppressed.
    #[test]
    fn genre_shelf_is_pack_and_suppresses_inherited_author() {
        let entries = vec![
            folder(0, None, "Genre - SciFI"),
            folder(1, Some(0), "Andy Weir - Project Hail Mary"),
            audio(2, Some(1), "Andy Weir - Project Hail Mary.m4b", 100_000),
            folder(3, Some(0), "Rework"),
            audio(4, Some(3), "Rework.m4b", 90_000),
        ];
        let cs = classify(&entries);
        assert_eq!(class_of(&cs, 0), FolderClass::PackContainer);
        // Own-name author kept.
        let phm = cs.iter().find(|c| c.id == 1).unwrap();
        assert_eq!(phm.evidence.clean_author.as_deref(), Some("Andy Weir"));
        // Bare book under the shelf: its inherited `Genre` author is suppressed.
        let rework = cs.iter().find(|c| c.id == 3).unwrap();
        assert_eq!(rework.evidence.clean_author, None);
    }

    /// F-203, the Wings of Fire fixture (`FixtureFamily::MultiBookWingsOfFire`
    /// in `crate::fixtures::manifest`): a series distinguished PRIMARILY by a
    /// leading series index still flags as `multi-book-suspect`, with the
    /// evidence's distinct-title count matching the five real per-book titles
    /// (not the shared series name and not the raw file count).
    #[test]
    fn wings_of_fire_series_index_folder_is_multibook_suspect() {
        let entries = vec![
            folder(0, None, "Wings of Fire"),
            audio(
                1,
                Some(0),
                "Wings of Fire 01 - The Dragonet Prophecy.mp3",
                90_000,
            ),
            audio(2, Some(0), "Wings of Fire 02 - The Lost Heir.mp3", 88_000),
            audio(
                3,
                Some(0),
                "Wings of Fire 03 - The Hidden Kingdom.mp3",
                85_000,
            ),
            audio(4, Some(0), "Wings of Fire 04 - The Dark Secret.mp3", 82_000),
            audio(
                5,
                Some(0),
                "Wings of Fire 05 - The Brightest Night.mp3",
                91_000,
            ),
        ];
        let cs = classify(&entries);
        let f = cs.iter().find(|c| c.id == 0).unwrap();
        assert_eq!(f.class, FolderClass::MultiBookSuspect);
        assert_eq!(f.rule_id, "multibook-distinct-titles");
        assert_eq!(f.evidence.distinct_titles.len(), 5);
        assert_eq!(
            f.evidence.distinct_titles,
            vec![
                "the brightest night".to_string(),
                "the dark secret".to_string(),
                "the dragonet prophecy".to_string(),
                "the hidden kingdom".to_string(),
                "the lost heir".to_string(),
            ]
        );
    }

    /// Determinism: classifying twice yields identical output.
    #[test]
    fn classification_is_deterministic() {
        let entries = vec![
            folder(0, None, "Genre - SciFI"),
            folder(1, Some(0), "Andy Weir - Project Hail Mary"),
            audio(2, Some(1), "Andy Weir - Project Hail Mary.m4b", 100_000),
        ];
        assert_eq!(classify(&entries), classify(&entries));
    }
}
