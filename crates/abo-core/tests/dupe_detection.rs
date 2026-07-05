//! F-701 duplicate detection over the standard fixture library (AC-27, AC-28).
//!
//! Builds the plan node view of the manifest in memory (the same synthetic tree
//! the plan builder golden uses) and runs the pure detector over it, so the
//! fixture's declared same-basename/same-size pair is exercised end to end.

use abo_core::classify::classify;
use abo_core::classify::engine::ClassifyInput;
use abo_core::dupes::detect::{
    detect_duplicates, detect_exact_duplicates, dupe_entries_from_plan_nodes, METHOD_EXACT,
};
use abo_core::fixtures::{standard_library_manifest, FixtureNode};
use abo_core::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};
use abo_core::plan::builder::{classify_inputs_from_plan_nodes, PlanNode};
use abo_core::scan::typing::classify_path;
use std::path::Path;

const ROOT: &str = "library";

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

fn merged_of(nodes: &[PlanNode]) -> Vec<MergedEntry> {
    let entry_inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    extract(&entry_inputs)
}

/// AC-27: the fixture's `Island by Aldous Huxley.m4b` appears twice (loose root
/// plus a `Backup Copy` folder) with identical basename and size, so exact
/// detection yields one group of two copies. The `Island by Aldous Huxley
/// (1).m4b` suffixed copy has a different basename and is NOT in the group.
#[test]
fn island_exact_pair_is_one_group_and_suffixed_copy_excluded() {
    let nodes = nodes_from_manifest();
    let merged = merged_of(&nodes);
    let entries = dupe_entries_from_plan_nodes(&nodes, &merged);

    let exact = detect_exact_duplicates(&entries);
    let island = exact
        .iter()
        .find(|g| g.group_key.starts_with("Island by Aldous Huxley.m4b|"))
        .expect("the Island exact-duplicate pair must be detected");
    assert_eq!(island.method, METHOD_EXACT);
    assert_eq!(island.copies(), 2, "exactly the two exact copies");
    assert_eq!(
        island.total_bytes, 240_000,
        "candidate estimate = sum of both members' bytes"
    );
    for m in &island.members {
        assert!(
            m.path.ends_with("/Island by Aldous Huxley.m4b"),
            "the suffixed \" (1)\" copy must be excluded: {}",
            m.path
        );
    }
}

/// Every group counts as a GROUP (FD-08): the reported unit is groups, members
/// are copies (>= 2 per group), and no group is a single file.
#[test]
fn every_detected_group_has_at_least_two_copies() {
    let nodes = nodes_from_manifest();
    let merged = merged_of(&nodes);
    let entries = dupe_entries_from_plan_nodes(&nodes, &merged);

    let groups = detect_duplicates(&entries);
    assert!(
        !groups.is_empty(),
        "the fixture has at least the Island pair"
    );
    for g in &groups {
        assert!(g.copies() >= 2, "a group has >= 2 copies: {}", g.group_key);
    }
}

/// Detection is deterministic: two runs over the same tree produce identical
/// groups in identical order.
#[test]
fn detection_is_deterministic() {
    let nodes = nodes_from_manifest();
    let merged = merged_of(&nodes);
    let entries = dupe_entries_from_plan_nodes(&nodes, &merged);

    let a = detect_duplicates(&entries);
    let b = detect_duplicates(&entries);
    assert_eq!(a, b);
}

/// A classify pass over the fixture nodes still succeeds (sanity that the plan
/// node view feeds the classifier the detector shares its input shape with).
#[test]
fn fixture_nodes_classify_without_panic() {
    let nodes = nodes_from_manifest();
    let inputs: Vec<ClassifyInput> = classify_inputs_from_plan_nodes(&nodes);
    let cs = classify(&inputs);
    assert_eq!(
        cs.len(),
        nodes.iter().filter(|n| n.kind == NodeKind::Folder).count()
    );
}
