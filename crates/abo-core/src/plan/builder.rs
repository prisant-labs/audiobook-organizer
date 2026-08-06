//! F-403 (plan builder): turn one snapshot plus one ruleset into the ordered,
//! immutable, deterministic operation list that is the product's contract with
//! the user.
//!
//! The plan is the trust ceremony's payload: given the SAME snapshot and the
//! SAME ruleset, [`build_plan`] must produce a byte-identical serialized plan
//! forever (AC-8), and every operation must carry a human rationale, a rule id,
//! a confidence, and a byte size (AC-9). Nothing here touches the filesystem or
//! a database: it is pure logic over already-decided inputs (the snapshot
//! entries, the F-201 classifications, the F-303 merged fields, and the F-801
//! ruleset). Persistence for real callers is a thin, separate step in
//! [`crate::plan::validate::persist_validated_plan`] (validate-before-insert),
//! so the pure builder stays trivially testable and deterministic. Like the
//! rest of `crate::plan`, this module's production logic has no `cfg`-gating
//! (the CFG RULE): path handling is string SEMANTICS keyed off the library
//! root's own separator, so a plan built on one host serializes identically on
//! another. (The one `cfg(test)` item is `persist_plan_for_tests`, a
//! persistence shim used only by unit tests; production never persists a plan
//! that way.)
//!
//! # The eight internal passes and the seven user-facing groups (FD-26, AC-11)
//!
//! Work is organized into eight internal passes ([`InternalPass`]) that fold
//! onto seven user-facing campaign groups ([`CampaignGroup`]) by collapsing
//! `strip-noise` and `normalize-series` into one "messy names" group. The
//! persisted `plan_ops.op_group` column stores the INTERNAL pass name (the
//! fine-grained provenance of each op, matching the sample ops in
//! [`crate::db::plans`]); the seven-group fold is a display mapping the export
//! (F-505) and report (F-506) apply. [`CampaignGroup::ALL`] is the canonical
//! seven-label set the review UI and report agree on.
//!
//! v0.3.0 Phase 4 built the passes whose behavior is fully grounded in the
//! classifications: `loose-root-books`, `strip-noise`, `split-multi-book`,
//! `flatten-packs` (with F-507 provenance capture), and `empty-cleanup`.
//! `staging-separation` emits a documented no-op per staging area (contents are
//! separated by a later, interactive step).
//!
//! v0.3.0 Phase 6 fills in the two passes Phase 4 left wired-but-empty, plus the
//! F-402 clutter and F-205 parallel-format policy ops:
//!   - `normalize-series` now emits F-204 disc renames: a nonconforming
//!     disc-part folder name (`CD3`, `Disk_04`, `Disc1`) is renamed to the
//!     conformant `CD 3` / `Disk 04` / `Disc 1` shape (see [`crate::plan::disc`],
//!     AC-32); conformant disc folders are left untouched.
//!   - `empty-cleanup` now also sets aside the F-205 parallel-format loser (the
//!     non-preferred-format copy when a book holds both, default keep m4b,
//!     AC-33) and the F-402 non-audio clutter the ruleset quarantines
//!     (nfo/sfv/playlist/weblink; ebook/cover are kept, AC-6). These are
//!     quarantine ops, so they live in `empty-cleanup` (strictly after all
//!     moves) with the pack-shell quarantines and empty-folder removals.
//!
//! `dedupe-quarantine` remains empty in the PLAN: F-701 duplicate detection is
//! candidate-only this release (flag-only, no auto-quarantine until later hash
//! verification), so it writes the `duplicate_groups`/`duplicate_members` tables
//! (see [`crate::dupes`]) and the report counts groups from there, but emits no
//! plan op. Every pass still participates in the group fold so the seven-group
//! contract (AC-11) holds.
//!
//! # Ordering (AC-10)
//!
//! Passes run in [`InternalPass::ALL`] order; within a pass, work units are
//! processed in ascending source-path order. Two dependency invariants are
//! enforced structurally rather than by a post-hoc sort:
//!   - **mkdir before move-into**: each unit emits the `mkdir` ops for the
//!     ancestor folders its move needs BEFORE the move itself, deduplicated
//!     against a shared created-directory set (seeded with every folder that
//!     already exists in the snapshot), so no move ever targets a directory not
//!     yet created.
//!   - **moves out before rmdir**: every `rmdir-empty` and pack-shell
//!     `quarantine` op is emitted in the final `empty-cleanup` pass, strictly
//!     after every move that empties a folder, so no removal precedes the moves
//!     that empty it.
//!
//! # Fabricate nothing
//!
//! A book that cannot be placed without guessing (a missing required template
//! field, per F-401) becomes a `no-op(manual-review)` carrying its reason, never
//! a guessed move (AC-7 posture at the op level). A folder the classifier routed
//! to `manual-review` (FD-17 non-book media) likewise produces no move/rename.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::classify::engine::{ClassifyInput, FolderClass, FolderClassification};
use crate::parse::extract::{Confidence, FieldSource, MergedEntry, NodeKind};
use crate::parse::strip::{strip, StripOptions};
use crate::plan::disc::conformant_disc_rename;
use crate::plan::templates::{render_path, ManualReviewReason, RenderOutcome, TemplateFields};
use crate::ruleset::{
    clutter_kind_from_name, pack_shell_disposition, ClutterAction, PackShellOutcome,
    PreferredFormat, Ruleset,
};
use crate::scan::typing::FileClass;

/// The folder name new set-aside shells and clutter land under. Never deletes:
/// the never-delete-audio invariant means a pack shell or non-preferred copy is
/// MOVED here, not removed. The on-disk name is the plain-language "Set Aside"
/// (FD-31); "quarantine" stays internal vocabulary (the const name, the op kind,
/// type names) and never appears on disk or in a stored rationale sentence.
///
/// FD-34: the set-aside ROOT this names is a SIBLING of the library, resolved
/// OUTSIDE the library root (default `<library-parent>\Set Aside\`), not a folder
/// inside it. See [`default_set_aside_root`].
pub const QUARANTINE_DIRNAME: &str = "Set Aside";

/// The literal path segment the builder stamps where the apply job's id will go
/// in a set-aside target (`<set-aside-root>\{job-id}\<original relative path>`,
/// FD-34). The plan is built before any apply job exists, and `plan_ops` is
/// frozen, so the builder cannot know the real job id: it writes this stable
/// placeholder, and the executor substitutes the real `jobs.id` into the segment
/// at apply time for the ops it walks (never rewriting the frozen row). A fixed
/// token keeps the builder pure and its golden job-independent; the per-job
/// segment gives each tidy-up its own collision-free set-aside folder (FD-34).
pub const QUARANTINE_JOB_PLACEHOLDER: &str = "{job-id}";

/// The FD-34 default set-aside root: a `Set Aside` folder that is a SIBLING of
/// the library (one level up from `library_root`), resolved OUTSIDE the library
/// so a post-apply rescan and family media players never index set-aside items.
/// Same-volume by construction (a sibling is on the library's volume), which
/// keeps set-aside moves rename-first (D-08).
///
/// Pure string semantics keyed off the library root's own separator (the CFG
/// RULE), so it is host-independent. A bare synthetic root with no separator
/// (the fixture convention, e.g. `library`) yields the bare sibling name
/// `Set Aside` at the same level. A real root (`E:\Books - Audio`) yields
/// `E:\Set Aside`. A configured override (F-803 settings) replaces this default.
pub fn default_set_aside_root(library_root: &str) -> String {
    let sep = separator_of(library_root);
    let parent = parent_dir(library_root, sep);
    if parent == library_root {
        // No separator in the root: the sibling sits at the same (top) level.
        QUARANTINE_DIRNAME.to_string()
    } else {
        join_path(parent, sep, &[QUARANTINE_DIRNAME])
    }
}

/// `path` made relative to `library_root` (the root prefix and one leading
/// separator removed), so a set-aside target can preserve a book's ORIGINAL
/// relative path under the per-job folder (FD-34, AC-21). A path that does not
/// sit under the root is returned unchanged (defensive: the builder only ever
/// sets aside items scanned under the library root).
fn rel_under_root<'a>(path: &'a str, library_root: &str, sep: char) -> &'a str {
    match path.strip_prefix(library_root) {
        Some(rest) => rest.trim_start_matches(sep),
        None => path,
    }
}

/// One of the eight internal plan passes. The order of [`InternalPass::ALL`]
/// is the order passes run in, which (together with per-pass source-path
/// ordering) fixes the plan's operation sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InternalPass {
    /// Route staging/inbox areas (`_sort`) for later separation.
    StagingSeparation,
    /// Fold audio files sitting loose at the library root into book folders.
    LooseRootBooks,
    /// Strip ripper/noise markers from folder names kept in place.
    StripNoise,
    /// Split a folder holding several complete books into one folder per book.
    SplitMultiBook,
    /// Extract books from a pack/collection container to their canonical home.
    FlattenPacks,
    /// Normalize series-index / disc-part naming (F-204 feeds this in Phase 6).
    NormalizeSeries,
    /// Quarantine duplicate copies (F-701 feeds this in Phase 6).
    DedupeQuarantine,
    /// Remove verified-empty folders and quarantine emptied pack shells.
    EmptyCleanup,
}

impl InternalPass {
    /// Every internal pass, in run order.
    pub const ALL: &'static [InternalPass] = &[
        InternalPass::StagingSeparation,
        InternalPass::LooseRootBooks,
        InternalPass::StripNoise,
        InternalPass::SplitMultiBook,
        InternalPass::FlattenPacks,
        InternalPass::NormalizeSeries,
        InternalPass::DedupeQuarantine,
        InternalPass::EmptyCleanup,
    ];

    /// The stable kebab-case name stored in `plan_ops.op_group`.
    pub fn as_str(self) -> &'static str {
        match self {
            InternalPass::StagingSeparation => "staging-separation",
            InternalPass::LooseRootBooks => "loose-root-books",
            InternalPass::StripNoise => "strip-noise",
            InternalPass::SplitMultiBook => "split-multi-book",
            InternalPass::FlattenPacks => "flatten-packs",
            InternalPass::NormalizeSeries => "normalize-series",
            InternalPass::DedupeQuarantine => "dedupe-quarantine",
            InternalPass::EmptyCleanup => "empty-cleanup",
        }
    }

    /// The user-facing campaign group this pass folds into (FD-26).
    pub fn group(self) -> CampaignGroup {
        match self {
            InternalPass::StagingSeparation => CampaignGroup::Staging,
            InternalPass::LooseRootBooks => CampaignGroup::LooseBooks,
            InternalPass::StripNoise | InternalPass::NormalizeSeries => CampaignGroup::MessyNames,
            // A multi-book folder is a "box set" the user splits into separate
            // books; a pack/collection container is a "collection bundle" the
            // user unpacks (prototype canon, FD-26).
            InternalPass::SplitMultiBook => CampaignGroup::BoxSets,
            InternalPass::FlattenPacks => CampaignGroup::Bundles,
            InternalPass::DedupeQuarantine => CampaignGroup::Copies,
            InternalPass::EmptyCleanup => CampaignGroup::EmptyFolders,
        }
    }
}

/// One of the seven user-facing campaign groups (FD-26 canon). These are the
/// labels the review UI and the dry-run report share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CampaignGroup {
    Staging,
    LooseBooks,
    MessyNames,
    BoxSets,
    Bundles,
    Copies,
    EmptyFolders,
}

impl CampaignGroup {
    /// Every user-facing group, in the canonical display order.
    pub const ALL: &'static [CampaignGroup] = &[
        CampaignGroup::Staging,
        CampaignGroup::LooseBooks,
        CampaignGroup::MessyNames,
        CampaignGroup::BoxSets,
        CampaignGroup::Bundles,
        CampaignGroup::Copies,
        CampaignGroup::EmptyFolders,
    ];

    /// The FD-26 canonical label.
    pub fn label(self) -> &'static str {
        match self {
            CampaignGroup::Staging => "staging",
            CampaignGroup::LooseBooks => "loose books",
            CampaignGroup::MessyNames => "messy names",
            CampaignGroup::BoxSets => "box sets",
            CampaignGroup::Bundles => "bundles",
            CampaignGroup::Copies => "copies",
            CampaignGroup::EmptyFolders => "empty folders",
        }
    }

    /// The stable kebab-case identifier this group crosses IPC as (v0.4.0
    /// Phase 5, `crate::plan::query`/`crate::ipc::PlanGroupView::group`).
    /// Distinct from [`InternalPass::as_str`] (the fine-grained pass name
    /// stored in `plan_ops.op_group`): this is the coarser seven-group id the
    /// review UI's group cards and their approval commands key off.
    pub fn slug(self) -> &'static str {
        match self {
            CampaignGroup::Staging => "staging",
            CampaignGroup::LooseBooks => "loose-books",
            CampaignGroup::MessyNames => "messy-names",
            CampaignGroup::BoxSets => "box-sets",
            CampaignGroup::Bundles => "bundles",
            CampaignGroup::Copies => "copies",
            CampaignGroup::EmptyFolders => "empty-folders",
        }
    }

    /// Recover a [`CampaignGroup`] from its [`CampaignGroup::slug`]. `None`
    /// for an unrecognized string (never fabricates a group).
    pub fn from_slug(s: &str) -> Option<Self> {
        CampaignGroup::ALL.iter().copied().find(|g| g.slug() == s)
    }
}

/// One snapshot entry the builder operates over: identity + structure + the
/// full stored path. This is the builder's view of a persisted `entries` row
/// (see [`plan_nodes_from_snapshot`]); it carries the same fields as a
/// [`ClassifyInput`] plus the entry's full `path`, which the builder needs to
/// emit concrete source/target paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub name: String,
    pub path: String,
    pub kind: NodeKind,
    pub file_class: Option<FileClass>,
    pub size: u64,
}

/// One planned operation, owned. Mirrors the descriptive `plan_ops` columns the
/// caller freezes at insert time (see [`crate::db::plans::NewPlanOp`]); the
/// builder assigns every field here, and the persistence step copies them
/// verbatim.
/// `validation_state` is `"pending"` at build time (F-404 validation is Phase
/// 5); the field is present so the persisted shape is complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlannedOp {
    /// The internal pass (`plan_ops.op_group`).
    pub op_group: String,
    /// One of `mkdir`/`move`/`rename`/`quarantine`/`rmdir-empty`/`no-op`.
    pub kind: String,
    /// The reason qualifier for a `no-op` (`manual-review`, `staging`); `None`
    /// for a concrete operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_reason: Option<String>,
    pub source_path: String,
    pub target_path: String,
    /// A plain-language sentence stating what the op does and why.
    pub rationale: String,
    /// The rule id that produced this op (stable, kebab-case).
    pub rule_id: String,
    /// `high` / `medium` / `low`.
    pub confidence: String,
    /// Bytes the op moves (a file's size, or a folder's total descendant
    /// bytes); 0 for pure-structure ops (mkdir/rmdir/rename/no-op).
    pub byte_size: i64,
    /// F-507 pack-provenance JSON, present only on `flatten-packs` member moves.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance_json: Option<String>,
}

impl PlannedOp {
    /// The `validation_state` every freshly built op carries (F-404 has not run
    /// yet at build time).
    pub const VALIDATION_PENDING: &'static str = "pending";
}

/// A per-group operation count (deterministic, in [`CampaignGroup::ALL`] order).
///
/// `Deserialize` (added Phase 7, F-505) round-trips `plans.stats_json` back
/// into a value the exporter can read without recomputing counts; `group` is
/// already the FD-26 user-facing label, so this type carries no internal
/// vocabulary an export ever needs to scrub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCount {
    pub group: String,
    pub ops: u64,
}

/// Plan-level aggregate counts, serialized into `plans.stats_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStats {
    pub total_ops: u64,
    /// Operations that are `no-op(manual-review)` (a book/folder the plan
    /// deliberately refuses to touch without human input).
    pub manual_review_ops: u64,
    /// One entry per user-facing campaign group, always all seven, zeroed where
    /// a group produced nothing.
    pub per_group: Vec<GroupCount>,
}

/// The built plan: the ordered operations plus the aggregate stats. Its
/// [`Serialize`] form (via [`BuiltPlan::to_canonical_json`]) is the determinism
/// golden's subject (AC-8): identical input yields a byte-identical string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuiltPlan {
    pub ops: Vec<PlannedOp>,
    pub stats: PlanStats,
}

impl BuiltPlan {
    /// The canonical serialization used by the determinism golden. Pretty JSON
    /// (2-space indent) in a fixed field order over the operations in `seq`
    /// order; no map iteration, so byte-stable across runs and hosts.
    pub fn to_canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a BuiltPlan always serializes to JSON")
    }
}

/// Confidence tier as the stable string stored in `plan_ops.confidence`.
fn confidence_str(c: Confidence) -> &'static str {
    match c {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
    }
}

/// The worse (lower) of two confidence tiers (`low` worst, `high` best).
fn worse(a: Confidence, b: Confidence) -> Confidence {
    fn rank(c: Confidence) -> u8 {
        match c {
            Confidence::High => 2,
            Confidence::Medium => 1,
            Confidence::Low => 0,
        }
    }
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// Whether `name` carries strippable noise (its all-passes stripped form,
/// minus underscore-to-space normalization, differs from the original). Mirrors
/// the health-metric noise predicate so strip-noise targets exactly the folders
/// the F-202 "noisy names" count flagged.
fn noise_strip_options() -> StripOptions {
    StripOptions {
        bracket_tags: true,
        release_group_suffix: true,
        bitrate: true,
        size: true,
        rank_prefix: true,
        year_prefix: true,
        underscores_to_spaces: false,
    }
}

fn has_noise(name: &str) -> bool {
    strip(name, noise_strip_options()).text.trim() != name.trim()
}

/// The path separator the plan uses, inferred from the library root so a plan
/// over a Windows root keeps backslashes and a plan over a POSIX/synthetic root
/// keeps forward slashes. Determinism is preserved because the separator is a
/// pure function of the input root string, not of the host.
fn separator_of(library_root: &str) -> char {
    if library_root.contains('\\') {
        '\\'
    } else {
        '/'
    }
}

/// Join a root and trailing components with `sep`.
fn join_path(root: &str, sep: char, components: &[&str]) -> String {
    let mut out = root.trim_end_matches(sep).to_string();
    for c in components {
        out.push(sep);
        out.push_str(c);
    }
    out
}

/// The parent-directory portion of `path` (everything before the last
/// separator), or the whole path if it has none.
fn parent_dir(path: &str, sep: char) -> &str {
    match path.rfind(sep) {
        Some(idx) => &path[..idx],
        None => path,
    }
}

/// The final component of `path`.
fn base_name(path: &str, sep: char) -> &str {
    match path.rfind(sep) {
        Some(idx) => &path[idx + 1..],
        None => path,
    }
}

/// Owned template values pulled from a merged entry (plus optional cleaned
/// author/series overrides from classification evidence), so a borrowed
/// [`TemplateFields`] can be handed to [`render_path`].
struct BookFields {
    author: Option<String>,
    title: Option<String>,
    series: Option<String>,
    series_index: Option<String>,
    year: Option<u16>,
    /// The worst confidence among the fields actually consulted, used as the
    /// op's confidence.
    confidence: Confidence,
}

impl BookFields {
    fn as_template(&self) -> TemplateFields<'_> {
        TemplateFields {
            author: self.author.as_deref(),
            title: self.title.as_deref(),
            series: self.series.as_deref(),
            series_index: self.series_index.as_deref(),
            year: self.year,
            narrator: None,
            subtitle: None,
            genre: None,
        }
    }
}

fn source_confidence(src: FieldSource) -> Confidence {
    match src {
        FieldSource::OwnName => Confidence::High,
        FieldSource::Inherited => Confidence::Medium,
        FieldSource::Derived => Confidence::Low,
    }
}

/// Fields for a LOOSE FILE at the library root (pattern-1 `Title by Author`):
/// there is no classification to consult, so author/title/series come straight
/// from the file's own merged parse. Own-name values are safe here because a
/// loose file has no organizational-shelf ancestor to inherit a spurious author
/// from.
fn book_fields_from_file(merged: Option<&MergedEntry>) -> BookFields {
    let mut confidence = Confidence::High;
    let author = merged.and_then(|m| m.fields.author.as_ref()).map(|f| {
        confidence = worse(confidence, source_confidence(f.source));
        f.value.clone()
    });
    let title = merged.and_then(|m| m.fields.title.as_ref()).map(|f| {
        confidence = worse(confidence, source_confidence(f.source));
        f.value.clone()
    });
    let series = merged
        .and_then(|m| m.fields.series.as_ref())
        .map(|f| f.value.clone());
    let series_index = merged
        .and_then(|m| m.fields.series_index.as_ref())
        .map(|f| f.value.clone());
    let year = merged.and_then(|m| m.fields.year.as_ref()).map(|f| f.value);
    BookFields {
        author,
        title,
        series,
        series_index,
        year,
        confidence,
    }
}

/// Fields for a CLASSIFIED FOLDER (a pack member or a split book file). Author
/// and series come from the classification's shelf-suppressed clean values
/// ([`crate::classify::Evidence::clean_author`]/`clean_series`), NOT the raw
/// merged fields: a book sitting under a `Genre - X` shelf has a spurious
/// inherited `Genre` author in its merged fields, and the whole point of
/// suppression is to drop it. When the clean author is absent the book is
/// unplaceable (the render returns manual-review), which is the correct,
/// fabricate-nothing outcome rather than filing it under a shelf name. Title,
/// series index, and year are book-specific (never shelf-inherited) so they come
/// from the entry's own merged parse.
fn book_fields_from_folder(
    merged: Option<&MergedEntry>,
    clean_author: Option<&str>,
    clean_series: Option<&str>,
) -> BookFields {
    let mut confidence = Confidence::High;
    // Reflect the raw author field's source in confidence when a clean author
    // survived (a suppressed None already forces manual review downstream).
    if clean_author.is_some() {
        match merged.and_then(|m| m.fields.author.as_ref()) {
            Some(f) => confidence = worse(confidence, source_confidence(f.source)),
            None => confidence = worse(confidence, Confidence::Medium),
        }
    }
    let title = merged.and_then(|m| m.fields.title.as_ref()).map(|f| {
        confidence = worse(confidence, source_confidence(f.source));
        f.value.clone()
    });
    let series_index = merged
        .and_then(|m| m.fields.series_index.as_ref())
        .map(|f| f.value.clone());
    let year = merged.and_then(|m| m.fields.year.as_ref()).map(|f| f.value);
    BookFields {
        author: clean_author.map(|s| s.to_string()),
        title,
        series: clean_series.map(|s| s.to_string()),
        series_index,
        year,
        confidence,
    }
}

/// The plain-language reason a render fell to manual review.
fn manual_review_note(reason: ManualReviewReason) -> &'static str {
    match reason {
        ManualReviewReason::MissingAuthor => {
            "No author could be read from the name, so a safe destination cannot be chosen."
        }
        ManualReviewReason::MissingTitle => {
            "No title could be read from the name, so a safe destination cannot be chosen."
        }
        ManualReviewReason::MissingSeriesIndex => {
            "This looks like a series book but carries no book number, so its position cannot be guessed."
        }
    }
}

/// Internal working context shared across passes.
struct Builder<'a> {
    nodes: &'a [PlanNode],
    index_of: HashMap<usize, usize>,
    children: HashMap<usize, Vec<usize>>,
    class_by_id: HashMap<usize, &'a FolderClassification>,
    merged_by_id: HashMap<usize, &'a MergedEntry>,
    ruleset: &'a Ruleset,
    library_root: String,
    /// The resolved FD-34 set-aside root (a sibling of the library, OUTSIDE it),
    /// under which every set-aside target lands as
    /// `<set_aside_root>\{job-id}\<original relative path>`.
    set_aside_root: String,
    sep: char,
    /// Total descendant file bytes per folder id (0 for files/leaf-less).
    folder_bytes: HashMap<usize, u64>,
    /// Directory paths known to exist or already scheduled for creation.
    created_dirs: HashSet<String>,
    /// Folder ids already claimed by an earlier pass (never re-handled).
    handled: HashSet<usize>,
    /// Folder ids that an earlier pass MOVED or RENAMED as a unit (flatten-packs
    /// members, strip-noise renames, normalize-series disc renames). Their
    /// contents (including any clutter) travel with the folder, so clutter under
    /// them is not separately set aside (its stored path would be stale). Split
    /// multi-book folders are deliberately NOT here: they are emptied in place,
    /// so their leftover clutter IS set aside (FD-31, riding Box sets).
    relocated: HashSet<usize>,
    ops: Vec<PlannedOp>,
    /// Pack shells to quarantine in `empty-cleanup` (path, byte total = 0).
    quarantine_shells: Vec<(usize, String)>,
    /// Source folders emptied by a move pass, to `rmdir-empty` in cleanup.
    emptied_sources: BTreeSet<String>,
}

impl<'a> Builder<'a> {
    fn class_of(&self, id: usize) -> Option<FolderClass> {
        self.class_by_id.get(&id).map(|c| c.class)
    }

    /// Whether any ancestor of `id` is a pack container.
    fn inside_pack(&self, id: usize) -> bool {
        let mut cur = self.node(id).parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > self.nodes.len() {
                break;
            }
            steps += 1;
            if self.class_of(p) == Some(FolderClass::PackContainer) {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Whether any ancestor of `id` is a staging area.
    fn inside_staging(&self, id: usize) -> bool {
        let mut cur = self.node(id).parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > self.nodes.len() {
                break;
            }
            steps += 1;
            if self.class_of(p) == Some(FolderClass::Staging) {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Whether any ancestor of `id` has been claimed (moved/renamed) by an
    /// earlier pass. Used by `normalize-series` to skip a disc folder whose
    /// disc-split book was already relocated (renaming a child at its stale
    /// pre-move path would be wrong).
    fn ancestor_handled(&self, id: usize) -> bool {
        let mut cur = self.node(id).parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > self.nodes.len() {
                break;
            }
            steps += 1;
            if self.handled.contains(&p) {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Whether any ancestor of `id` was MOVED or RENAMED as a unit (in the
    /// [`Builder::relocated`] set). Clutter under such a folder rides along with
    /// it, so it is never separately set aside.
    fn ancestor_relocated(&self, id: usize) -> bool {
        let mut cur = self.node(id).parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > self.nodes.len() {
                break;
            }
            steps += 1;
            if self.relocated.contains(&p) {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Whether any ancestor of `id` is a multi-book folder (the split-multi-book
    /// pass owns its transformation, FD-31 Box sets).
    fn inside_multibook(&self, id: usize) -> bool {
        let mut cur = self.node(id).parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > self.nodes.len() {
                break;
            }
            steps += 1;
            if self.class_of(p) == Some(FolderClass::MultiBookSuspect) {
                return true;
            }
            cur = self.node(p).parent;
        }
        false
    }

    /// Whether `id` sits directly at the library root (its parent is the single
    /// scan root, or it is parentless in the multi-root fixture convention). Such
    /// root-side clutter rides the loose-root-books transformation (FD-31 Loose
    /// books).
    fn is_root_side(&self, id: usize) -> bool {
        match self.node(id).parent {
            None => true,
            Some(p) => scan_root_id(self.nodes) == Some(p),
        }
    }

    /// The internal pass whose user-facing group a clutter set-aside op inherits
    /// (FD-31): pack clutter rides Bundles (flatten-packs), multi-book clutter
    /// rides Box sets (split-multi-book), root-side clutter rides Loose books
    /// (loose-root-books), and standalone/unowned clutter joins the empty-cleanup
    /// pile. Precedence is pack > multi-book > root > standalone.
    fn clutter_owning_pass(&self, id: usize) -> InternalPass {
        if self.inside_pack(id) {
            InternalPass::FlattenPacks
        } else if self.inside_multibook(id) {
            InternalPass::SplitMultiBook
        } else if self.is_root_side(id) {
            InternalPass::LooseRootBooks
        } else {
            InternalPass::EmptyCleanup
        }
    }

    fn node(&self, id: usize) -> &PlanNode {
        &self.nodes[self.index_of[&id]]
    }

    /// Emit `mkdir` ops for each directory in `chain` (already the full path of
    /// each level, parent-first) that does not yet exist or is not yet
    /// scheduled. Ancestors precede descendants because `chain` is ordered.
    fn ensure_dirs(&mut self, chain: &[String], pass: InternalPass, rule_id: &str) {
        for dir in chain {
            if self.created_dirs.contains(dir) {
                continue;
            }
            self.created_dirs.insert(dir.clone());
            self.ops.push(PlannedOp {
                op_group: pass.as_str().to_string(),
                kind: "mkdir".to_string(),
                kind_reason: None,
                source_path: String::new(),
                target_path: dir.clone(),
                rationale: format!(
                    "Create the folder \"{}\" so books can be moved into it.",
                    base_name(dir, self.sep)
                ),
                rule_id: rule_id.to_string(),
                confidence: "high".to_string(),
                byte_size: 0,
                provenance_json: None,
            });
        }
    }

    /// Build the ordered list of ancestor directory paths for a rendered
    /// component list under the library root. `include_leaf` controls whether
    /// the final component (the book's own leaf folder) is itself a directory
    /// to create (true when a FILE moves into it; false when a FOLDER move
    /// creates it).
    fn dir_chain(&self, components: &[String], include_leaf: bool) -> Vec<String> {
        let take = if include_leaf {
            components.len()
        } else {
            components.len().saturating_sub(1)
        };
        let mut chain = Vec::with_capacity(take);
        let mut acc: Vec<&str> = Vec::new();
        for comp in components.iter().take(take) {
            acc.push(comp.as_str());
            chain.push(join_path(&self.library_root, self.sep, &acc));
        }
        chain
    }
}

/// Build a plan from a snapshot, its classifications, its merged fields, and a
/// ruleset. Pure and deterministic: identical inputs yield a byte-identical
/// [`BuiltPlan::to_canonical_json`] (AC-8). `library_root` is the scan root's
/// stored path (the folder every target is built under); its separator sets the
/// plan's path style.
pub fn build_plan(
    nodes: &[PlanNode],
    classifications: &[FolderClassification],
    merged: &[MergedEntry],
    ruleset: &Ruleset,
    library_root: &str,
) -> BuiltPlan {
    let set_aside_root = default_set_aside_root(library_root);
    build_plan_with_set_aside_root(
        nodes,
        classifications,
        merged,
        ruleset,
        library_root,
        &set_aside_root,
    )
}

/// [`build_plan`] with an explicit set-aside root (FD-34). The one production
/// caller ([`crate::plan::report::build_and_persist_plan`]) resolves the root
/// from F-803 settings, falling back to [`default_set_aside_root`]; every other
/// caller uses [`build_plan`], which passes the FD-34 default. Set-aside targets
/// become `<set_aside_root>\{job-id}\<original relative path>`
/// ([`QUARANTINE_JOB_PLACEHOLDER`]); everything else is identical.
pub fn build_plan_with_set_aside_root(
    nodes: &[PlanNode],
    classifications: &[FolderClassification],
    merged: &[MergedEntry],
    ruleset: &Ruleset,
    library_root: &str,
    set_aside_root: &str,
) -> BuiltPlan {
    let index_of: HashMap<usize, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();

    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    for n in nodes {
        if let Some(p) = n.parent {
            children.entry(p).or_default().push(n.id);
        }
    }
    // Deterministic child order: by source path.
    for v in children.values_mut() {
        v.sort_by(|&a, &b| nodes[index_of[&a]].path.cmp(&nodes[index_of[&b]].path));
    }

    let class_by_id: HashMap<usize, &FolderClassification> =
        classifications.iter().map(|c| (c.id, c)).collect();
    let merged_by_id: HashMap<usize, &MergedEntry> = merged.iter().map(|m| (m.id, m)).collect();

    let folder_bytes = compute_folder_bytes(nodes, &index_of);

    // Seed the created-dir set with every folder that already exists.
    let created_dirs: HashSet<String> = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Folder)
        .map(|n| n.path.clone())
        .collect();

    let mut b = Builder {
        nodes,
        index_of,
        children,
        class_by_id,
        merged_by_id,
        ruleset,
        library_root: library_root.to_string(),
        set_aside_root: set_aside_root.to_string(),
        sep: separator_of(library_root),
        folder_bytes,
        created_dirs,
        handled: HashSet::new(),
        relocated: HashSet::new(),
        ops: Vec::new(),
        quarantine_shells: Vec::new(),
        emptied_sources: BTreeSet::new(),
    };

    for &pass in InternalPass::ALL {
        match pass {
            InternalPass::StagingSeparation => pass_staging(&mut b),
            InternalPass::LooseRootBooks => pass_loose_root(&mut b),
            InternalPass::StripNoise => pass_strip_noise(&mut b),
            InternalPass::SplitMultiBook => pass_split_multibook(&mut b),
            InternalPass::FlattenPacks => pass_flatten_packs(&mut b),
            InternalPass::NormalizeSeries => pass_normalize_series(&mut b),
            // F-701 duplicate detection is candidate-only this release (writes
            // duplicate_groups/duplicate_members via crate::dupes, no plan op).
            InternalPass::DedupeQuarantine => { /* flag-only: no plan op in v0.3.0 */ }
            InternalPass::EmptyCleanup => pass_empty_cleanup(&mut b),
        }
    }

    let stats = compute_stats(&b.ops);
    BuiltPlan { ops: b.ops, stats }
}

/// Total descendant FILE bytes per folder id.
fn compute_folder_bytes(
    nodes: &[PlanNode],
    index_of: &HashMap<usize, usize>,
) -> HashMap<usize, u64> {
    let mut bytes: HashMap<usize, u64> = HashMap::new();
    for n in nodes {
        if n.kind != NodeKind::File {
            continue;
        }
        // Walk up the parent chain, attributing this file's bytes to every
        // ancestor folder.
        let mut cur = n.parent;
        let mut steps = 0;
        while let Some(p) = cur {
            if steps > nodes.len() {
                break;
            }
            steps += 1;
            *bytes.entry(p).or_default() += n.size;
            cur = nodes[index_of[&p]].parent;
        }
    }
    bytes
}

/// Detect the single scan-root folder id, if the snapshot uses the real
/// single-rooted convention (exactly one parentless entry, a folder). Returns
/// `None` for the multi-root fixture convention (loose files are parentless).
fn scan_root_id(nodes: &[PlanNode]) -> Option<usize> {
    let parentless: Vec<&PlanNode> = nodes.iter().filter(|n| n.parent.is_none()).collect();
    match parentless.as_slice() {
        [only] if only.kind == NodeKind::Folder => Some(only.id),
        _ => None,
    }
}

// ---- pass: staging-separation ----

fn pass_staging(b: &mut Builder) {
    let pass = InternalPass::StagingSeparation;
    let mut staging: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| b.class_of(n.id) == Some(FolderClass::Staging))
        .map(|n| n.id)
        .collect();
    staging.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));
    for id in staging {
        b.handled.insert(id);
        let path = b.node(id).path.clone();
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "no-op".to_string(),
            kind_reason: Some("staging".to_string()),
            source_path: path.clone(),
            target_path: String::new(),
            rationale:
                "This is a staging or inbox area; its contents are separated into books in a later step, not moved automatically now."
                    .to_string(),
            rule_id: "staging-area".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }
}

// ---- pass: loose-root-books ----

fn pass_loose_root(b: &mut Builder) {
    let pass = InternalPass::LooseRootBooks;
    let root_id = scan_root_id(b.nodes);

    // Loose audio files: audio files whose parent is the scan root (real
    // convention) or which are themselves parentless (fixture convention).
    let mut loose: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| {
            n.kind == NodeKind::File
                && n.file_class == Some(FileClass::Audio)
                && match root_id {
                    Some(r) => n.parent == Some(r),
                    None => n.parent.is_none(),
                }
        })
        .map(|n| n.id)
        .collect();
    loose.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

    let preset = b.ruleset.naming.preset;
    let width = b.ruleset.naming.series_index_width;

    for id in loose {
        let src = b.node(id).path.clone();
        let size = b.node(id).size;
        let fields = book_fields_from_file(b.merged_by_id.get(&id).copied());
        match render_path(preset, &fields.as_template(), width) {
            RenderOutcome::Rendered(components) => {
                let chain = b.dir_chain(&components, true);
                b.ensure_dirs(&chain, pass, "loose-root-books-mkdir");
                let target_dir = chain
                    .last()
                    .cloned()
                    .unwrap_or_else(|| b.library_root.clone());
                let file_name = base_name(&src, b.sep).to_string();
                let target = join_path(&target_dir, b.sep, &[file_name.as_str()]);
                b.ops.push(PlannedOp {
                    op_group: pass.as_str().to_string(),
                    kind: "move".to_string(),
                    kind_reason: None,
                    source_path: src,
                    target_path: target,
                    rationale: format!(
                        "This audiobook is sitting loose at the library root; move it into \"{}\".",
                        components.join(&b.sep.to_string())
                    ),
                    rule_id: "loose-root-books".to_string(),
                    confidence: confidence_str(fields.confidence).to_string(),
                    byte_size: size as i64,
                    provenance_json: None,
                });
            }
            RenderOutcome::ManualReview(reason) => {
                push_manual_review(b, pass, "loose-root-books-manual", &src, reason);
            }
        }
    }
}

// ---- pass: strip-noise ----

fn pass_strip_noise(b: &mut Builder) {
    let pass = InternalPass::StripNoise;
    // Folders kept in place whose name carries strippable noise: book / series
    // containers not otherwise relocated (not in a pack, not in staging, not a
    // multi-book folder about to be split, not already handled).
    let mut candidates: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Folder)
        .filter(|n| {
            matches!(
                b.class_of(n.id),
                Some(FolderClass::Book) | Some(FolderClass::SeriesContainer)
            )
        })
        .map(|n| n.id)
        .filter(|&id| {
            !b.handled.contains(&id)
                && !b.inside_pack(id)
                && !b.inside_staging(id)
                && has_noise(&b.node(id).name)
        })
        .collect();
    candidates.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

    for id in candidates {
        let src = b.node(id).path.clone();
        let name = b.node(id).name.clone();
        let cleaned = strip(&name, noise_strip_options()).text.trim().to_string();
        if cleaned.is_empty() || cleaned == name {
            continue;
        }
        b.handled.insert(id);
        b.relocated.insert(id);
        let parent = parent_dir(&src, b.sep).to_string();
        let target = join_path(&parent, b.sep, &[cleaned.as_str()]);
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "rename".to_string(),
            kind_reason: None,
            source_path: src,
            target_path: target,
            rationale: format!(
                "Remove ripper and size markers from the folder name, renaming it to \"{cleaned}\"."
            ),
            rule_id: "strip-noise".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }
}

// ---- pass: split-multi-book ----

fn pass_split_multibook(b: &mut Builder) {
    let pass = InternalPass::SplitMultiBook;
    let mut folders: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| b.class_of(n.id) == Some(FolderClass::MultiBookSuspect))
        .map(|n| n.id)
        .filter(|&id| !b.handled.contains(&id) && !b.inside_pack(id) && !b.inside_staging(id))
        .collect();
    folders.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

    let preset = b.ruleset.naming.preset;
    let width = b.ruleset.naming.series_index_width;

    for folder_id in folders {
        b.handled.insert(folder_id);
        let evidence = b.class_by_id.get(&folder_id).map(|c| &c.evidence);
        let clean_author = evidence.and_then(|e| e.clean_author.clone());
        let clean_series = evidence.and_then(|e| e.clean_series.clone());

        // Direct audio children, deterministic order.
        let mut audio_children: Vec<usize> = b
            .children
            .get(&folder_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| {
                b.node(c).kind == NodeKind::File && b.node(c).file_class == Some(FileClass::Audio)
            })
            .collect();
        audio_children.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

        let total = audio_children.len();
        let mut moved = 0usize;
        for file_id in audio_children {
            let src = b.node(file_id).path.clone();
            let size = b.node(file_id).size;
            let fields = book_fields_from_folder(
                b.merged_by_id.get(&file_id).copied(),
                clean_author.as_deref(),
                clean_series.as_deref(),
            );
            match render_path(preset, &fields.as_template(), width) {
                RenderOutcome::Rendered(components) => {
                    let chain = b.dir_chain(&components, true);
                    b.ensure_dirs(&chain, pass, "split-multi-book-mkdir");
                    let target_dir = chain
                        .last()
                        .cloned()
                        .unwrap_or_else(|| b.library_root.clone());
                    let file_name = base_name(&src, b.sep).to_string();
                    let target = join_path(&target_dir, b.sep, &[file_name.as_str()]);
                    b.ops.push(PlannedOp {
                        op_group: pass.as_str().to_string(),
                        kind: "move".to_string(),
                        kind_reason: None,
                        source_path: src,
                        target_path: target,
                        rationale: format!(
                            "This folder holds several complete books; move this one into its own folder \"{}\".",
                            components.join(&b.sep.to_string())
                        ),
                        rule_id: "split-multi-book".to_string(),
                        confidence: confidence_str(fields.confidence).to_string(),
                        byte_size: size as i64,
                        provenance_json: None,
                    });
                    moved += 1;
                }
                RenderOutcome::ManualReview(reason) => {
                    push_manual_review(b, pass, "split-multi-book-manual", &src, reason);
                }
            }
        }
        // If every book was placed, the now-empty folder can be removed.
        if total > 0 && moved == total {
            b.emptied_sources.insert(b.node(folder_id).path.clone());
        }
    }
}

// ---- pass: flatten-packs ----

fn pass_flatten_packs(b: &mut Builder) {
    let pass = InternalPass::FlattenPacks;
    let mut packs: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| b.class_of(n.id) == Some(FolderClass::PackContainer))
        .map(|n| n.id)
        // Parity with the other content passes: never touch a pack an earlier
        // pass already claimed, and never reach INTO a staging area. The staging
        // no-op (pass_staging) promises nothing under a `_sort`/inbox folder
        // moves now; without this guard a pack sitting inside staging would be
        // flattened anyway, leaking books out of the pile the report says is
        // left alone. A pack nested inside ANOTHER pack is deliberately NOT
        // skipped here (no `inside_pack` guard): flatten processes packs at
        // every depth, because an outer pack only emits a "nested collection,
        // needs manual review" no-op for an inner pack and never recurses into
        // it. Skipping nested packs would strand their members and drop their
        // F-507 provenance (e.g. the Hugo Collection pack under Genre - SciFI).
        .filter(|&id| !b.handled.contains(&id) && !b.inside_staging(id))
        .collect();
    packs.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

    let preset = b.ruleset.naming.preset;
    let width = b.ruleset.naming.series_index_width;

    for pack_id in packs {
        let pack_path = b.node(pack_id).path.clone();
        let pack_name = b.node(pack_id).name.clone();

        // Direct child folders of the pack, deterministic order.
        let mut items: Vec<usize> = b
            .children
            .get(&pack_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| b.node(c).kind == NodeKind::Folder)
            .collect();
        items.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

        let total = items.len();
        let mut planned = 0usize;
        for item_id in items {
            let item_class = b.class_of(item_id);
            let is_book = matches!(
                item_class,
                Some(FolderClass::Book) | Some(FolderClass::MultiBookSuspect)
            );
            let src = b.node(item_id).path.clone();
            let item_name = b.node(item_id).name.clone();
            let size = b.folder_bytes.get(&item_id).copied().unwrap_or(0);

            if !is_book {
                // A nested container/other: not flattened in this phase, so the
                // pack is only partially extracted. Documented, not guessed.
                b.ops.push(PlannedOp {
                    op_group: pass.as_str().to_string(),
                    kind: "no-op".to_string(),
                    kind_reason: Some("manual-review".to_string()),
                    source_path: src,
                    target_path: String::new(),
                    rationale:
                        "This item inside the collection bundle is itself a collection; it needs manual review before it can be extracted."
                            .to_string(),
                    rule_id: "flatten-packs-nested".to_string(),
                    confidence: "medium".to_string(),
                    byte_size: 0,
                    provenance_json: None,
                });
                continue;
            }

            let evidence = b.class_by_id.get(&item_id).map(|c| &c.evidence);
            let clean_author = evidence.and_then(|e| e.clean_author.clone());
            let clean_series = evidence.and_then(|e| e.clean_series.clone());
            let fields = book_fields_from_folder(
                b.merged_by_id.get(&item_id).copied(),
                clean_author.as_deref(),
                clean_series.as_deref(),
            );
            match render_path(preset, &fields.as_template(), width) {
                RenderOutcome::Rendered(components) => {
                    b.handled.insert(item_id);
                    b.relocated.insert(item_id);
                    let chain = b.dir_chain(&components, false);
                    b.ensure_dirs(&chain, pass, "flatten-packs-mkdir");
                    let comp_refs: Vec<&str> = components.iter().map(|s| s.as_str()).collect();
                    let target = join_path(&b.library_root, b.sep, &comp_refs);
                    let award = if item_name.contains('^') {
                        Some('^')
                    } else {
                        None
                    };
                    let provenance = pack_provenance_json(&pack_path, &pack_name, award);
                    b.ops.push(PlannedOp {
                        op_group: pass.as_str().to_string(),
                        kind: "move".to_string(),
                        kind_reason: None,
                        source_path: src,
                        target_path: target,
                        rationale: format!(
                            "Extract this book from the collection bundle \"{pack_name}\" into \"{}\".",
                            components.join(&b.sep.to_string())
                        ),
                        rule_id: "flatten-packs".to_string(),
                        confidence: confidence_str(fields.confidence).to_string(),
                        byte_size: size as i64,
                        provenance_json: Some(provenance),
                    });
                    planned += 1;
                }
                RenderOutcome::ManualReview(reason) => {
                    push_manual_review(b, pass, "flatten-packs-manual", &src, reason);
                }
            }
        }

        // AC-5: quarantine the emptied shell only when EVERY member was planned
        // (and the policy asks for it); otherwise leave it in place so no
        // un-extracted book is stranded.
        let all_planned = total > 0 && planned == total;
        if let PackShellOutcome::Quarantine =
            pack_shell_disposition(b.ruleset.structure.pack_shell, all_planned)
        {
            b.quarantine_shells.push((pack_id, pack_path));
        }
    }
}

/// The F-507 provenance object recorded on each flatten-packs member move.
/// Keys serialize in sorted order (serde_json default map), so the string is
/// deterministic.
fn pack_provenance_json(pack_path: &str, pack_name: &str, award: Option<char>) -> String {
    let value = match award {
        Some(marker) => serde_json::json!({
            "pack_path": pack_path,
            "pack_name": pack_name,
            "award_marker": marker.to_string(),
        }),
        None => serde_json::json!({
            "pack_path": pack_path,
            "pack_name": pack_name,
        }),
    };
    serde_json::to_string(&value).expect("provenance JSON always serializes")
}

// ---- pass: normalize-series (F-204 disc renames) ----

fn pass_normalize_series(b: &mut Builder) {
    let pass = InternalPass::NormalizeSeries;
    // Every folder the classifier marked a disc part (`detected == "disc-part"`,
    // set on a single-book audio-leaf folder whose name is a disc name). A
    // disc-split single book itself is `detected == "disc-split"` and is NOT a
    // disc part, so it is not touched here.
    let mut discs: Vec<usize> = b
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Folder)
        .filter(|n| b.class_by_id.get(&n.id).and_then(|c| c.evidence.detected) == Some("disc-part"))
        .map(|n| n.id)
        .filter(|&id| !b.handled.contains(&id) && !b.ancestor_handled(id))
        .collect();
    discs.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));

    for id in discs {
        let name = b.node(id).name.clone();
        let Some(target_name) = conformant_disc_rename(&name) else {
            continue; // already conformant: leave untouched (AC-32)
        };
        b.handled.insert(id);
        b.relocated.insert(id);
        let src = b.node(id).path.clone();
        let parent = parent_dir(&src, b.sep).to_string();
        let target = join_path(&parent, b.sep, &[target_name.as_str()]);
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "rename".to_string(),
            kind_reason: None,
            source_path: src,
            target_path: target,
            rationale: format!(
                "Rename this disc folder to the standard \"{target_name}\" shape so it is recognized as a disc of one book."
            ),
            rule_id: "normalize-series-disc".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }
}

// ---- pass: empty-cleanup (removals + set-aside, strictly after all moves) ----

/// The extension (no dot) of the preferred canonical audio format.
fn preferred_ext(preferred: PreferredFormat) -> &'static str {
    match preferred {
        PreferredFormat::M4b => "m4b",
        PreferredFormat::Mp3 => "mp3",
    }
}

/// Whether an audio file NAME is the preferred canonical format.
fn is_preferred_audio(name: &str, preferred: PreferredFormat) -> bool {
    name.to_ascii_lowercase()
        .ends_with(&format!(".{}", preferred_ext(preferred)))
}

/// Whether `folder_id` has a direct child folder named `0 <ext>` (the Hugo-pack
/// `0 M4B` convention) that holds a direct preferred-format audio file. This is
/// how a book keeps its preferred copy in a sibling subfolder while the loser
/// chapters sit at the folder root.
fn zero_format_child_has_preferred(
    b: &Builder,
    folder_id: usize,
    preferred: PreferredFormat,
) -> bool {
    let marker = format!("0 {}", preferred_ext(preferred));
    let Some(kids) = b.children.get(&folder_id) else {
        return false;
    };
    for &c in kids {
        if b.node(c).kind != NodeKind::Folder {
            continue;
        }
        if !b.node(c).name.trim().eq_ignore_ascii_case(&marker) {
            continue;
        }
        if let Some(grandkids) = b.children.get(&c) {
            if grandkids.iter().any(|&g| {
                b.node(g).kind == NodeKind::File
                    && b.node(g).file_class == Some(FileClass::Audio)
                    && is_preferred_audio(&b.node(g).name, preferred)
            }) {
                return true;
            }
        }
    }
    false
}

/// The audio stem of a file name (lowercased, extension removed). Used to match
/// a non-preferred copy to its preferred twin in the direct-sibling case.
fn audio_stem(name: &str) -> String {
    const EXTS: &[&str] = &[".m4b", ".mp3", ".m4a", ".opus", ".wma", ".flac"];
    let lower = name.to_ascii_lowercase();
    for ext in EXTS {
        if let Some(stripped) = lower.strip_suffix(ext) {
            return stripped.to_string();
        }
    }
    lower
}

/// F-205: the non-preferred-format audio files to set aside (keep the preferred
/// copy, AC-33). Two precise shapes are recognized so unrelated same-folder
/// files are never mistaken for a parallel format:
///   - **`0 M4B` convention**: a folder holds direct non-preferred chapters AND
///     a `0 <ext>` child folder with the combined preferred copy (the Hugo-pack
///     case). Every direct non-preferred audio file is a loser.
///   - **direct twin**: a non-preferred audio file has a direct preferred-format
///     sibling with the SAME stem (`Book.mp3` beside `Book.m4b`). Only the
///     matched non-preferred file is a loser, so `CON.mp3` beside an unrelated
///     `COM1.m4b` is never flagged.
///
/// Returns file ids sorted by path (determinism).
fn detect_parallel_format_losers(b: &Builder) -> Vec<usize> {
    let preferred = b.ruleset.structure.preferred_format;
    let mut losers: Vec<usize> = Vec::new();
    for n in b.nodes {
        if n.kind != NodeKind::Folder {
            continue;
        }
        let id = n.id;
        if matches!(
            b.class_of(id),
            Some(FolderClass::Staging) | Some(FolderClass::ManualReview)
        ) || b.inside_staging(id)
        {
            continue;
        }
        let Some(kids) = b.children.get(&id) else {
            continue;
        };
        let direct_audio: Vec<usize> = kids
            .iter()
            .copied()
            .filter(|&c| {
                b.node(c).kind == NodeKind::File && b.node(c).file_class == Some(FileClass::Audio)
            })
            .collect();
        let non_preferred: Vec<usize> = direct_audio
            .iter()
            .copied()
            .filter(|&c| !is_preferred_audio(&b.node(c).name, preferred))
            .collect();
        if non_preferred.is_empty() {
            continue;
        }

        if zero_format_child_has_preferred(b, id, preferred) {
            // The combined preferred copy lives in a `0 <ext>` subfolder; every
            // direct non-preferred chapter is a loser.
            losers.extend(non_preferred);
            continue;
        }

        // Direct-twin case: a non-preferred file whose stem matches a direct
        // preferred sibling's stem.
        let preferred_stems: std::collections::HashSet<String> = direct_audio
            .iter()
            .filter(|&&c| is_preferred_audio(&b.node(c).name, preferred))
            .map(|&c| audio_stem(&b.node(c).name))
            .collect();
        if preferred_stems.is_empty() {
            continue;
        }
        for c in non_preferred {
            if preferred_stems.contains(&audio_stem(&b.node(c).name)) {
                losers.push(c);
            }
        }
    }
    losers.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));
    losers
}

/// F-402: the non-audio clutter files the ruleset sets aside
/// (nfo/sfv/playlist/weblink by default; ebook/cover are kept). Skips files in a
/// staging area or under a folder MOVED/RENAMED as a unit by an earlier pass
/// (that clutter rides along physically, so a separate set-aside from its stale
/// path would be wrong). Clutter under a split multi-book folder (emptied in
/// place, not relocated) is NOT skipped: it is left behind and set aside,
/// riding the Box sets group per FD-31. Returns file ids sorted by path.
fn detect_clutter_quarantine(b: &Builder) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for n in b.nodes {
        if n.kind != NodeKind::File {
            continue;
        }
        let id = n.id;
        let Some(kind) = clutter_kind_from_name(&n.name) else {
            continue;
        };
        if b.ruleset.structure.clutter.action_for(kind) != ClutterAction::Quarantine {
            continue;
        }
        if b.inside_staging(id) || b.ancestor_relocated(id) {
            continue;
        }
        out.push(id);
    }
    out.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));
    out
}

/// The per-job set-aside folder every set-aside target lands under (FD-34):
/// `<set_aside_root>\{job-id}\`. The `{job-id}` segment is the
/// [`QUARANTINE_JOB_PLACEHOLDER`] the executor substitutes at apply time.
fn set_aside_job_dir(b: &Builder) -> String {
    join_path(&b.set_aside_root, b.sep, &[QUARANTINE_JOB_PLACEHOLDER])
}

/// The FD-34 set-aside target for an item at `source_path`:
/// `<set_aside_root>\{job-id}\<original relative path>` (AC-21). The original
/// relative path is preserved under the per-job folder so a set-aside item is
/// restorable to exactly where it came from.
fn set_aside_target(b: &Builder, source_path: &str) -> String {
    let rel = rel_under_root(source_path, &b.library_root, b.sep);
    join_path(&set_aside_job_dir(b), b.sep, &[rel])
}

/// Schedule the FD-34 per-job set-aside folder (`<set_aside_root>\{job-id}\`) for
/// creation, emitting one `mkdir` the first time any set-aside op in `pass` needs
/// it. Deduplicated via the shared `created_dirs` set, so the mkdir appears
/// exactly once and always before the first move into it. Deeper set-aside
/// parents (each item's own relative subpath) are created by the executor's
/// mkdir-first step, exactly as for every other move target.
fn ensure_quarantine_dir(b: &mut Builder, pass: InternalPass) {
    let job_dir = set_aside_job_dir(b);
    if !b.created_dirs.contains(&job_dir) {
        b.created_dirs.insert(job_dir.clone());
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "mkdir".to_string(),
            kind_reason: None,
            source_path: String::new(),
            target_path: job_dir,
            rationale:
                "Create the \"Set Aside\" folder so items can be set aside without deleting anything."
                    .to_string(),
            rule_id: "empty-cleanup-mkdir".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }
}

fn pass_empty_cleanup(b: &mut Builder) {
    let pass = InternalPass::EmptyCleanup;

    // 1. Quarantine emptied pack shells (deterministic order by path).
    let mut shells = std::mem::take(&mut b.quarantine_shells);
    shells.sort_by(|(_, a), (_, c)| a.cmp(c));
    for (_, shell_path) in shells {
        let name = base_name(&shell_path, b.sep).to_string();
        ensure_quarantine_dir(b, pass);
        let target = set_aside_target(b, &shell_path);
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "quarantine".to_string(),
            kind_reason: None,
            source_path: shell_path,
            target_path: target,
            rationale: format!(
                "The collection bundle \"{name}\" is empty after its books were extracted; set the empty shell aside (never deleted)."
            ),
            rule_id: "flatten-packs-shell".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }

    // 2. F-205 parallel-format losers: set aside the non-preferred copy (keep
    // the m4b, AC-33). Detected here so the set-aside follows every move, but
    // grouped under Copies (FD-31: the non-preferred format is a copy of the
    // book), which is the dedupe-quarantine pass's user-facing group.
    let losers = detect_parallel_format_losers(b);
    for id in losers {
        let src = b.node(id).path.clone();
        let name = base_name(&src, b.sep).to_string();
        let size = b.node(id).size;
        ensure_quarantine_dir(b, pass);
        let target = set_aside_target(b, &src);
        b.ops.push(PlannedOp {
            op_group: InternalPass::DedupeQuarantine.as_str().to_string(),
            kind: "quarantine".to_string(),
            kind_reason: None,
            source_path: src,
            target_path: target,
            rationale: format!(
                "This book keeps a preferred {} copy, so the extra \"{name}\" copy is set aside (never deleted).",
                preferred_ext(b.ruleset.structure.preferred_format)
            ),
            rule_id: "parallel-format-quarantine".to_string(),
            confidence: "high".to_string(),
            byte_size: size as i64,
            provenance_json: None,
        });
    }

    // 3. F-402 non-audio clutter: set aside nfo/sfv/playlist/weblink; ebook and
    // cover are kept with their book (no op).
    let clutter = detect_clutter_quarantine(b);
    for id in clutter {
        let src = b.node(id).path.clone();
        let name = base_name(&src, b.sep).to_string();
        let size = b.node(id).size;
        // FD-31: the set-aside op inherits the user-facing group of the pass
        // that owns its parent folder's transformation. The op still executes in
        // empty-cleanup order (strictly after every move); only its op_group tag
        // (the display fold) reflects the owning pass.
        let owning = b.clutter_owning_pass(id);
        ensure_quarantine_dir(b, pass);
        let target = set_aside_target(b, &src);
        b.ops.push(PlannedOp {
            op_group: owning.as_str().to_string(),
            kind: "quarantine".to_string(),
            kind_reason: None,
            source_path: src,
            target_path: target,
            rationale: format!(
                "\"{name}\" is not an audiobook file; set it aside (never deleted)."
            ),
            rule_id: "clutter-quarantine".to_string(),
            confidence: "high".to_string(),
            byte_size: size as i64,
            provenance_json: None,
        });
    }

    // 4. Remove folders emptied by a split (deterministic; BTreeSet is sorted).
    if b.ruleset.structure.empty_folder_removal {
        let emptied = std::mem::take(&mut b.emptied_sources);
        for path in emptied {
            b.ops.push(PlannedOp {
                op_group: pass.as_str().to_string(),
                kind: "rmdir-empty".to_string(),
                kind_reason: None,
                source_path: path.clone(),
                target_path: path,
                rationale:
                    "This folder is empty after its books were moved out; remove the empty folder."
                        .to_string(),
                rule_id: "empty-cleanup-emptied".to_string(),
                confidence: "high".to_string(),
                byte_size: 0,
                provenance_json: None,
            });
        }

        // 5. Folders classified empty in the snapshot.
        let mut empties: Vec<usize> = b
            .nodes
            .iter()
            .filter(|n| b.class_of(n.id) == Some(FolderClass::Empty))
            .map(|n| n.id)
            .filter(|&id| !b.handled.contains(&id))
            .collect();
        empties.sort_by(|&a, &c| b.node(a).path.cmp(&b.node(c).path));
        for id in empties {
            b.handled.insert(id);
            let path = b.node(id).path.clone();
            b.ops.push(PlannedOp {
                op_group: pass.as_str().to_string(),
                kind: "rmdir-empty".to_string(),
                kind_reason: None,
                source_path: path.clone(),
                target_path: path,
                rationale:
                    "This folder holds no audiobooks and no other content; remove the empty folder."
                        .to_string(),
                rule_id: "empty-cleanup".to_string(),
                confidence: "high".to_string(),
                byte_size: 0,
                provenance_json: None,
            });
        }
    }
}

// ---- shared helpers ----

fn push_manual_review(
    b: &mut Builder,
    pass: InternalPass,
    rule_id: &str,
    src: &str,
    reason: ManualReviewReason,
) {
    b.ops.push(PlannedOp {
        op_group: pass.as_str().to_string(),
        kind: "no-op".to_string(),
        kind_reason: Some("manual-review".to_string()),
        source_path: src.to_string(),
        target_path: String::new(),
        rationale: manual_review_note(reason).to_string(),
        rule_id: rule_id.to_string(),
        confidence: "low".to_string(),
        byte_size: 0,
        provenance_json: None,
    });
}

fn compute_stats(ops: &[PlannedOp]) -> PlanStats {
    let mut per_group: HashMap<CampaignGroup, u64> = HashMap::new();
    let mut manual_review_ops = 0u64;
    for op in ops {
        if let Some(pass) = pass_from_str(&op.op_group) {
            *per_group.entry(pass.group()).or_default() += 1;
        }
        if op.kind == "no-op" && op.kind_reason.as_deref() == Some("manual-review") {
            manual_review_ops += 1;
        }
    }
    let per_group = CampaignGroup::ALL
        .iter()
        .map(|&g| GroupCount {
            group: g.label().to_string(),
            ops: per_group.get(&g).copied().unwrap_or(0),
        })
        .collect();
    PlanStats {
        total_ops: ops.len() as u64,
        manual_review_ops,
        per_group,
    }
}

/// Recover the [`InternalPass`] from its stored kebab string.
pub fn pass_from_str(s: &str) -> Option<InternalPass> {
    InternalPass::ALL.iter().copied().find(|p| p.as_str() == s)
}

/// The user-facing campaign group for a stored `op_group` string (the FD-26
/// fold the export and report apply). `None` for an unrecognized string.
pub fn group_for_op_group(op_group: &str) -> Option<CampaignGroup> {
    pass_from_str(op_group).map(|p| p.group())
}

// ---- persistence (thin, separate from the pure builder) ----

/// TEST-ONLY persistence shim: insert a built plan with every op's
/// `validation_state` frozen at `"pending"` and NO update path, via
/// [`crate::db::plans::insert_plan`]. Because migration 0002 freezes the
/// descriptive/validation columns at insert time, a plan persisted this way is
/// stuck `pending` forever. That is acceptable for a round-trip unit test but
/// would be a foot-gun in production, so this is `#[cfg(test)]` and named with a
/// `_for_tests` suffix: the ONLY production persistence path is
/// [`crate::plan::validate::persist_validated_plan`], which writes real verdicts
/// once. Serializes [`PlanStats`] into `plans.stats_json`.
#[cfg(test)]
pub async fn persist_plan_for_tests(
    pool: &sqlx::SqlitePool,
    scan_id: i64,
    ruleset_id: i64,
    status: &str,
    built: &BuiltPlan,
    now: &str,
) -> Result<i64, sqlx::Error> {
    use crate::db::plans::{insert_plan, NewPlan, NewPlanOp};

    let stats_json = serde_json::to_string(&built.stats).expect("stats always serialize");
    let ops: Vec<NewPlanOp> = built
        .ops
        .iter()
        .map(|op| NewPlanOp {
            op_group: op.op_group.as_str(),
            kind: op.kind.as_str(),
            kind_reason: op.kind_reason.as_deref(),
            source_path: op.source_path.as_str(),
            target_path: op.target_path.as_str(),
            rationale: op.rationale.as_str(),
            rule_id: op.rule_id.as_str(),
            confidence: op.confidence.as_str(),
            byte_size: op.byte_size,
            validation_state: PlannedOp::VALIDATION_PENDING,
            validation_reason: None,
            provenance_json: op.provenance_json.as_deref(),
        })
        .collect();

    insert_plan(
        pool,
        &NewPlan {
            scan_id,
            ruleset_id,
            status,
            stats_json: Some(&stats_json),
        },
        &ops,
        now,
    )
    .await
}

/// Adapt persisted snapshot rows into [`PlanNode`]s (the real product path from
/// a stored scan to the plan builder). Mirrors
/// [`crate::classify::inputs_from_snapshot`] but keeps the full path each op
/// needs. File classes are re-derived from the name via the F-103 typing table,
/// exactly as the classification bridge does.
pub fn plan_nodes_from_snapshot(rows: &[crate::ipc::EntryRow]) -> Vec<PlanNode> {
    use crate::scan::typing::classify_path;
    use std::path::Path;

    rows.iter()
        .map(|r| {
            let is_dir = r.kind == "dir";
            PlanNode {
                id: r.id as usize,
                parent: r.parent_id.map(|p| p as usize),
                name: r.name.clone(),
                path: r.path.clone(),
                kind: if is_dir {
                    NodeKind::Folder
                } else {
                    NodeKind::File
                },
                file_class: if is_dir {
                    None
                } else {
                    Some(classify_path(Path::new(&r.name)))
                },
                size: r.size.max(0) as u64,
            }
        })
        .collect()
}

/// Build a [`ClassifyInput`] view from [`PlanNode`]s so a caller with plan nodes
/// can classify them without re-reading the snapshot.
pub fn classify_inputs_from_plan_nodes(nodes: &[PlanNode]) -> Vec<ClassifyInput> {
    nodes
        .iter()
        .map(|n| ClassifyInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
            file_class: n.file_class,
            size: n.size,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::parse::extract::{extract, EntryInput};
    use crate::ruleset::default_ruleset;

    const ROOT: &str = "lib";

    fn leaf(path: &str) -> String {
        path.rsplit('/').next().unwrap().to_string()
    }

    fn dir(id: usize, parent: Option<usize>, path: &str) -> PlanNode {
        PlanNode {
            id,
            parent,
            name: leaf(path),
            path: path.to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }
    }

    fn aud(id: usize, parent: Option<usize>, path: &str, size: u64) -> PlanNode {
        PlanNode {
            id,
            parent,
            name: leaf(path),
            path: path.to_string(),
            kind: NodeKind::File,
            file_class: Some(FileClass::Audio),
            size,
        }
    }

    /// Classify + merge the nodes exactly as the real pipeline would.
    fn analyze(nodes: &[PlanNode]) -> (Vec<FolderClassification>, Vec<MergedEntry>) {
        let inputs = classify_inputs_from_plan_nodes(nodes);
        let classifications = classify(&inputs);
        let entry_inputs: Vec<EntryInput> = nodes
            .iter()
            .map(|n| EntryInput {
                id: n.id,
                parent: n.parent,
                name: n.name.clone(),
                kind: n.kind,
            })
            .collect();
        let merged = extract(&entry_inputs);
        (classifications, merged)
    }

    fn build(nodes: &[PlanNode]) -> BuiltPlan {
        let (cs, merged) = analyze(nodes);
        build_plan(nodes, &cs, &merged, &default_ruleset(), ROOT)
    }

    /// Build with every clutter kind set to `Quarantine`.
    ///
    /// FD-40 flipped the DEFAULT clutter policy to Keep, so the clutter tests
    /// below can no longer lean on `default_ruleset()`. That is the point: they
    /// test where a clutter set-aside op is ROUTED once one is emitted, not
    /// whether the default emits one. Asking for the policy explicitly makes
    /// each test say what it is actually about, and means a future change to
    /// the default cannot silently turn these into no-op assertions.
    fn build_clutter_quarantined(nodes: &[PlanNode]) -> BuiltPlan {
        let (cs, merged) = analyze(nodes);
        let mut rs = default_ruleset();
        rs.structure.clutter = crate::ruleset::ClutterPolicy {
            ebook: crate::ruleset::ClutterAction::Keep,
            cover: crate::ruleset::ClutterAction::Keep,
            nfo: crate::ruleset::ClutterAction::Quarantine,
            sfv: crate::ruleset::ClutterAction::Quarantine,
            playlist: crate::ruleset::ClutterAction::Quarantine,
            weblink: crate::ruleset::ClutterAction::Quarantine,
        };
        build_plan(nodes, &cs, &merged, &rs, ROOT)
    }

    fn ops_of<'a>(plan: &'a BuiltPlan, group: &str) -> Vec<&'a PlannedOp> {
        plan.ops.iter().filter(|o| o.op_group == group).collect()
    }

    // ---- FD-34: set-aside root resolves OUTSIDE the library (AC-21). --------

    /// The FD-34 default set-aside root is a `Set Aside` SIBLING of the library,
    /// one level up, on the library's own volume (so moves stay rename-first).
    #[test]
    fn default_set_aside_root_is_a_sibling_outside_the_library() {
        // A real Windows root: the sibling sits at the drive root beside it.
        assert_eq!(default_set_aside_root(r"E:\Books - Audio"), r"E:\Set Aside");
        // A nested real root: the sibling sits beside the library folder.
        assert_eq!(
            default_set_aside_root(r"E:\Media\Books"),
            r"E:\Media\Set Aside"
        );
        // A POSIX/synthetic root keeps forward slashes.
        assert_eq!(default_set_aside_root("E:/Library"), "E:/Set Aside");
        // A bare synthetic root (the fixture convention) has no parent, so the
        // sibling is the bare name at the same top level.
        assert_eq!(default_set_aside_root("lib"), "Set Aside");
    }

    /// FD-34 end to end: with a real drive-letter library root, every set-aside
    /// target lands OUTSIDE the library root, under the per-job folder, on the
    /// same volume (D-08 rename-first), preserving the original relative path.
    #[test]
    fn set_aside_targets_land_outside_the_real_library_root() {
        let root = "E:/Books";
        let nodes = vec![
            dir(0, None, "E:/Books/Some Book"),
            file(1, Some(0), "E:/Books/Some Book/track01.mp3", 40_000),
            file(2, Some(0), "E:/Books/Some Book/track02.mp3", 40_000),
            dir(3, Some(0), "E:/Books/Some Book/0 M4B"),
            file(4, Some(3), "E:/Books/Some Book/0 M4B/Some Book.m4b", 80_000),
        ];
        let (cs, merged) = analyze(&nodes);
        let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), root);
        let losers: Vec<&PlannedOp> = plan.ops.iter().filter(|o| o.kind == "quarantine").collect();
        assert_eq!(losers.len(), 2, "both mp3 losers are set aside");
        for q in &losers {
            // Outside the library root (FD-34), under the sibling set-aside root.
            assert!(
                !q.target_path.starts_with("E:/Books/"),
                "a set-aside target must be OUTSIDE the library root: {}",
                q.target_path
            );
            assert!(
                q.target_path.starts_with("E:/Set Aside/{job-id}/"),
                "target under the per-job set-aside folder: {}",
                q.target_path
            );
            // Same volume as the library (D-08 rename-first stays valid).
            assert!(q.target_path.starts_with("E:/"));
            // AC-21: original relative path preserved under the job folder.
            let rel = &q.source_path[root.len() + 1..];
            assert!(
                q.target_path.ends_with(rel),
                "original relative path preserved: {} -> {}",
                q.source_path,
                q.target_path
            );
        }
    }

    // ---- AC-11: eight internal passes fold to exactly seven groups. --------

    #[test]
    fn eight_passes_fold_to_seven_canonical_groups() {
        assert_eq!(InternalPass::ALL.len(), 8);

        let groups: BTreeSet<&str> = InternalPass::ALL
            .iter()
            .map(|p| p.group().label())
            .collect();
        assert_eq!(groups.len(), 7, "exactly seven distinct user-facing groups");

        let canonical: BTreeSet<&str> = CampaignGroup::ALL.iter().map(|g| g.label()).collect();
        assert_eq!(
            groups, canonical,
            "the fold's label set must equal the FD-26 canon"
        );
        // The two passes that share a group are strip-noise and normalize-series.
        assert_eq!(
            InternalPass::StripNoise.group(),
            InternalPass::NormalizeSeries.group()
        );
        assert_eq!(InternalPass::StripNoise.group().label(), "messy names");
    }

    #[test]
    fn campaign_group_labels_are_the_fd26_canon() {
        let labels: Vec<&str> = CampaignGroup::ALL.iter().map(|g| g.label()).collect();
        assert_eq!(
            labels,
            vec![
                "staging",
                "loose books",
                "messy names",
                "box sets",
                "bundles",
                "copies",
                "empty folders",
            ]
        );
    }

    /// SPEC guard (PR-B regression): the EXACT pass-to-group fold for every
    /// internal pass, not just the label set. The prior label-set-only test let
    /// an inverted SplitMultiBook<->FlattenPacks mapping sail through, so this
    /// pins each pass to its FD-26 group by identity.
    #[test]
    fn every_internal_pass_folds_to_its_exact_group() {
        use CampaignGroup::*;
        use InternalPass::*;
        let expected: &[(InternalPass, CampaignGroup)] = &[
            (StagingSeparation, Staging),
            (LooseRootBooks, LooseBooks),
            (StripNoise, MessyNames),
            // A multi-book folder is the "box set" the user splits.
            (SplitMultiBook, BoxSets),
            // A pack/collection container is the "collection bundle" unpacked.
            (FlattenPacks, Bundles),
            (NormalizeSeries, MessyNames),
            (DedupeQuarantine, Copies),
            (EmptyCleanup, EmptyFolders),
        ];
        // Every pass is covered exactly once.
        assert_eq!(expected.len(), InternalPass::ALL.len());
        for &(pass, group) in expected {
            assert_eq!(
                pass.group(),
                group,
                "{} must fold into {:?}",
                pass.as_str(),
                group.label()
            );
        }
    }

    // ---- AC-8: determinism (same input twice = byte-identical plan). -------

    #[test]
    fn plan_is_byte_identical_across_two_runs() {
        let nodes = flatten_fixture();
        let a = build(&nodes);
        let b = build(&nodes);
        assert_eq!(
            a.to_canonical_json(),
            b.to_canonical_json(),
            "the serialized plan must be byte-identical across runs"
        );
    }

    // ---- AC-9: every op carries the required fields. -----------------------

    #[test]
    fn every_op_carries_the_required_fields() {
        let nodes = flatten_fixture();
        let plan = build(&nodes);
        assert!(!plan.ops.is_empty());
        for op in &plan.ops {
            assert!(!op.op_group.is_empty(), "op_group present");
            assert!(
                pass_from_str(&op.op_group).is_some(),
                "op_group is a known pass"
            );
            assert!(!op.kind.is_empty(), "kind present");
            assert!(!op.rationale.is_empty(), "rationale sentence present");
            assert!(
                op.rationale.trim_end().ends_with('.'),
                "rationale reads as a sentence: {:?}",
                op.rationale
            );
            assert!(!op.rule_id.is_empty(), "rule id present");
            assert!(
                matches!(op.confidence.as_str(), "high" | "medium" | "low"),
                "confidence is a known tier: {:?}",
                op.confidence
            );
            assert!(op.byte_size >= 0, "byte size present and non-negative");
            // A no-op deliberately has no target; every concrete op has one.
            if op.kind != "no-op" {
                assert!(
                    !op.target_path.is_empty(),
                    "concrete op {:?} must carry a target",
                    op.kind
                );
            }
            // A move carries a source and a target that differ.
            if op.kind == "move" {
                assert!(!op.source_path.is_empty());
                assert_ne!(op.source_path, op.target_path);
            }
        }
    }

    // ---- AC-10: dependency ordering on a nested fixture. -------------------

    #[test]
    fn dependency_ordering_holds_on_a_nested_fixture() {
        let nodes = flatten_fixture();
        let plan = build(&nodes);

        // Every existing folder is "created"; each mkdir adds one.
        let mut created: HashSet<String> = nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Folder)
            .map(|n| n.path.clone())
            .collect();

        // Track when a folder is removed/quarantined so a later move out of it
        // would be a violation.
        let mut removed: Vec<String> = Vec::new();

        for op in &plan.ops {
            match op.kind.as_str() {
                "mkdir" => {
                    // Parent must already exist.
                    let parent = parent_dir(&op.target_path, '/').to_string();
                    assert!(
                        created.contains(&parent) || parent == ROOT || parent == op.target_path,
                        "mkdir {:?} precedes creation of its parent {:?}",
                        op.target_path,
                        parent
                    );
                    created.insert(op.target_path.clone());
                }
                "move" => {
                    let parent = parent_dir(&op.target_path, '/').to_string();
                    assert!(
                        created.contains(&parent),
                        "move into {:?} precedes creation of {:?}",
                        op.target_path,
                        parent
                    );
                    // The source must not sit inside an already-removed folder.
                    for r in &removed {
                        assert!(
                            !op.source_path.starts_with(&format!("{r}/")),
                            "move out of {:?} occurs after it was removed ({r})",
                            op.source_path
                        );
                    }
                }
                "rmdir-empty" | "quarantine" => {
                    removed.push(op.source_path.clone());
                }
                _ => {}
            }
        }
    }

    // ---- loose-root-books ---------------------------------------------------

    #[test]
    fn loose_root_audio_folds_into_an_author_folder() {
        let nodes = vec![
            aud(0, None, "lib/Sapiens by Yuval Noah Harari.m4b", 180_000),
            aud(
                1,
                None,
                "lib/FREE SAMPLE_ The Martian by Andy Weir.m4b",
                2_700,
            ),
        ];
        let plan = build(&nodes);
        let loose = ops_of(&plan, "loose-root-books");

        // The clean one produces a mkdir chain + a move; the no-author sample
        // produces a manual-review no-op.
        let moves: Vec<&&PlannedOp> = loose.iter().filter(|o| o.kind == "move").collect();
        assert_eq!(moves.len(), 1);
        let mv = moves[0];
        assert_eq!(mv.rule_id, "loose-root-books");
        assert!(
            mv.target_path.starts_with("lib/Yuval Noah Harari/Sapiens/"),
            "target: {}",
            mv.target_path
        );
        assert_eq!(mv.byte_size, 180_000);

        let manual: Vec<&&PlannedOp> = loose.iter().filter(|o| o.kind == "no-op").collect();
        assert_eq!(manual.len(), 1);
        assert_eq!(manual[0].kind_reason.as_deref(), Some("manual-review"));
    }

    // ---- strip-noise --------------------------------------------------------

    #[test]
    fn noisy_book_folder_name_is_renamed_in_place() {
        let nodes = vec![
            dir(
                0,
                None,
                "lib/Jim Butcher - Cold Days [320k 18;48;03 2.52GB]",
            ),
            aud(
                1,
                Some(0),
                "lib/Jim Butcher - Cold Days [320k 18;48;03 2.52GB]/Cold Days.m4b",
                100_000,
            ),
        ];
        let plan = build(&nodes);
        let strip = ops_of(&plan, "strip-noise");
        assert_eq!(strip.len(), 1);
        assert_eq!(strip[0].kind, "rename");
        assert_eq!(strip[0].rule_id, "strip-noise");
        assert!(
            !strip[0].target_path.contains('['),
            "renamed target still noisy: {}",
            strip[0].target_path
        );
    }

    // ---- flatten-packs + F-507 provenance + AC-5 --------------------------

    /// A pack container with two clean, distinct-author book members and a
    /// deliberately unplaceable third member (no author): the two clean books
    /// flatten out with provenance, and because the third cannot be placed the
    /// shell is NOT quarantined (AC-5 partial-extraction guard).
    fn flatten_fixture() -> Vec<PlanNode> {
        vec![
            dir(0, None, "lib/Hugo Collection"),
            dir(
                1,
                Some(0),
                "lib/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season",
            ),
            aud(
                2,
                Some(1),
                "lib/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
            dir(
                3,
                Some(0),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood",
            ),
            aud(
                4,
                Some(3),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000,
            ),
            // A bare-titled member with no author: cannot be placed.
            dir(5, Some(0), "lib/Hugo Collection/Some Bare Title"),
            aud(
                6,
                Some(5),
                "lib/Hugo Collection/Some Bare Title/Some Bare Title.m4b",
                90_000,
            ),
        ]
    }

    #[test]
    fn pack_members_flatten_with_provenance_and_partial_pack_shell_stays() {
        let nodes = flatten_fixture();
        let plan = build(&nodes);
        let flat = ops_of(&plan, "flatten-packs");

        let moves: Vec<&&PlannedOp> = flat.iter().filter(|o| o.kind == "move").collect();
        assert_eq!(moves.len(), 2, "two clean members flatten");
        for mv in &moves {
            assert_eq!(mv.rule_id, "flatten-packs");
            let prov = mv.provenance_json.as_ref().expect("provenance captured");
            assert!(
                prov.contains("\"pack_name\":\"Hugo Collection\""),
                "prov: {prov}"
            );
            assert!(
                prov.contains("\"pack_path\":\"lib/Hugo Collection\""),
                "prov: {prov}"
            );
        }
        // The award-marked member records the caret.
        let jemisin = moves
            .iter()
            .find(|o| o.source_path.contains("Jemisin"))
            .unwrap();
        assert!(
            jemisin
                .provenance_json
                .as_ref()
                .unwrap()
                .contains("\"award_marker\":\"^\""),
            "award marker not captured: {:?}",
            jemisin.provenance_json
        );

        // AC-5: because one member could not be placed, the shell is NOT
        // quarantined anywhere in the plan.
        assert!(
            !plan.ops.iter().any(|o| o.kind == "quarantine"),
            "a partially-extracted pack must not quarantine its shell"
        );
    }

    #[test]
    fn fully_extracted_pack_quarantines_its_shell() {
        // Same pack but every member is placeable.
        let nodes = vec![
            dir(0, None, "lib/Hugo Collection"),
            dir(
                1,
                Some(0),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season",
            ),
            aud(
                2,
                Some(1),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
            dir(
                3,
                Some(0),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood",
            ),
            aud(
                4,
                Some(3),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000,
            ),
        ];
        let plan = build(&nodes);
        let quarantines: Vec<&PlannedOp> =
            plan.ops.iter().filter(|o| o.kind == "quarantine").collect();
        assert_eq!(quarantines.len(), 1, "the emptied shell is quarantined");
        let q = quarantines[0];
        assert_eq!(q.source_path, "lib/Hugo Collection");
        // FD-34: the shell is set aside OUTSIDE the library, under the per-job
        // folder, preserving its original relative path (AC-21). ROOT is "lib"
        // (a bare synthetic root), so the sibling set-aside root is "Set Aside".
        assert!(
            !q.target_path.starts_with("lib/"),
            "FD-34: a set-aside target never sits inside the library root: {}",
            q.target_path
        );
        assert_eq!(q.target_path, "Set Aside/{job-id}/Hugo Collection");
        // The quarantine (a removal) comes after both member moves.
        let q_idx = plan
            .ops
            .iter()
            .position(|o| o.kind == "quarantine")
            .unwrap();
        let last_move = plan.ops.iter().rposition(|o| o.kind == "move").unwrap();
        assert!(
            q_idx > last_move,
            "shell quarantine must follow the member moves"
        );
    }

    #[test]
    fn leave_in_place_pack_shell_toggle_emits_no_quarantine() {
        use crate::ruleset::PackShellDestination;
        let nodes = vec![
            dir(0, None, "lib/Hugo Collection"),
            dir(
                1,
                Some(0),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season",
            ),
            aud(
                2,
                Some(1),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
        ];
        let (cs, merged) = analyze(&nodes);
        let mut rs = default_ruleset();
        rs.structure.pack_shell = PackShellDestination::LeaveInPlace;
        let plan = build_plan(&nodes, &cs, &merged, &rs, ROOT);
        assert!(
            !plan.ops.iter().any(|o| o.kind == "quarantine"),
            "leave-in-place must not quarantine the shell"
        );
        // But the members still flatten.
        assert!(plan
            .ops
            .iter()
            .any(|o| o.op_group == "flatten-packs" && o.kind == "move"));
    }

    // ---- empty-cleanup + staging -------------------------------------------

    #[test]
    fn empty_folder_is_removed_and_staging_is_a_documented_no_op() {
        let nodes = vec![
            dir(0, None, "lib/Nothing Here"),
            dir(1, None, "lib/_sort"),
            aud(2, Some(1), "lib/_sort/New Audiobook 1.mp3", 50_000),
        ];
        let plan = build(&nodes);

        let empties = ops_of(&plan, "empty-cleanup");
        assert_eq!(empties.len(), 1);
        assert_eq!(empties[0].kind, "rmdir-empty");
        assert_eq!(empties[0].source_path, "lib/Nothing Here");

        let staging = ops_of(&plan, "staging-separation");
        assert_eq!(staging.len(), 1);
        assert_eq!(staging[0].kind, "no-op");
        assert_eq!(staging[0].kind_reason.as_deref(), Some("staging"));
    }

    /// Staging-leak regression: a pack (collection bundle) nested inside a
    /// `_sort` staging area must NOT be flattened. The staging no-op promises
    /// nothing under the pile moves now, so the only op the plan emits is that
    /// single staging no-op: no flatten member moves, and no empty-cleanup of
    /// the pack shell. Before the `inside_staging` guard on `pass_flatten_packs`,
    /// the pack's members were flattened out of the pile the report leaves alone.
    #[test]
    fn pack_nested_under_staging_is_left_alone() {
        let nodes = vec![
            dir(0, None, "lib/_sort"),
            dir(1, Some(0), "lib/_sort/Hugo Collection"),
            dir(
                2,
                Some(1),
                "lib/_sort/Hugo Collection/N.K. Jemisin - The Fifth Season",
            ),
            aud(
                3,
                Some(2),
                "lib/_sort/Hugo Collection/N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
            dir(
                4,
                Some(1),
                "lib/_sort/Hugo Collection/Charles Stross - Neptune's Brood",
            ),
            aud(
                5,
                Some(4),
                "lib/_sort/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000,
            ),
        ];
        // The nested pack really is a pack (guard precondition), so the skip is
        // meaningful and not an artifact of misclassification.
        let (cs, _) = analyze(&nodes);
        assert_eq!(
            cs.iter().find(|c| c.id == 1).map(|c| c.class),
            Some(FolderClass::PackContainer),
            "the nested folder must classify as a pack for this test to bite"
        );

        let plan = build(&nodes);

        assert!(
            ops_of(&plan, "flatten-packs").is_empty(),
            "a pack inside staging must not be flattened: {:?}",
            ops_of(&plan, "flatten-packs")
        );
        assert!(
            !plan
                .ops
                .iter()
                .any(|o| o.source_path.contains("Hugo Collection")),
            "nothing under the staging pile may be touched: {:?}",
            plan.ops
                .iter()
                .map(|o| (&o.op_group, &o.kind, &o.source_path))
                .collect::<Vec<_>>()
        );
        // Exactly the one staging no-op, nothing else.
        assert_eq!(plan.ops.len(), 1, "only the staging no-op: {:?}", plan.ops);
        assert_eq!(plan.ops[0].op_group, "staging-separation");
        assert_eq!(plan.ops[0].kind, "no-op");
    }

    // ---- F-205 parallel-format (AC-33) -------------------------------------

    fn file(id: usize, parent: Option<usize>, path: &str, size: u64) -> PlanNode {
        PlanNode {
            id,
            parent,
            name: leaf(path),
            path: path.to_string(),
            kind: NodeKind::File,
            file_class: Some(crate::scan::typing::classify_path(std::path::Path::new(
                &leaf(path),
            ))),
            size,
        }
    }

    /// AC-33: a book folder holding mp3 chapters plus an m4b sibling in a `0 M4B`
    /// child folder is flagged; the mp3 losers are set aside and the m4b is kept.
    #[test]
    fn parallel_format_quarantines_the_mp3_losers_and_keeps_the_m4b() {
        let nodes = vec![
            dir(0, None, "lib/Some Book"),
            file(1, Some(0), "lib/Some Book/track01.mp3", 40_000),
            file(2, Some(0), "lib/Some Book/track02.mp3", 40_000),
            dir(3, Some(0), "lib/Some Book/0 M4B"),
            file(4, Some(3), "lib/Some Book/0 M4B/Some Book.m4b", 80_000),
        ];
        let plan = build(&nodes);
        let quarantines: Vec<&PlannedOp> = plan
            .ops
            .iter()
            .filter(|o| o.rule_id == "parallel-format-quarantine")
            .collect();
        assert_eq!(quarantines.len(), 2, "both mp3 losers set aside");
        for q in &quarantines {
            assert_eq!(q.kind, "quarantine");
            // FD-31: a parallel-format loser is a copy of the book, so it folds
            // into the Copies group (the dedupe-quarantine pass), not the
            // empty-cleanup pile, even though it is emitted in empty-cleanup.
            assert_eq!(q.op_group, "dedupe-quarantine");
            assert_eq!(
                group_for_op_group(&q.op_group).map(|g| g.label()),
                Some("copies")
            );
            assert!(q.source_path.ends_with(".mp3"), "loser is an mp3");
            // FD-34: set aside OUTSIDE the library under the per-job folder,
            // preserving the original relative path (AC-21).
            assert!(
                !q.target_path.starts_with("lib/"),
                "FD-34: a set-aside target never sits inside the library root: {}",
                q.target_path
            );
            assert!(q.target_path.starts_with("Set Aside/{job-id}/"));
            assert!(
                q.target_path.ends_with(&q.source_path["lib/".len()..]),
                "AC-21: the original relative path is preserved under the job folder: {} -> {}",
                q.source_path,
                q.target_path
            );
        }
        // The m4b is never a quarantine source anywhere in the plan.
        assert!(
            !plan
                .ops
                .iter()
                .any(|o| o.kind == "quarantine" && o.source_path.ends_with(".m4b")),
            "the preferred m4b copy must be kept"
        );
    }

    /// A direct m4b sibling (not in a `0 M4B` folder) also triggers the
    /// parallel-format policy on the mp3 loser.
    #[test]
    fn parallel_format_direct_sibling_m4b_also_flags() {
        let nodes = vec![
            dir(0, None, "lib/Some Book"),
            file(1, Some(0), "lib/Some Book/Some Book.mp3", 40_000),
            file(2, Some(0), "lib/Some Book/Some Book.m4b", 80_000),
        ];
        let plan = build(&nodes);
        let losers: Vec<&PlannedOp> = plan
            .ops
            .iter()
            .filter(|o| o.rule_id == "parallel-format-quarantine")
            .collect();
        assert_eq!(losers.len(), 1);
        assert!(losers[0].source_path.ends_with(".mp3"));
    }

    /// A folder with mp3s but no m4b anywhere is NOT a parallel-format case (no
    /// quarantine); the mp3s are the book's only copy.
    #[test]
    fn mp3_only_folder_is_not_parallel_format() {
        let nodes = vec![
            dir(0, None, "lib/Some Book"),
            file(1, Some(0), "lib/Some Book/track01.mp3", 40_000),
            file(2, Some(0), "lib/Some Book/track02.mp3", 40_000),
        ];
        let plan = build(&nodes);
        assert!(!plan
            .ops
            .iter()
            .any(|o| o.rule_id == "parallel-format-quarantine"));
    }

    // ---- F-402 clutter (AC-6) ----------------------------------------------

    /// The default clutter policy sets aside nfo/sfv/playlist/weblink and keeps
    /// ebook/cover with the book.
    #[test]
    fn clutter_quarantines_junk_and_keeps_ebook_and_cover() {
        let nodes = vec![
            dir(0, None, "lib/N.K. Jemisin"),
            dir(1, Some(0), "lib/N.K. Jemisin/The Fifth Season"),
            aud(
                2,
                Some(1),
                "lib/N.K. Jemisin/The Fifth Season/The Fifth Season.m4b",
                100_000,
            ),
            file(
                3,
                Some(1),
                "lib/N.K. Jemisin/The Fifth Season/release.nfo",
                500,
            ),
            file(
                4,
                Some(1),
                "lib/N.K. Jemisin/The Fifth Season/check.sfv",
                200,
            ),
            file(
                5,
                Some(1),
                "lib/N.K. Jemisin/The Fifth Season/cover.jpg",
                3_000,
            ),
            file(
                6,
                Some(1),
                "lib/N.K. Jemisin/The Fifth Season/book.epub",
                9_000,
            ),
        ];
        let plan = build_clutter_quarantined(&nodes);
        let clutter: Vec<&PlannedOp> = plan
            .ops
            .iter()
            .filter(|o| o.rule_id == "clutter-quarantine")
            .collect();
        assert_eq!(clutter.len(), 2, "only nfo + sfv set aside");
        let sources: Vec<&str> = clutter.iter().map(|o| o.source_path.as_str()).collect();
        assert!(sources.iter().any(|s| s.ends_with("release.nfo")));
        assert!(sources.iter().any(|s| s.ends_with("check.sfv")));
        // ebook + cover are kept: never a quarantine source.
        assert!(!plan.ops.iter().any(|o| o.kind == "quarantine"
            && (o.source_path.ends_with(".jpg") || o.source_path.ends_with(".epub"))));
    }

    // ---- FD-31: clutter set-aside ops inherit the owning pass's group -------

    /// The single clutter set-aside op in a plan, for the group-inheritance
    /// tests below (each fixture is built to produce exactly one).
    fn only_clutter_op(plan: &BuiltPlan) -> &PlannedOp {
        let clutter: Vec<&PlannedOp> = plan
            .ops
            .iter()
            .filter(|o| o.rule_id == "clutter-quarantine")
            .collect();
        assert_eq!(
            clutter.len(),
            1,
            "expected exactly one clutter op: {clutter:?}"
        );
        clutter[0]
    }

    /// FD-31: clutter sitting directly in a pack/collection container rides the
    /// Bundles group (the flatten-packs pass), because unpacking owns that
    /// folder's transformation. The pack here is partially extracted (a bare
    /// member), so the shell is not itself set aside and the nfo is a distinct
    /// clutter op.
    #[test]
    fn pack_internal_clutter_rides_bundles() {
        let nodes = vec![
            dir(0, None, "lib/Hugo Collection"),
            dir(
                1,
                Some(0),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season",
            ),
            aud(
                2,
                Some(1),
                "lib/Hugo Collection/N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
            dir(
                3,
                Some(0),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood",
            ),
            aud(
                4,
                Some(3),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000,
            ),
            // A bare-title member keeps the extraction partial (shell stays).
            dir(5, Some(0), "lib/Hugo Collection/Some Bare Title"),
            aud(
                6,
                Some(5),
                "lib/Hugo Collection/Some Bare Title/Some Bare Title.m4b",
                90_000,
            ),
            // Clutter directly in the pack container.
            file(7, Some(0), "lib/Hugo Collection/release.nfo", 500),
        ];
        let plan = build_clutter_quarantined(&nodes);
        let op = only_clutter_op(&plan);
        assert_eq!(op.op_group, "flatten-packs");
        assert_eq!(
            group_for_op_group(&op.op_group).map(|g| g.label()),
            Some("bundles")
        );
        // FD-34: set aside OUTSIDE the library, preserving the relative path.
        assert!(
            !op.target_path.starts_with("lib/"),
            "FD-34: a set-aside target never sits inside the library root: {}",
            op.target_path
        );
        assert_eq!(
            op.target_path,
            "Set Aside/{job-id}/Hugo Collection/release.nfo"
        );
    }

    /// FD-31: clutter left behind in a multi-book folder (which is emptied in
    /// place, not relocated) rides the Box sets group (the split-multi-book
    /// pass). This case was previously SKIPPED because the folder was marked
    /// handled; it must now be set aside and grouped under box sets.
    #[test]
    fn multibook_internal_clutter_rides_box_sets() {
        let nodes = vec![
            dir(0, None, "lib/Chronicles of Narnia"),
            aud(
                1,
                Some(0),
                "lib/Chronicles of Narnia/The Lion, the Witch and the Wardrobe.m4b",
                90_000,
            ),
            aud(
                2,
                Some(0),
                "lib/Chronicles of Narnia/Prince Caspian.m4b",
                75_000,
            ),
            aud(
                3,
                Some(0),
                "lib/Chronicles of Narnia/The Voyage of the Dawn Treader.m4b",
                80_000,
            ),
            file(4, Some(0), "lib/Chronicles of Narnia/release.nfo", 500),
        ];
        let plan = build_clutter_quarantined(&nodes);
        // Precondition: the folder really is a multi-book suspect (so this test
        // exercises the SplitMultiBook inheritance branch, not a fallback).
        let (cs, _) = analyze(&nodes);
        assert_eq!(
            cs.iter().find(|c| c.id == 0).map(|c| c.class),
            Some(FolderClass::MultiBookSuspect),
            "fixture must be a multi-book suspect"
        );
        let op = only_clutter_op(&plan);
        assert_eq!(op.op_group, "split-multi-book");
        assert_eq!(
            group_for_op_group(&op.op_group).map(|g| g.label()),
            Some("box sets")
        );
    }

    /// FD-31: clutter sitting loose at the library root rides the Loose books
    /// group (the loose-root-books pass).
    #[test]
    fn root_side_clutter_rides_loose_books() {
        let nodes = vec![
            aud(0, None, "lib/Sapiens by Yuval Noah Harari.m4b", 180_000),
            file(1, None, "lib/notes.nfo", 500),
        ];
        let plan = build_clutter_quarantined(&nodes);
        let op = only_clutter_op(&plan);
        assert_eq!(op.op_group, "loose-root-books");
        assert_eq!(
            group_for_op_group(&op.op_group).map(|g| g.label()),
            Some("loose books")
        );
    }

    /// FD-31: standalone clutter with no owning transformation (a plain kept
    /// book folder that is not at the root, not in a pack, not a multi-book, and
    /// not renamed) joins the empty-cleanup pile (Empty folders group). Two
    /// top-level book folders keep the snapshot multi-rooted so neither book is
    /// mistaken for the scan root (which would make its contents read root-side).
    #[test]
    fn standalone_clutter_joins_the_empty_cleanup_pile() {
        let nodes = vec![
            dir(0, None, "lib/Book One"),
            aud(1, Some(0), "lib/Book One/Book One.m4b", 100_000),
            file(2, Some(0), "lib/Book One/release.nfo", 500),
            dir(3, None, "lib/Book Two"),
            aud(4, Some(3), "lib/Book Two/Book Two.m4b", 100_000),
        ];
        let plan = build_clutter_quarantined(&nodes);
        // Precondition: Book One is a plain book leaf, not a container/pack.
        let (cs, _) = analyze(&nodes);
        assert_eq!(
            cs.iter().find(|c| c.id == 0).map(|c| c.class),
            Some(FolderClass::Book),
            "fixture must be a plain book folder"
        );
        let op = only_clutter_op(&plan);
        assert_eq!(op.op_group, "empty-cleanup");
        assert_eq!(
            group_for_op_group(&op.op_group).map(|g| g.label()),
            Some("empty folders")
        );
    }

    // ---- stats --------------------------------------------------------------

    #[test]
    fn stats_report_all_seven_groups_and_total() {
        let nodes = flatten_fixture();
        let plan = build(&nodes);
        assert_eq!(plan.stats.per_group.len(), 7, "all seven groups reported");
        assert_eq!(plan.stats.total_ops, plan.ops.len() as u64);
        let sum: u64 = plan.stats.per_group.iter().map(|g| g.ops).sum();
        assert_eq!(sum, plan.stats.total_ops, "group counts sum to total");
        assert!(
            plan.stats.manual_review_ops >= 1,
            "the bare-title member is manual review"
        );
    }

    // ---- persistence round-trip --------------------------------------------

    #[tokio::test]
    async fn persist_plan_round_trips_ops_in_order() {
        use crate::db::open_db;
        use crate::db::plans::get_plan_ops;
        use crate::db::rulesets::{insert_ruleset, NewRuleset};
        use tempfile::TempDir;

        let nodes = flatten_fixture();
        let plan = build(&nodes);

        let dir_tmp = TempDir::new().unwrap();
        let (pool, _) = open_db(dir_tmp.path()).await.unwrap();
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'lib', '2026-07-04T00:00:00Z', 'completed')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let ruleset_id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Default",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();

        let plan_id = persist_plan_for_tests(
            &pool,
            scan_id,
            ruleset_id,
            "draft",
            &plan,
            "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();

        let rows = get_plan_ops(&pool, plan_id).await.unwrap();
        assert_eq!(rows.len(), plan.ops.len());
        for (row, op) in rows.iter().zip(plan.ops.iter()) {
            assert_eq!(row.op_group, op.op_group);
            assert_eq!(row.kind, op.kind);
            assert_eq!(row.source_path, op.source_path);
            assert_eq!(row.target_path, op.target_path);
            assert_eq!(row.rationale, op.rationale);
            assert_eq!(row.rule_id, op.rule_id);
            assert_eq!(row.byte_size, op.byte_size);
            assert_eq!(
                row.provenance_json.as_deref(),
                op.provenance_json.as_deref()
            );
            assert_eq!(row.validation_state, "pending");
        }
    }
}
