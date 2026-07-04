//! F-403 plan builder goldens (AC-8, AC-9, AC-10, AC-11): builds a plan over
//! the real P1 fixture library (`crates/abo-core/src/fixtures/manifest.rs`),
//! entirely in memory, and locks the serialized result with a committed insta
//! snapshot plus targeted assertions.
//!
//! # Why an in-memory conversion, not a generated filesystem tree
//!
//! The classification goldens materialize the manifest onto a real temp
//! filesystem, which forces them to take a STABLE SUBSET (the reserved-name /
//! near-limit / Unicode-twin subtrees skip-with-note on some hosts, so a
//! committed snapshot over the full generated tree would differ between the
//! ubuntu and windows CI legs). The plan builder is pure string logic over the
//! entry tree, so this test converts the manifest DIRECTLY into plan nodes with
//! synthetic forward-slash paths under a fixed `library` root, never touching a
//! filesystem. That makes the FULL library (every subtree, host-variable ones
//! included) deterministic and byte-identical across hosts, so the committed
//! golden is the determinism gate itself (AC-8) and freezes the plan
//! serialization format (the Phase 4 decision gate).

use abo_core::classify::classify;
use abo_core::classify::engine::ClassifyInput;
use abo_core::fixtures::{standard_library_manifest, FixtureNode};
use abo_core::parse::extract::{extract, EntryInput, NodeKind};
use abo_core::plan::builder::{
    build_plan, classify_inputs_from_plan_nodes, group_for_op_group, pass_from_str, CampaignGroup,
    PlanNode,
};
use abo_core::ruleset::default_ruleset;
use abo_core::scan::typing::classify_path;
use std::path::Path;

const ROOT: &str = "library";

/// Convert the declarative manifest into a flat [`PlanNode`] list with synthetic
/// `/`-joined paths under `library`. Ids are assigned in a stable depth-first
/// pre-order; every top-level manifest node is parentless (the multi-root
/// fixture convention the health metrics and builder both understand), so loose
/// top-level audio files read as loose-root books.
fn nodes_from_manifest() -> Vec<PlanNode> {
    let manifest = standard_library_manifest();
    let mut nodes = Vec::new();
    let mut next_id = 0usize;
    for node in &manifest.top_level {
        walk(node, None, ROOT, &mut next_id, &mut nodes);
    }
    nodes
}

fn walk(
    node: &FixtureNode,
    parent: Option<usize>,
    parent_path: &str,
    next_id: &mut usize,
    out: &mut Vec<PlanNode>,
) {
    let id = *next_id;
    *next_id += 1;
    match node {
        FixtureNode::Folder(f) => {
            let path = format!("{parent_path}/{}", f.name);
            out.push(PlanNode {
                id,
                parent,
                name: f.name.clone(),
                path: path.clone(),
                kind: NodeKind::Folder,
                file_class: None,
                size: 0,
            });
            for child in &f.children {
                walk(child, Some(id), &path, next_id, out);
            }
        }
        FixtureNode::File(file) => {
            let path = format!("{parent_path}/{}", file.name);
            out.push(PlanNode {
                id,
                parent,
                name: file.name.clone(),
                path,
                kind: NodeKind::File,
                file_class: Some(classify_path(Path::new(&file.name))),
                size: file.size,
            });
        }
    }
}

fn analyze(
    nodes: &[PlanNode],
) -> (
    Vec<abo_core::classify::FolderClassification>,
    Vec<abo_core::parse::extract::MergedEntry>,
) {
    let inputs: Vec<ClassifyInput> = classify_inputs_from_plan_nodes(nodes);
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

/// AC-8: the serialized plan over the full fixture library is frozen. The
/// committed snapshot is the byte-for-byte contract; regenerating it changes the
/// plan format and must be a deliberate act (the Phase 4 decision gate).
#[test]
fn full_library_plan_matches_the_committed_golden() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);
    insta::assert_snapshot!("full_library_plan", plan.to_canonical_json());
}

/// AC-8 (CI golden, literal form): the same snapshot + same ruleset produce a
/// byte-identical serialized plan across two independent builds.
#[test]
fn full_library_plan_is_byte_identical_across_two_runs() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let a = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);
    let b = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);
    assert_eq!(a.to_canonical_json(), b.to_canonical_json());
}

/// AC-9: over the whole library, every emitted op carries the required fields.
#[test]
fn every_library_op_carries_the_required_fields() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);
    assert!(!plan.ops.is_empty());
    for op in &plan.ops {
        assert!(
            pass_from_str(&op.op_group).is_some(),
            "op_group known: {}",
            op.op_group
        );
        assert!(!op.kind.is_empty());
        assert!(
            op.rationale.trim_end().ends_with('.'),
            "rationale: {:?}",
            op.rationale
        );
        assert!(!op.rule_id.is_empty());
        assert!(matches!(op.confidence.as_str(), "high" | "medium" | "low"));
        assert!(op.byte_size >= 0);
        if op.kind != "no-op" {
            assert!(!op.target_path.is_empty(), "{:?} needs a target", op.kind);
        }
    }
}

/// AC-11: over the whole library, every op's group folds into one of exactly
/// the seven FD-26 user-facing groups, and the stats report all seven.
#[test]
fn library_plan_folds_into_seven_groups() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);

    for op in &plan.ops {
        assert!(
            group_for_op_group(&op.op_group).is_some(),
            "op_group {} does not fold to a user-facing group",
            op.op_group
        );
    }
    assert_eq!(plan.stats.per_group.len(), 7);
    let labels: Vec<&str> = plan
        .stats
        .per_group
        .iter()
        .map(|g| g.group.as_str())
        .collect();
    let canonical: Vec<&str> = CampaignGroup::ALL.iter().map(|g| g.label()).collect();
    assert_eq!(labels, canonical);
}

/// The loose-root pass folderizes the pattern-1 loose books (the headline G-3
/// metric): the three clean loose books each produce a move, and the
/// "FREE SAMPLE_" outlier (no clean author) becomes a manual-review no-op.
#[test]
fn loose_root_books_move_the_clean_loose_audio() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);

    let loose_moves = plan
        .ops
        .iter()
        .filter(|o| o.op_group == "loose-root-books" && o.kind == "move")
        .count();
    // Sapiens, Atomic Habits, The Phoenix Project, and both Island copies are
    // clean pattern-1 loose books; FREE SAMPLE_ is the one manual-review outlier.
    assert!(
        loose_moves >= 3,
        "expected the clean loose books to move, got {loose_moves}"
    );
    assert!(
        plan.ops.iter().any(|o| o.op_group == "loose-root-books"
            && o.kind == "no-op"
            && o.source_path.contains("FREE SAMPLE_")),
        "the FREE SAMPLE_ outlier must be a manual-review no-op"
    );
}

/// F-507: every flatten-packs member move records pack provenance, and the Hugo
/// award-marked member records the caret.
#[test]
fn flatten_pack_members_record_provenance() {
    let nodes = nodes_from_manifest();
    let (cs, merged) = analyze(&nodes);
    let plan = build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT);

    let member_moves: Vec<_> = plan
        .ops
        .iter()
        .filter(|o| o.op_group == "flatten-packs" && o.kind == "move")
        .collect();
    assert!(
        !member_moves.is_empty(),
        "the Hugo pack should flatten at least one member"
    );
    for mv in &member_moves {
        let prov = mv
            .provenance_json
            .as_ref()
            .expect("member move carries provenance");
        assert!(
            prov.contains("pack_name"),
            "provenance missing pack_name: {prov}"
        );
    }
    assert!(
        member_moves
            .iter()
            .any(|o| o.provenance_json.as_ref().unwrap().contains("award_marker")),
        "the ^ award-marked Hugo member must record its marker"
    );
}
