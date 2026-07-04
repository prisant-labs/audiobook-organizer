//! The hostile plan fixture (v0.3.0 Phase 5, F-404 / AC-12): a purpose-built
//! plan that seeds every Windows-reality hazard validation must catch, each as
//! one clearly labelled operation with a KNOWN expected verdict.
//!
//! # Why an in-memory plan, not a generated filesystem tree
//!
//! [`crate::plan::validate`] is pure over a built plan plus an injected
//! [`crate::plan::validate::ValidationEnv`] (existing paths, the
//! `LongPathsEnabled` bool, and a [`FreeSpace`](crate::plan::validate::FreeSpace)
//! source). Every hazard it checks is a property of the plan STRINGS plus that
//! env, never of real bytes on a real disk: a case-only collision is two target
//! strings, an over-length path is a long string, insufficient free space is an
//! injected number. So the hostile fixture is a deterministic, host-independent,
//! zero-disk, zero-network Rust value - the same reason the plan-builder golden
//! converts the manifest in memory rather than materializing it. (The disk-tree
//! [`crate::fixtures::manifest`] separately carries the near-limit-path and
//! reserved-name FAMILIES for the scanner/generator goldens; this module is the
//! validation-layer counterpart.)
//!
//! [`hostile_plan`] returns the plan, the env inputs to feed
//! [`crate::plan::validate::validate_plan`], and an `expectations` list aligned
//! 1:1 with the plan's ops, stating each op's expected state and machine code.
//! The Phase-5 hostile suite asserts the real verdicts equal these expectations,
//! so every seeded hazard has a named, mechanical catch (the phase's signature).

use std::collections::HashSet;

use crate::plan::builder::{BuiltPlan, GroupCount, PlanStats, PlannedOp};
use crate::plan::validate::FixedFreeSpace;

/// The library root every hostile target is built under.
pub const HOSTILE_ROOT: &str = "E:/lib";

/// A drive with too little free space for the cross-volume moves aimed at it.
pub const SCARCE_VOLUME: &str = "D:";
/// A drive with ample free space (the cross-volume-copy warning case).
pub const AMPLE_VOLUME: &str = "F:";

/// The expected verdict for one hostile op, aligned by index with
/// [`HostilePlan::plan`]'s ops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostileExpectation {
    /// A human label naming the hazard this op seeds.
    pub label: &'static str,
    /// The expected `validation_state` string (`valid`/`warning`/`blocked`).
    pub state: &'static str,
    /// The expected machine code, or `None` for a clean op.
    pub code: Option<&'static str>,
}

/// The hostile plan plus everything needed to validate it deterministically.
pub struct HostilePlan {
    pub plan: BuiltPlan,
    /// Paths that "exist on disk" at validation time (case-insensitive).
    pub existing_paths: HashSet<String>,
    /// `false` here so the fixture exercises BOTH the near-260 interop warning
    /// (AC-13) and the `LongPathsEnabled=0` how-to warning (AC-14).
    pub long_paths_enabled: bool,
    /// Injected free space: [`SCARCE_VOLUME`] cannot hold its cross-volume load,
    /// [`AMPLE_VOLUME`] can.
    pub free_space: FixedFreeSpace,
    /// Expected verdict per op (index-aligned with `plan.ops`).
    pub expectations: Vec<HostileExpectation>,
}

/// Build a `move` op (the common hostile shape).
fn mv(source: &str, target: &str, bytes: i64) -> PlannedOp {
    PlannedOp {
        op_group: "loose-root-books".to_string(),
        kind: "move".to_string(),
        kind_reason: None,
        source_path: source.to_string(),
        target_path: target.to_string(),
        rationale: "Hostile fixture op.".to_string(),
        rule_id: "hostile".to_string(),
        confidence: "high".to_string(),
        byte_size: bytes,
        provenance_json: None,
    }
}

/// Build a `rename` op (same-directory).
fn rename(source: &str, target: &str) -> PlannedOp {
    PlannedOp {
        op_group: "strip-noise".to_string(),
        kind: "rename".to_string(),
        kind_reason: None,
        source_path: source.to_string(),
        target_path: target.to_string(),
        rationale: "Hostile fixture rename.".to_string(),
        rule_id: "hostile".to_string(),
        confidence: "high".to_string(),
        byte_size: 0,
        provenance_json: None,
    }
}

fn expect(label: &'static str, state: &'static str, code: Option<&'static str>) -> HostileExpectation {
    HostileExpectation { label, state, code }
}

/// The purpose-built hostile plan (AC-12). Every op below is one seeded hazard
/// (or a control), and `expectations` states the verdict each must draw.
pub fn hostile_plan() -> HostilePlan {
    // A 255-char target (near the 260 legacy limit) and a 300-char target
    // (over it). Built as single long leaves of ASCII 'a' so no other check
    // (reserved/illegal/collision) fires first.
    let near_leaf = "a".repeat(255 - (HOSTILE_ROOT.len() + 1));
    let near_target = format!("{HOSTILE_ROOT}/{near_leaf}");
    debug_assert_eq!(near_target.chars().count(), 255);
    let over_leaf = "a".repeat(300 - (HOSTILE_ROOT.len() + 1));
    let over_target = format!("{HOSTILE_ROOT}/{over_leaf}");
    debug_assert_eq!(over_target.chars().count(), 300);
    // A target beyond the extended-length allowance (the block threshold).
    let too_long_leaf = "b".repeat(crate::plan::validate::EXTENDED_LENGTH_LIMIT + 50);
    let too_long_target = format!("{HOSTILE_ROOT}/{too_long_leaf}.m4b");

    let ops = vec![
        // 0. Control: a perfectly clean loose-book move.
        mv(
            "E:/lib/clean.m4b",
            "E:/lib/Author/Clean/clean.m4b",
            1_000,
        ),
        // 1+2. Planned collision: two moves producing the SAME target.
        mv("E:/lib/dup1.m4b", "E:/lib/Author/Same/book.m4b", 1_000),
        mv("E:/lib/dup2.m4b", "E:/lib/Author/Same/book.m4b", 1_000),
        // 3+4. Case-only collision (AC-15): targets differ only in case.
        mv("E:/lib/case1.m4b", "E:/lib/Author/CaseBook.m4b", 1_000),
        mv("E:/lib/case2.m4b", "E:/lib/author/casebook.m4b", 1_000),
        // 5. On-disk collision: the target already exists (not vacated).
        mv("E:/lib/od.m4b", "E:/lib/Existing/there.m4b", 1_000),
        // 6. Source-inside-target cycle: move a folder into its own subtree.
        mv("E:/lib/Series", "E:/lib/Series/Sub", 0),
        // 7. Over-length path beyond the extended-length allowance.
        mv("E:/lib/long.m4b", &too_long_target, 1_000),
        // 8. Reserved device name (backstop to F-304).
        rename("E:/lib/res.m4b", "E:/lib/COM1.m4b"),
        // 9+10. Cross-volume moves to a SCARCE volume, summing beyond its free
        //       space -> both blocked (insufficient space).
        mv("E:/lib/cvA.m4b", "D:/lib/Author/cvA.m4b", 4_000),
        mv("E:/lib/cvB.m4b", "D:/lib/Author/cvB.m4b", 4_000),
        // 11. Snapshot staleness: the source vanished since the scan.
        mv("E:/lib/gone.m4b", "E:/lib/Author/Gone/gone.m4b", 1_000),
        // 12. Cross-volume move that FITS an ample volume -> copy warning.
        mv("E:/lib/cvfit.m4b", "F:/lib/Author/cvfit.m4b", 100),
        // 13. Near-260 interop warning (AC-13).
        mv("E:/lib/near.m4b", &near_target, 1_000),
        // 14. Over-260 with LongPathsEnabled=0 -> how-to warning (AC-14).
        mv("E:/lib/over.m4b", &over_target, 1_000),
    ];

    let expectations = vec![
        expect("clean control", "valid", None),
        expect("planned collision A", "blocked", Some("collision-in-plan")),
        expect("planned collision B", "blocked", Some("collision-in-plan")),
        expect("case-only collision A", "blocked", Some("collision-in-plan")),
        expect("case-only collision B", "blocked", Some("collision-in-plan")),
        expect("on-disk collision", "blocked", Some("collision-on-disk")),
        expect("source-inside-target cycle", "blocked", Some("cycle-detected")),
        expect("over-length path", "blocked", Some("path-too-long")),
        expect("reserved device name", "blocked", Some("reserved-name")),
        expect(
            "cross-volume insufficient A",
            "blocked",
            Some("cross-volume-space-insufficient"),
        ),
        expect(
            "cross-volume insufficient B",
            "blocked",
            Some("cross-volume-space-insufficient"),
        ),
        expect("snapshot stale", "blocked", Some("snapshot-stale")),
        expect("cross-volume fits", "warning", Some("cross-volume-copy")),
        expect("near-260 interop", "warning", Some("path-length-near-260")),
        expect("long-paths disabled", "warning", Some("long-paths-disabled")),
    ];
    debug_assert_eq!(ops.len(), expectations.len());

    // "Existing" = every op's source EXCEPT the stale one (11), plus the folder
    // the cycle op moves and the pre-existing on-disk-collision target.
    let mut existing: HashSet<String> = HashSet::new();
    for (i, op) in ops.iter().enumerate() {
        if i == 11 {
            continue; // the stale op's source must NOT exist
        }
        if !op.source_path.is_empty() {
            existing.insert(op.source_path.clone());
        }
    }
    // The on-disk-collision target already exists.
    existing.insert("E:/lib/Existing/there.m4b".to_string());

    let free_space = FixedFreeSpace::new()
        // D: holds one 4000-byte move but not both (8000 needed).
        .with(SCARCE_VOLUME, 5_000)
        // F: has ample room for the 100-byte move.
        .with(AMPLE_VOLUME, 1_000_000);

    let plan = BuiltPlan {
        stats: PlanStats {
            total_ops: ops.len() as u64,
            manual_review_ops: 0,
            per_group: vec![GroupCount {
                group: "loose books".to_string(),
                ops: ops.len() as u64,
            }],
        },
        ops,
    };

    HostilePlan {
        plan,
        existing_paths: existing,
        long_paths_enabled: false,
        free_space,
        expectations,
    }
}
