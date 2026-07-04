//! F-404 hostile-fixture validation suite (v0.3.0 Phase 5, the phase's
//! signature): every seeded Windows-reality hazard is caught, each with a named
//! test asserting the exact machine code (AC-12), plus the path-length dual
//! threshold (AC-13), the `LongPathsEnabled=0` how-to and junction
//! non-traversal (AC-14), the case-only collision block (AC-15), and the
//! approval state machine including blocked-cannot-approve (AC-17).
//!
//! The plan and its validation env come from
//! [`abo_core::fixtures::hostile::hostile_plan`], a pure in-memory value (see
//! that module's doc for why no filesystem is touched).

use std::collections::HashSet;

use abo_core::fixtures::hostile::{hostile_plan, HostilePlan};
use abo_core::plan::validate::{
    next_approval, validate_plan, ApprovalAction, ApprovalError, OpVerdict, ValidationEnv,
    ValidationState,
};

/// Validate the hostile plan and return the verdicts (index-aligned with ops).
fn verdicts_of(h: &HostilePlan) -> Vec<OpVerdict> {
    let env = ValidationEnv::new(&h.existing_paths, h.long_paths_enabled, &h.free_space);
    validate_plan(&h.plan, &env)
}

/// AC-12: every seeded hazard draws its expected state + machine code. This is
/// the whole-fixture assertion; the per-hazard tests below name each one.
#[test]
fn every_seeded_hazard_gets_the_expected_verdict() {
    let h = hostile_plan();
    let verdicts = verdicts_of(&h);
    assert_eq!(
        verdicts.len(),
        h.expectations.len(),
        "one verdict per hostile op"
    );
    for (i, (v, exp)) in verdicts.iter().zip(h.expectations.iter()).enumerate() {
        assert_eq!(
            v.state.as_str(),
            exp.state,
            "op {i} ({}): state mismatch",
            exp.label
        );
        assert_eq!(
            v.reason_code(),
            exp.code,
            "op {i} ({}): code mismatch",
            exp.label
        );
    }
}

/// AC-12: the six seeded hazard families the spec names each appear at least
/// once, by machine code, so none silently drops out of the fixture.
#[test]
fn the_named_hazard_codes_are_all_present() {
    let h = hostile_plan();
    let codes: HashSet<&str> = verdicts_of(&h)
        .iter()
        .filter_map(|v| v.reason_code())
        .collect();
    for required in [
        "collision-in-plan",
        "collision-on-disk",
        "cycle-detected",
        "path-too-long",
        "reserved-name",
        "cross-volume-space-insufficient",
        "snapshot-stale",
    ] {
        assert!(codes.contains(required), "missing hazard code: {required}");
    }
}

/// Helper: find the verdict for the op at the expectation labelled `label`.
fn verdict_for(h: &HostilePlan, verdicts: &[OpVerdict], label: &str) -> OpVerdict {
    let idx = h
        .expectations
        .iter()
        .position(|e| e.label == label)
        .unwrap_or_else(|| panic!("no hostile op labelled {label}"));
    verdicts[idx]
}

#[test]
fn planned_collision_blocks_both_ops() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "planned collision A").reason_code(),
        Some("collision-in-plan")
    );
    assert_eq!(
        verdict_for(&h, &v, "planned collision B").reason_code(),
        Some("collision-in-plan")
    );
}

/// AC-15: two targets differing ONLY in case collide on NTFS and are blocked.
#[test]
fn case_only_collision_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    let a = verdict_for(&h, &v, "case-only collision A");
    let b = verdict_for(&h, &v, "case-only collision B");
    assert!(a.is_blocked() && b.is_blocked());
    assert_eq!(a.reason_code(), Some("collision-in-plan"));
    assert_eq!(b.reason_code(), Some("collision-in-plan"));
}

#[test]
fn on_disk_collision_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "on-disk collision").reason_code(),
        Some("collision-on-disk")
    );
}

#[test]
fn source_inside_target_cycle_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "source-inside-target cycle").reason_code(),
        Some("cycle-detected")
    );
}

#[test]
fn reserved_device_name_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "reserved device name").reason_code(),
        Some("reserved-name")
    );
}

#[test]
fn insufficient_space_cross_volume_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "cross-volume insufficient A").reason_code(),
        Some("cross-volume-space-insufficient")
    );
    assert_eq!(
        verdict_for(&h, &v, "cross-volume insufficient B").reason_code(),
        Some("cross-volume-space-insufficient")
    );
    // The cross-volume move that FITS its ample volume is only a warning.
    assert_eq!(
        verdict_for(&h, &v, "cross-volume fits").reason_code(),
        Some("cross-volume-copy")
    );
}

#[test]
fn snapshot_stale_source_is_blocked() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    assert_eq!(
        verdict_for(&h, &v, "snapshot stale").reason_code(),
        Some("snapshot-stale")
    );
}

/// AC-13: path-length checks use the extended-length allowance for the BLOCK
/// threshold and still emit a near-260 interop WARNING; both thresholds tested.
#[test]
fn path_length_dual_threshold() {
    let h = hostile_plan();
    let v = verdicts_of(&h);
    // Block: beyond the extended-length allowance.
    let too_long = verdict_for(&h, &v, "over-length path");
    assert!(too_long.is_blocked());
    assert_eq!(too_long.reason_code(), Some("path-too-long"));
    // Warn: near the legacy 260-char limit (interop), not blocked.
    let near = verdict_for(&h, &v, "near-260 interop");
    assert!(near.is_warning());
    assert_eq!(near.reason_code(), Some("path-length-near-260"));
}

/// AC-14 (part 1): a target over 260 chars with `LongPathsEnabled=0` produces a
/// warning carrying a how-to link reference.
#[test]
fn long_paths_disabled_warning_carries_a_how_to_link() {
    let h = hostile_plan();
    assert!(!h.long_paths_enabled, "fixture models LongPathsEnabled=0");
    let v = verdicts_of(&h);
    let over = verdict_for(&h, &v, "long-paths disabled");
    assert!(over.is_warning());
    assert_eq!(over.reason_code(), Some("long-paths-disabled"));
    assert!(
        over.how_to.is_some(),
        "the disabled warning must carry a how-to link"
    );
}

/// AC-14 (part 2): junctions/reparse points are never traversed. The scanner
/// records a junction and does not descend, so a plan never contains an
/// operation whose SOURCE lies beyond the junction. We model this by asserting
/// validation never invents such an op and that a source path sitting on the
/// far side of a recorded-but-not-followed junction is treated as
/// non-existent (stale) rather than silently followed.
#[test]
fn junction_targets_are_never_traversed() {
    // The scanner recorded a junction `E:/lib/Link` but did not descend, so
    // nothing under it is in the existing-paths set. A hand-built op whose
    // source lies beyond the junction must therefore read as snapshot-stale
    // (not silently accepted by following the link).
    use abo_core::plan::builder::{BuiltPlan, GroupCount, PlanStats, PlannedOp};
    use abo_core::plan::validate::FixedFreeSpace;

    let op = PlannedOp {
        op_group: "loose-root-books".to_string(),
        kind: "move".to_string(),
        kind_reason: None,
        source_path: "E:/lib/Link/beyond.m4b".to_string(),
        target_path: "E:/lib/Author/beyond.m4b".to_string(),
        rationale: "op beyond a junction.".to_string(),
        rule_id: "test".to_string(),
        confidence: "high".to_string(),
        byte_size: 1,
        provenance_json: None,
    };
    let plan = BuiltPlan {
        stats: PlanStats {
            total_ops: 1,
            manual_review_ops: 0,
            per_group: vec![GroupCount {
                group: "loose books".to_string(),
                ops: 1,
            }],
        },
        ops: vec![op],
    };
    // Existing set records the junction itself but NOTHING beyond it.
    let existing: HashSet<String> = ["E:/lib".to_string(), "E:/lib/Link".to_string()]
        .into_iter()
        .collect();
    let fs = FixedFreeSpace::new();
    let env = ValidationEnv::new(&existing, true, &fs);
    let v = validate_plan(&plan, &env);
    assert_eq!(
        v[0].reason_code(),
        Some("snapshot-stale"),
        "a source beyond a non-followed junction must not be accepted"
    );
}

/// AC-17: the approval state machine, driven over the hostile plan's real
/// verdicts. A blocked op cannot be approved; a valid/warning op can; blocked
/// ops can still be excluded or rejected.
#[test]
fn blocked_ops_cannot_be_approved_but_can_be_excluded() {
    let h = hostile_plan();
    let verdicts = verdicts_of(&h);

    // Every blocked op refuses approval and accepts exclusion.
    let mut saw_blocked = false;
    let mut saw_approvable = false;
    for v in &verdicts {
        match v.state {
            ValidationState::Blocked => {
                saw_blocked = true;
                assert!(matches!(
                    next_approval(v.state, ApprovalAction::Approve),
                    Err(ApprovalError::BlockedCannotBeApproved)
                ));
                assert!(next_approval(v.state, ApprovalAction::Exclude).is_ok());
                assert!(next_approval(v.state, ApprovalAction::Reject).is_ok());
            }
            ValidationState::Valid | ValidationState::Warning => {
                saw_approvable = true;
                assert!(next_approval(v.state, ApprovalAction::Approve).is_ok());
            }
        }
    }
    assert!(saw_blocked, "the hostile plan must contain blocked ops");
    assert!(
        saw_approvable,
        "the hostile plan must contain approvable (valid/warning) ops"
    );
}
