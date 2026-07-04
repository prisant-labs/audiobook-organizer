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
//! ruleset). Persistence is a thin, separate step ([`persist_plan`]) so the
//! pure builder stays trivially testable and deterministic. Like the rest of
//! `crate::plan`, this module has no `cfg`-gating of any kind (the CFG RULE):
//! path handling is string SEMANTICS keyed off the library root's own
//! separator, so a plan built on one host serializes identically on another.
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
//! v0.3.0 Phase 4 (this module) builds the passes whose behavior is fully
//! grounded in the classifications: `loose-root-books`, `strip-noise`,
//! `split-multi-book`, `flatten-packs` (with F-507 provenance capture), and
//! `empty-cleanup`. `staging-separation` emits a documented no-op per staging
//! area (contents are separated by a later, interactive step). Two passes are
//! wired but intentionally empty in this phase, per the implementation plan:
//! `normalize-series` is fed by F-204 disc detection (Phase 6), and
//! `dedupe-quarantine` is fed by F-701 duplicate detection (Phase 6). Both
//! still participate in the group fold so the seven-group contract holds now.
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

use serde::Serialize;

use crate::classify::engine::{ClassifyInput, FolderClass, FolderClassification};
use crate::parse::extract::{Confidence, FieldSource, MergedEntry, NodeKind};
use crate::parse::strip::{strip, StripOptions};
use crate::plan::templates::{render_path, ManualReviewReason, RenderOutcome, TemplateFields};
use crate::ruleset::{pack_shell_disposition, PackShellOutcome, Ruleset};
use crate::scan::typing::FileClass;

/// The folder name new quarantined shells and clutter land under, relative to
/// the library root. Never deletes: the never-delete-audio invariant means a
/// pack shell or non-preferred copy is MOVED here, not removed.
pub const QUARANTINE_DIRNAME: &str = "_Quarantine";

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
            InternalPass::SplitMultiBook => CampaignGroup::Bundles,
            InternalPass::FlattenPacks => CampaignGroup::BoxSets,
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
/// builder assigns every field here, and [`persist_plan`] copies them verbatim.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupCount {
    pub group: String,
    pub ops: u64,
}

/// Plan-level aggregate counts, serialized into `plans.stats_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

/// Assemble the fields to render a book target from a merged entry, letting
/// classification-supplied clean author/series override the raw inherited
/// values (shelf-suppression already applied). Confidence starts at `high` and
/// is lowered to the worst source among the consulted fields.
fn source_confidence(src: FieldSource) -> Confidence {
    match src {
        FieldSource::OwnName => Confidence::High,
        FieldSource::Inherited => Confidence::Medium,
        FieldSource::Derived => Confidence::Low,
    }
}

fn book_fields(
    merged: Option<&MergedEntry>,
    author_override: Option<&str>,
    series_override: Option<&str>,
) -> BookFields {
    let mut confidence = Confidence::High;

    let author = match author_override {
        Some(a) => {
            // Reflect the raw field's source in confidence when we have it.
            match merged.and_then(|m| m.fields.author.as_ref()) {
                Some(f) => confidence = worse(confidence, source_confidence(f.source)),
                None => confidence = worse(confidence, Confidence::Medium),
            }
            Some(a.to_string())
        }
        None => merged.and_then(|m| m.fields.author.as_ref()).map(|f| {
            confidence = worse(confidence, source_confidence(f.source));
            f.value.clone()
        }),
    };

    let title = merged.and_then(|m| m.fields.title.as_ref()).map(|f| {
        confidence = worse(confidence, source_confidence(f.source));
        f.value.clone()
    });

    let series = match series_override {
        Some(s) => Some(s.to_string()),
        None => merged
            .and_then(|m| m.fields.series.as_ref())
            .map(|f| f.value.clone()),
    };
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
    sep: char,
    /// Total descendant file bytes per folder id (0 for files/leaf-less).
    folder_bytes: HashMap<usize, u64>,
    /// Directory paths known to exist or already scheduled for creation.
    created_dirs: HashSet<String>,
    /// Folder ids already claimed by an earlier pass (never re-handled).
    handled: HashSet<usize>,
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
        sep: separator_of(library_root),
        folder_bytes,
        created_dirs,
        handled: HashSet::new(),
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
            InternalPass::NormalizeSeries => { /* fed by F-204 in Phase 6 */ }
            InternalPass::DedupeQuarantine => { /* fed by F-701 in Phase 6 */ }
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
        let fields = book_fields(b.merged_by_id.get(&id).copied(), None, None);
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
            .map(|v| v.clone())
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
            let fields = book_fields(
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
            .map(|v| v.clone())
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
                        "This item inside the box set is itself a collection; it needs manual review before it can be extracted."
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
            let fields = book_fields(
                b.merged_by_id.get(&item_id).copied(),
                clean_author.as_deref(),
                clean_series.as_deref(),
            );
            match render_path(preset, &fields.as_template(), width) {
                RenderOutcome::Rendered(components) => {
                    b.handled.insert(item_id);
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
                            "Extract this book from the box set \"{pack_name}\" into \"{}\".",
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

// ---- pass: empty-cleanup ----

fn pass_empty_cleanup(b: &mut Builder) {
    let pass = InternalPass::EmptyCleanup;

    // 1. Quarantine emptied pack shells (deterministic order by path).
    let mut shells = std::mem::take(&mut b.quarantine_shells);
    shells.sort_by(|(_, a), (_, c)| a.cmp(c));
    for (_, shell_path) in shells {
        let name = base_name(&shell_path, b.sep).to_string();
        let quarantine_dir = join_path(&b.library_root, b.sep, &[QUARANTINE_DIRNAME]);
        // Ensure the quarantine root exists.
        if !b.created_dirs.contains(&quarantine_dir) {
            b.created_dirs.insert(quarantine_dir.clone());
            b.ops.push(PlannedOp {
                op_group: pass.as_str().to_string(),
                kind: "mkdir".to_string(),
                kind_reason: None,
                source_path: String::new(),
                target_path: quarantine_dir.clone(),
                rationale: "Create the quarantine folder so emptied box-set shells can be set aside without deleting anything.".to_string(),
                rule_id: "empty-cleanup-mkdir".to_string(),
                confidence: "high".to_string(),
                byte_size: 0,
                provenance_json: None,
            });
        }
        let target = join_path(&quarantine_dir, b.sep, &[name.as_str()]);
        b.ops.push(PlannedOp {
            op_group: pass.as_str().to_string(),
            kind: "quarantine".to_string(),
            kind_reason: None,
            source_path: shell_path,
            target_path: target,
            rationale: format!(
                "The box set \"{name}\" is empty after its books were extracted; set the shell aside in quarantine (never deleted)."
            ),
            rule_id: "flatten-packs-shell".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        });
    }

    // 2. Remove folders emptied by a split (deterministic; BTreeSet is sorted).
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

        // 3. Folders classified empty in the snapshot.
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

/// Persist a built plan via [`crate::db::plans::insert_plan`], returning the new
/// plan id. Serializes [`PlanStats`] into `plans.stats_json`. The pure build and
/// this persistence are deliberately separate so the determinism golden tests
/// the builder without a database.
pub async fn persist_plan(
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

    fn ops_of<'a>(plan: &'a BuiltPlan, group: &str) -> Vec<&'a PlannedOp> {
        plan.ops.iter().filter(|o| o.op_group == group).collect()
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
        assert!(
            q.target_path.starts_with("lib/_Quarantine/"),
            "target: {}",
            q.target_path
        );
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

        let plan_id = persist_plan(
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
