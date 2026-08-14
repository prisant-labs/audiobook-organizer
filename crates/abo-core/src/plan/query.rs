//! F-903/F-502/F-503/F-504 (v0.4.0 Phase 5): the plan REVIEW query layer.
//!
//! This is where a persisted plan's `plan_ops` rows become the shapes the
//! review screen renders: the seven [`crate::ipc::PlanGroupView`] cards
//! (F-502, counts/sizes/status/reason) and the flat, filterable
//! [`crate::ipc::PlanOpView`] listing the group-detail pane and the F-503
//! filter box both read from (F-504 explainability lives on each op view).
//!
//! Like the rest of `crate::plan`, the BUILDING functions here
//! ([`build_plan_review`], [`build_plan_op_view`]) are pure: no I/O, no
//! `cfg`-gating (the CFG RULE). They consume already-read
//! [`crate::db::plans::PlanOpRow`] rows and already-detected
//! [`crate::dupes::DuplicateGroup`] candidates. The one impure orchestration
//! ([`plan_review_for`] / [`plan_ops_for`]) lives here too, mirroring
//! [`crate::plan::report`]'s split between pure generator and impure
//! full-chain entry point.
//!
//! # The Copies group is not `plan_ops` (OQ-3)
//!
//! `InternalPass::DedupeQuarantine` deliberately emits no plan operation
//! (F-701 duplicate detection is candidate-only this release, see
//! `plan::builder`'s module doc); its card is instead built directly from
//! [`crate::dupes::DuplicateGroup`] candidates, always in the
//! [`crate::ipc::GroupStatus::Checking`] "held" state with its switch
//! disabled, matching the design-system 4.9 `.hold` "checking copies"
//! treatment and the F-506 report's own "hold"/"checking" chip
//! (`report::push_summary_table`). This is an implementation decision for
//! OQ-3 ("candidates in a waiting/flag-only state" branch), not a resolved
//! decision record; the exhaustive per-duplicate detail view stays v0.6.0
//! scope (F-702/F-703/F-905 are out of this release).
//!
//! # Explainability is re-derived, not stored (F-504, AC-18)
//!
//! `plan_ops` stores one overall `confidence` and a plain-language
//! `rationale`, not a per-field confidence breakdown or a matched-pattern id.
//! Rather than widen the frozen `plan_ops` schema, [`build_plan_op_view`]
//! re-runs the pure F-301/F-303 name parse
//! ([`crate::parse::extract::extract`]) against the op's OWN source name at
//! query time - deterministic, no I/O - to recover the matched pattern
//! (id + human name via [`crate::parse::matchers::MatcherId::handle`]) and
//! the per-field confidence the disclosure shows. This reflects the source
//! entry's OWN name only (not fields inherited from an ancestor folder,
//! since the review layer does not hold the whole snapshot tree); the
//! rationale sentence next to it already carries the full "why", so this is
//! the supplementary technical-truth layer FD-13 asks for, not the only
//! explanation on the row.

use sqlx::SqlitePool;

use crate::db::plans::{get_plan, get_plan_ops, PlanOpRow};
use crate::dupes::{detect_duplicates, dupe_entries_from_plan_nodes, DuplicateGroup};
use crate::error::AppError;
use crate::ipc::{
    ExtractedFieldView, GroupStatus, MatchedPatternView, PlanApproval, PlanGroupView, PlanOpView,
    PlanOpsPage, PlanReview, PlanValidation,
};
use crate::parse::extract::{
    extract, Confidence as FieldConfidence, EntryInput, NodeKind as ExtractNodeKind,
};
use crate::plan::builder::{
    group_for_op_group, plan_nodes_from_snapshot, BuiltPlan, CampaignGroup,
};
use crate::plan::validate::OpVerdict;
use crate::scan::typing::{classify_path, FileClass};

/// The op kinds that count as a user-facing "change" (mirrors
/// `report::CHANGE_KINDS`): what a group card's count/size and the switch's
/// included/left-out/checking status are computed over. `mkdir` scaffolding
/// and `no-op` hand-offs are never counted as changes.
const CHANGE_KINDS: &[&str] = &["move", "rename", "quarantine", "rmdir-empty"];

/// Hard cap on how many op rows [`plan_ops_for`] returns in one call (NFR
/// scale target: "comfortable over 20,000 files / 1,000 folders"). A plan
/// this large is far beyond anything the discovery baselines describe; the
/// cap exists so a pathological plan can never make one IPC call unbounded,
/// not because real libraries approach it. `mkdir` rows are excluded before
/// counting toward this cap (they are never a review row).
const MAX_OPS_RETURNED: usize = 20_000;

// ---- pure builders ----

/// Build the seven-card [`PlanReview`] for one plan: exactly
/// [`CampaignGroup::ALL`], in canonical order, whatever `ops` holds (a group
/// with zero ops still renders a card, per AC-10).
pub fn build_plan_review(
    plan_id: i64,
    scan_id: i64,
    ops: &[PlanOpRow],
    duplicate_groups: &[DuplicateGroup],
) -> PlanReview {
    let groups = CampaignGroup::ALL
        .iter()
        .map(|&g| build_group_view(g, ops, duplicate_groups))
        .collect();
    PlanReview {
        plan_id,
        scan_id,
        groups,
    }
}

fn build_group_view(
    group: CampaignGroup,
    ops: &[PlanOpRow],
    duplicate_groups: &[DuplicateGroup],
) -> PlanGroupView {
    if group == CampaignGroup::Copies {
        return copies_group_view(duplicate_groups);
    }

    let changes: Vec<&PlanOpRow> = ops
        .iter()
        .filter(|o| group_for_op_group(&o.op_group) == Some(group))
        .filter(|o| CHANGE_KINDS.contains(&o.kind.as_str()))
        .collect();
    let op_count = changes.len() as i64;
    let byte_size: i64 = changes.iter().map(|o| o.byte_size.max(0)).sum();
    let warning_count = changes
        .iter()
        .filter(|o| o.validation_state == "warning")
        .count() as i64;
    let blocked_count = changes
        .iter()
        .filter(|o| o.validation_state == "blocked")
        .count() as i64;
    // The ops that would ACTUALLY run: neither blocked nor individually
    // excluded. This is both the LeftOut/Included discriminator below and the
    // `actionable_count` the footer sums (never `op_count`, which includes
    // blocked/excluded ops that stay put).
    let actionable: Vec<&&PlanOpRow> = changes
        .iter()
        .filter(|o| o.validation_state != "blocked" && o.approval != "excluded")
        .collect();
    let actionable_count = actionable.len() as i64;
    let blocked_or_excluded = changes.len() - actionable.len();

    let (status, reason) = if changes.is_empty() {
        (
            GroupStatus::Checking,
            "Nothing to do here this time - your library already matches this group.".to_string(),
        )
    } else if blocked_or_excluded == changes.len() {
        (
            GroupStatus::Checking,
            "This group can't be included yet - every change in it still needs a decision \
             first."
                .to_string(),
        )
    } else {
        let all_rejected =
            !actionable.is_empty() && actionable.iter().all(|o| o.approval == "rejected");
        let status = if all_rejected {
            GroupStatus::LeftOut
        } else {
            GroupStatus::Included
        };
        (status, group_reason(group).to_string())
    };

    PlanGroupView {
        group: group.slug().to_string(),
        label: group.label().to_string(),
        headline: group_headline(group, op_count.max(0) as u64),
        reason,
        op_count,
        actionable_count,
        byte_size,
        status,
        warning_count,
        blocked_count,
    }
}

/// The Copies card (OQ-3): built from duplicate CANDIDATES, never `plan_ops`
/// (see module doc). Always [`GroupStatus::Checking`]: F-701 is
/// candidate-only this release, so the group can never be toggled on until
/// the F-702 hash verification (v0.6.0) exists.
fn copies_group_view(duplicate_groups: &[DuplicateGroup]) -> PlanGroupView {
    let exact: Vec<&DuplicateGroup> = duplicate_groups.iter().filter(|g| g.is_exact()).collect();
    let op_count = exact.len() as i64;
    let byte_size: i64 = exact.iter().map(|g| g.total_bytes as i64).sum();
    PlanGroupView {
        group: CampaignGroup::Copies.slug().to_string(),
        label: CampaignGroup::Copies.label().to_string(),
        headline: group_headline(CampaignGroup::Copies, op_count.max(0) as u64),
        reason: group_reason(CampaignGroup::Copies).to_string(),
        op_count,
        // The Copies group is always held (Checking) this release, so nothing
        // in it ever runs; it never contributes to the footer's actionable sum.
        actionable_count: 0,
        byte_size,
        status: GroupStatus::Checking,
        warning_count: 0,
        blocked_count: 0,
    }
}

/// The card headline, counts folded in (mirrors `report::group_headline`'s
/// wording so the review UI and the exported report agree, FD-26; `n` is
/// this query layer's own `op_count`/candidate count rather than the
/// report's provenance-derived counts, so the wording is phrased to match
/// exactly what `n` counts here).
fn group_headline(group: CampaignGroup, n: u64) -> String {
    match group {
        CampaignGroup::Staging => {
            if n == 1 {
                "Leave the 1 sorting pile for a later step".to_string()
            } else {
                format!("Leave the {n} sorting piles for a later step")
            }
        }
        CampaignGroup::LooseBooks => format!("Give {n} loose books their own folders"),
        CampaignGroup::MessyNames => format!("Clean up {n} messy folder names"),
        CampaignGroup::BoxSets => format!("Split {n} books out of their box-set folders"),
        CampaignGroup::Bundles => format!("Unpack {n} books out of collection bundles"),
        CampaignGroup::Copies => format!("Move {n} duplicate copies to the Archive"),
        CampaignGroup::EmptyFolders => format!("Sweep out {n} empty folders"),
    }
}

/// The plain-language "why" sentence for a group card (design-system 4.9).
fn group_reason(group: CampaignGroup) -> &'static str {
    match group {
        CampaignGroup::Staging => {
            "These are work-in-progress or sorting folders mixed in with finished books. They \
             move to a Staging area next to the library so Audiobookshelf stops trying to read \
             them; nothing inside them changes."
        }
        CampaignGroup::LooseBooks => {
            "These audiobooks are sitting as single files instead of their own folder. Each one \
             moves into a tidy folder named for its author and title."
        }
        CampaignGroup::MessyNames => {
            "These folder names carry leftover labels from wherever they came from, like \
             bitrates and file sizes. The books do not move; only the names change."
        }
        CampaignGroup::BoxSets => {
            "These folders hold several complete books at once, so Audiobookshelf reads the \
             whole folder as one giant book. Each book gets its own folder inside the series."
        }
        CampaignGroup::Bundles => {
            "These are download packages, not books in your library. The books inside move to \
             their authors; the collection they came from is remembered in the report."
        }
        CampaignGroup::Copies => {
            "Exact copies of the same book, found in more than one place. Before anything \
             moves to the Archive, every pair is double-checked byte by byte so a near-miss \
             is never mistaken for a copy."
        }
        CampaignGroup::EmptyFolders => {
            "These folders hold no audio at all - leftover placeholders from an earlier \
             organization."
        }
    }
}

/// Build one op's review-and-disclosure view (F-504). See the module doc for
/// why the matched pattern and per-field confidence are RE-DERIVED here
/// rather than read from a stored column.
pub fn build_plan_op_view(op: &PlanOpRow) -> PlanOpView {
    let campaign_group = group_for_op_group(&op.op_group);
    let group = campaign_group
        .map(|g| g.slug().to_string())
        .unwrap_or_else(|| op.op_group.clone());

    let validation = match op.validation_state.as_str() {
        "valid" => PlanValidation::Valid,
        "warning" => PlanValidation::Warning,
        _ => PlanValidation::Blocked,
    };
    let approval = match op.approval.as_str() {
        "approved" => PlanApproval::Approved,
        "rejected" => PlanApproval::Rejected,
        "excluded" => PlanApproval::Excluded,
        _ => PlanApproval::Pending,
    };
    let warning_text = match validation {
        PlanValidation::Warning => warning_text_for(op.validation_reason.as_deref()),
        PlanValidation::Blocked => blocked_reason_text(op.validation_reason.as_deref()),
        PlanValidation::Valid => None,
    };

    // F-504 disclosure honesty (FIX 2): the matched pattern and per-field
    // confidence are RE-DERIVED from the op's OWN source name (see module doc).
    // For the inheritance-heavy groups - box sets and bundles - a field a book
    // actually inherits from its ancestor folder (its author, typically) is not
    // present in the item's own name, so a re-derived per-field block would
    // MISREPORT those fields. For those two groups we therefore OMIT the block
    // entirely (None / empty) rather than show a value the engine did not use;
    // the honest mechanism - storing per-field provenance at plan build time -
    // is deferred to v0.6.0 (F-608 hardening; see the P5 report DEBT note). The
    // remaining groups keep the block, and the frontend adds a plain caveat
    // ("Based on this item's own name.") making the own-name basis explicit.
    let inheritance_heavy = matches!(
        campaign_group,
        Some(CampaignGroup::BoxSets) | Some(CampaignGroup::Bundles)
    );
    let (matched_pattern, extracted_fields) =
        if inheritance_heavy || op.kind == "mkdir" || op.source_path.is_empty() {
            (None, Vec::new())
        } else {
            disclosure_from_name(&op.source_path)
        };

    let stripped_noise = if op.kind == "rename" {
        diff_noise(&op.source_path, &op.target_path)
    } else {
        None
    };

    PlanOpView {
        id: op.id,
        group,
        kind: op.kind.clone(),
        kind_reason: op.kind_reason.clone(),
        source_path: op.source_path.clone(),
        target_path: op.target_path.clone(),
        rationale: op.rationale.clone(),
        confidence: op.confidence.clone(),
        byte_size: op.byte_size,
        validation,
        validation_reason: op.validation_reason.clone(),
        warning_text,
        approval,
        matched_pattern,
        extracted_fields,
        stripped_noise,
    }
}

/// The final path component, using whichever separator the path actually
/// contains (mirrors `plan::builder::base_name`'s pure string semantics).
fn base_name_of(path: &str) -> String {
    let sep = if path.contains('\\') { '\\' } else { '/' };
    match path.rfind(sep) {
        Some(idx) => path[idx + 1..].to_string(),
        None => path.to_string(),
    }
}

/// A cheap, extension-table heuristic for whether a name is a FILE (vs a
/// folder) for the purposes of the F-301 matcher's file/folder-aware
/// stripping (see `crate::parse::extract::own_name_outcome`): anything with a
/// recognized extension is treated as a file; an unrecognized/absent
/// extension (the overwhelming common case for a folder name in this domain)
/// is treated as a folder. This is a display-only heuristic for the
/// disclosure's re-derived parse, never a safety input.
fn guess_node_kind(name: &str) -> ExtractNodeKind {
    match classify_path(std::path::Path::new(name)) {
        FileClass::Other => ExtractNodeKind::Folder,
        _ => ExtractNodeKind::File,
    }
}

fn confidence_str(c: FieldConfidence) -> &'static str {
    match c {
        FieldConfidence::High => "high",
        FieldConfidence::Medium => "medium",
        FieldConfidence::Low => "low",
    }
}

/// Re-parse an op's own source name (F-301/F-303) to recover the matched
/// pattern and per-field confidence for the disclosure (AC-18).
fn disclosure_from_name(
    source_path: &str,
) -> (Option<MatchedPatternView>, Vec<ExtractedFieldView>) {
    let name = base_name_of(source_path);
    if name.is_empty() {
        return (None, Vec::new());
    }
    let kind = guess_node_kind(&name);
    let entries = vec![EntryInput {
        id: 0,
        parent: None,
        name,
        kind,
    }];
    let Some(m) = extract(&entries).into_iter().next() else {
        return (None, Vec::new());
    };

    let matched_pattern = m.own_matcher.map(|id| MatchedPatternView {
        id: id.as_str().to_string(),
        name: id.handle().to_string(),
    });

    let mut fields = Vec::new();
    if let Some(f) = &m.fields.title {
        fields.push(ExtractedFieldView {
            field: "title".to_string(),
            value: f.value.clone(),
            confidence: confidence_str(f.confidence).to_string(),
        });
    }
    if let Some(f) = &m.fields.author {
        fields.push(ExtractedFieldView {
            field: "author".to_string(),
            value: f.value.clone(),
            confidence: confidence_str(f.confidence).to_string(),
        });
    }
    if let Some(f) = &m.fields.series {
        fields.push(ExtractedFieldView {
            field: "series".to_string(),
            value: f.value.clone(),
            confidence: confidence_str(f.confidence).to_string(),
        });
    }
    if let Some(f) = &m.fields.series_index {
        fields.push(ExtractedFieldView {
            field: "series-index".to_string(),
            value: f.value.clone(),
            confidence: confidence_str(f.confidence).to_string(),
        });
    }
    if let Some(f) = &m.fields.year {
        fields.push(ExtractedFieldView {
            field: "year".to_string(),
            value: f.value.to_string(),
            confidence: confidence_str(f.confidence).to_string(),
        });
    }

    (matched_pattern, fields)
}

/// What a rename op's own before/after names removed, shown by the
/// disclosure's "stripped noise" line (AC-18): the leftover characters once
/// the (literal) cleaned name is removed from the original, trimmed. Falls
/// back to the whole original name when the cleaned name is not a literal
/// substring of it (e.g. an underscore-to-space rewrite changed characters
/// rather than only removing them).
fn diff_noise(source_path: &str, target_path: &str) -> Option<String> {
    let original = base_name_of(source_path);
    let cleaned = base_name_of(target_path);
    if cleaned.is_empty() || original == cleaned {
        return None;
    }
    if let Some(pos) = original.find(&cleaned) {
        let mut leftover = original[..pos].trim().to_string();
        let tail = original[pos + cleaned.len()..].trim();
        if !tail.is_empty() {
            if !leftover.is_empty() {
                leftover.push(' ');
            }
            leftover.push_str(tail);
        }
        if leftover.is_empty() {
            None
        } else {
            Some(leftover)
        }
    } else {
        Some(original)
    }
}

/// A "held / needs you" sentence for a WARNING verdict (design-system 6.6
/// register): calm, not alarming, never a bare machine code.
fn warning_text_for(reason: Option<&str>) -> Option<String> {
    match reason {
        Some("path-length-near-260") => Some(
            "This location's file path is quite long. It will still work, but a few older \
             Windows tools may not handle it well."
                .to_string(),
        ),
        Some("long-paths-disabled") => Some(
            "This location's file path is long, and Windows long-path support looks to be \
             turned off on this PC. Turning it on avoids any trouble."
                .to_string(),
        ),
        Some("cross-volume-copy") => Some(
            "This move crosses drives, so the file is copied and checked, then the original is \
             removed."
                .to_string(),
        ),
        _ => None,
    }
}

/// A plain-language sentence for a BLOCKED verdict, reusing the already
/// family-safe [`AppError::remediation`] text for the matching Section 8
/// code (the blocking [`crate::plan::validate::ValidationReason`] codes are
/// contract-tested to equal these `AppError` codes, so this is exact reuse,
/// not a guess) - one remediation vocabulary instead of a second copy of it.
fn blocked_reason_text(reason: Option<&str>) -> Option<String> {
    let code = reason?;
    let err = match code {
        "snapshot-stale" => AppError::SnapshotStale {
            path: String::new(),
        },
        "collision-in-plan" => AppError::CollisionInPlan {
            path: String::new(),
        },
        "collision-on-disk" => AppError::CollisionOnDisk {
            path: String::new(),
        },
        "path-too-long" => AppError::PathTooLong {
            path: String::new(),
            length: 0,
        },
        "illegal-component" => AppError::IllegalComponent {
            path: String::new(),
            component: String::new(),
        },
        "reserved-name" => AppError::ReservedName {
            path: String::new(),
            component: String::new(),
        },
        "cross-volume-space-insufficient" => AppError::CrossVolumeSpaceInsufficient {
            volume: String::new(),
            needed: 0,
            available: 0,
        },
        "cycle-detected" => AppError::CycleDetected {
            source_path: String::new(),
            target_path: String::new(),
        },
        "pack-member-blocked" => {
            return Some(
                "A book inside this collection bundle could not be placed, so this folder is \
                 held rather than removed - removing it now would strand that book."
                    .to_string(),
            )
        }
        _ => return None,
    };
    Some(err.remediation().to_string())
}

/// Project an in-memory, UNPERSISTED [`BuiltPlan`] plus its verdicts into the
/// same [`PlanOpRow`] shape [`build_plan_review`] already knows how to fold
/// into the seven group cards (F-906, AC-33's live re-plan preview).
///
/// This is the seam that lets the live preview reuse every bit of group
/// aggregation and status logic in this module (counts, byte sizes, the
/// included/left-out/checking rules) WITHOUT a second copy of that logic and
/// WITHOUT ever writing a `plans`/`plan_ops` row: `id`/`plan_id`/`seq` are
/// synthetic (never real database ids - a preview has no plan to belong to),
/// and `approval` is always `"pending"` (nothing has been reviewed yet; a
/// preview is diagnostic-only, never a persisted, approvable plan).
pub fn synthetic_op_rows(plan: &BuiltPlan, verdicts: &[OpVerdict]) -> Vec<PlanOpRow> {
    assert_eq!(
        plan.ops.len(),
        verdicts.len(),
        "verdicts must align 1:1 with plan ops"
    );
    plan.ops
        .iter()
        .zip(verdicts.iter())
        .enumerate()
        .map(|(i, (op, v))| PlanOpRow {
            id: i as i64,
            plan_id: 0,
            seq: i as i64,
            op_group: op.op_group.clone(),
            kind: op.kind.clone(),
            kind_reason: op.kind_reason.clone(),
            source_path: op.source_path.clone(),
            target_path: op.target_path.clone(),
            rationale: op.rationale.clone(),
            rule_id: op.rule_id.clone(),
            confidence: op.confidence.clone(),
            byte_size: op.byte_size,
            validation_state: v.state.as_str().to_string(),
            validation_reason: v.reason_code().map(|s| s.to_string()),
            provenance_json: op.provenance_json.clone(),
            approval: "pending".to_string(),
            approval_updated_at: None,
        })
        .collect()
}

// ---- impure orchestration ----

/// Map a database error onto the stable IPC taxonomy for the review surface.
fn db_err(e: sqlx::Error) -> AppError {
    AppError::PlanGenerationFailed {
        detail: e.to_string(),
    }
}

/// Re-detect the F-701 duplicate candidates for one scan (the same pure
/// pipeline `plan::report::build_and_persist_plan` runs, steps 2-4 minus the
/// classify+build steps this query does not need). Cheap: one snapshot read
/// plus pure logic, no filesystem I/O.
async fn duplicate_groups_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
) -> Result<Vec<DuplicateGroup>, AppError> {
    use crate::parse::extract::EntryInput as ExtractEntryInput;

    let rows = crate::scan::get_scan_entries(pool, scan_id).await?;
    let nodes = plan_nodes_from_snapshot(&rows);
    let entry_inputs: Vec<ExtractEntryInput> = nodes
        .iter()
        .map(|n| ExtractEntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    let merged = extract(&entry_inputs);
    let dupe_entries = dupe_entries_from_plan_nodes(&nodes, &merged);
    Ok(detect_duplicates(&dupe_entries))
}

/// Build the [`PlanReview`] (the seven group cards) for a persisted plan,
/// re-reading its ops and re-detecting duplicate candidates fresh on every
/// call - the same freshness posture `classify_overview` already uses (never
/// a stale cached count).
pub async fn plan_review_for(pool: &SqlitePool, plan_id: i64) -> Result<PlanReview, AppError> {
    let plan_row = get_plan(pool, plan_id)
        .await
        .map_err(db_err)?
        .ok_or(AppError::PlanNotFound { plan_id })?;
    let ops = get_plan_ops(pool, plan_id).await.map_err(db_err)?;
    let duplicate_groups = duplicate_groups_for_scan(pool, plan_row.scan_id).await?;
    Ok(build_plan_review(
        plan_id,
        plan_row.scan_id,
        &ops,
        &duplicate_groups,
    ))
}

/// The flat, filterable op listing (`plan_list_ops`): every non-`mkdir` row
/// across all seven groups, in the plan's own `seq` order, capped at
/// [`MAX_OPS_RETURNED`]. The group-detail pane and the F-503 filter box both
/// read from this one list client-side (AC-16, AC-17: filtering is view-only
/// and never touches approval).
pub async fn plan_ops_for(pool: &SqlitePool, plan_id: i64) -> Result<PlanOpsPage, AppError> {
    if get_plan(pool, plan_id).await.map_err(db_err)?.is_none() {
        return Err(AppError::PlanNotFound { plan_id });
    }
    let rows = get_plan_ops(pool, plan_id).await.map_err(db_err)?;
    let reviewable: Vec<&PlanOpRow> = rows.iter().filter(|o| o.kind != "mkdir").collect();
    let total = reviewable.len();
    let truncated = total > MAX_OPS_RETURNED;
    let ops = reviewable
        .into_iter()
        .take(MAX_OPS_RETURNED)
        .map(build_plan_op_view)
        .collect();
    Ok(PlanOpsPage {
        ops,
        total: total as i64,
        truncated,
    })
}

/// Distinct source/target NAMES an id set touches, used only by tests below
/// to sanity-check that no two ops in a fixture accidentally collide.
#[cfg(test)]
fn distinct_names(ops: &[PlanOpRow]) -> std::collections::HashSet<String> {
    ops.iter().map(|o| o.source_path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{GroupStatus, PlanApproval, PlanValidation};

    #[allow(clippy::too_many_arguments)]
    fn op(
        id: i64,
        op_group: &str,
        kind: &str,
        source: &str,
        target: &str,
        byte_size: i64,
        validation_state: &str,
        validation_reason: Option<&str>,
        approval: &str,
    ) -> PlanOpRow {
        PlanOpRow {
            id,
            plan_id: 1,
            seq: id,
            op_group: op_group.to_string(),
            kind: kind.to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test rationale.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size,
            validation_state: validation_state.to_string(),
            validation_reason: validation_reason.map(|s| s.to_string()),
            provenance_json: None,
            approval: approval.to_string(),
            approval_updated_at: None,
        }
    }

    /// Group headlines and reasons are USER-FACING: they cross IPC on
    /// `PlanGroupView` and render in `GroupCard.tsx` and `GroupDetail.tsx`. Like
    /// `AppError::remediation`, they passed through NEITHER vocabulary gate, which
    /// is how the Bundles reason kept saying "not shelves" after FD-47 retired the
    /// word. An adversarial review found it; no test could have.
    ///
    /// Swept over every group and both singular and plural headlines, since a
    /// headline that branches on count can hide a retired word on one branch.
    #[test]
    fn no_group_copy_carries_retired_vocabulary() {
        for group in CampaignGroup::ALL {
            let mut samples = vec![group_reason(*group).to_string(), group.label().to_string()];
            // Headlines are templates over a count: probe both sides of any
            // singular/plural branch rather than assuming one shape.
            for n in [0u64, 1, 2] {
                samples.push(group_headline(*group, n));
            }

            for sample in samples {
                let text = sample
                    .to_lowercase()
                    .replace("audiobookshelf", "<the-product>");
                for (word, decision, successor) in [
                    // "aside" bare, not "set aside": the adjacent-only pattern
                    // missed "Set it aside" and "sets the copy aside", both live.
                    ("aside", "FD-42", "Archive"),
                    ("shelf", "FD-47", "library"),
                    ("shelves", "FD-47", "library"),
                ] {
                    assert!(
                        !text.contains(word),
                        "{:?} group copy carries {word:?}, retired by {decision} in favour of \
                         {successor:?}. This renders on the review surface: {sample}",
                        group
                    );
                }
            }
        }
    }

    /// AC-10: exactly seven cards, canonical order, even when `ops` is empty
    /// (every group still renders, per FD-26).
    #[test]
    fn seven_cards_always_render_in_canonical_order() {
        let review = build_plan_review(1, 1, &[], &[]);
        assert_eq!(review.groups.len(), 7);
        let slugs: Vec<&str> = review.groups.iter().map(|g| g.group.as_str()).collect();
        assert_eq!(
            slugs,
            vec![
                "staging",
                "loose-books",
                "messy-names",
                "box-sets",
                "bundles",
                "copies",
                "empty-folders",
            ]
        );
    }

    /// A group with only `pending` ops defaults to `Included` (the design
    /// intent: everything is ready to go until the user turns it off).
    #[test]
    fn a_pending_group_defaults_to_included() {
        let ops = vec![op(
            1,
            "loose-root-books",
            "move",
            "E:/lib/Sapiens.m4b",
            "E:/lib/Yuval Noah Harari/Sapiens/Sapiens.m4b",
            1000,
            "valid",
            None,
            "pending",
        )];
        let review = build_plan_review(1, 1, &ops, &[]);
        let loose = review
            .groups
            .iter()
            .find(|g| g.group == "loose-books")
            .unwrap();
        assert_eq!(loose.status, GroupStatus::Included);
        assert_eq!(loose.op_count, 1);
        assert_eq!(loose.byte_size, 1000);
    }

    /// AC-11: toggling to "rejected" (the group-level skip) reports as
    /// `LeftOut`, and its op_count still reflects the group's real total (the
    /// footer states which quantity it reports, never mixing units).
    #[test]
    fn a_fully_rejected_group_is_left_out() {
        let ops = vec![op(
            1,
            "loose-root-books",
            "move",
            "E:/lib/A.m4b",
            "E:/lib/Author/A.m4b",
            500,
            "valid",
            None,
            "rejected",
        )];
        let review = build_plan_review(1, 1, &ops, &[]);
        let loose = review
            .groups
            .iter()
            .find(|g| g.group == "loose-books")
            .unwrap();
        assert_eq!(loose.status, GroupStatus::LeftOut);
        assert_eq!(loose.op_count, 1);
    }

    /// FIX 5: an included group's `actionable_count` counts only the ops that
    /// would actually run - blocked and individually-excluded ops are left out,
    /// even though `op_count` still reports the group's full change total.
    #[test]
    fn actionable_count_excludes_blocked_and_excluded_ops() {
        let ops = vec![
            op(
                1,
                "loose-root-books",
                "move",
                "E:/lib/A.m4b",
                "E:/lib/Author/A.m4b",
                10,
                "valid",
                None,
                "pending",
            ),
            op(
                2,
                "loose-root-books",
                "move",
                "E:/lib/B.m4b",
                "E:/lib/Author/B.m4b",
                10,
                "blocked",
                Some("snapshot-stale"),
                "pending",
            ),
            op(
                3,
                "loose-root-books",
                "move",
                "E:/lib/C.m4b",
                "E:/lib/Author/C.m4b",
                10,
                "valid",
                None,
                "excluded",
            ),
        ];
        let review = build_plan_review(1, 1, &ops, &[]);
        let loose = review
            .groups
            .iter()
            .find(|g| g.group == "loose-books")
            .unwrap();
        assert_eq!(loose.status, GroupStatus::Included);
        assert_eq!(loose.op_count, 3, "op_count is the full change total");
        assert_eq!(
            loose.actionable_count, 1,
            "only the one valid, non-excluded op would actually run"
        );
    }

    /// AC-15: when every member of a group is blocked or excluded, the group
    /// renders the blocked/empty (`Checking`) state, not a normal card.
    #[test]
    fn all_blocked_or_excluded_group_is_checking() {
        let ops = vec![
            op(
                1,
                "flatten-packs",
                "move",
                "E:/lib/Pack/A.m4b",
                "E:/lib/Author/A.m4b",
                10,
                "blocked",
                Some("snapshot-stale"),
                "pending",
            ),
            op(
                2,
                "flatten-packs",
                "move",
                "E:/lib/Pack/B.m4b",
                "E:/lib/Author/B.m4b",
                10,
                "valid",
                None,
                "excluded",
            ),
        ];
        let review = build_plan_review(1, 1, &ops, &[]);
        let bundles = review.groups.iter().find(|g| g.group == "bundles").unwrap();
        assert_eq!(bundles.status, GroupStatus::Checking);
        assert_eq!(bundles.blocked_count, 1);
    }

    /// A group with zero ops (nothing to do this scan) still renders, held.
    #[test]
    fn an_empty_group_renders_as_checking_not_missing() {
        let review = build_plan_review(1, 1, &[], &[]);
        let boxsets = review
            .groups
            .iter()
            .find(|g| g.group == "box-sets")
            .unwrap();
        assert_eq!(boxsets.status, GroupStatus::Checking);
        assert_eq!(boxsets.op_count, 0);
    }

    /// The Copies card is built from duplicate candidates, not `plan_ops`,
    /// and is always `Checking` (OQ-3).
    #[test]
    fn copies_card_reads_duplicate_candidates_and_is_always_checking() {
        use crate::dupes::{detect_duplicates, dupe_entries_from_plan_nodes, DupeEntry};
        use crate::parse::extract::{extract, EntryInput, NodeKind};
        use crate::plan::builder::PlanNode;

        let nodes = vec![
            PlanNode {
                id: 0,
                parent: None,
                name: "A".to_string(),
                path: "E:/lib/A".to_string(),
                kind: NodeKind::Folder,
                file_class: None,
                size: 0,
            },
            PlanNode {
                id: 1,
                parent: Some(0),
                name: "book.m4b".to_string(),
                path: "E:/lib/A/book.m4b".to_string(),
                kind: NodeKind::File,
                file_class: Some(crate::scan::typing::FileClass::Audio),
                size: 500,
            },
            PlanNode {
                id: 2,
                parent: None,
                name: "B".to_string(),
                path: "E:/lib/B".to_string(),
                kind: NodeKind::Folder,
                file_class: None,
                size: 0,
            },
            PlanNode {
                id: 3,
                parent: Some(2),
                name: "book.m4b".to_string(),
                path: "E:/lib/B/book.m4b".to_string(),
                kind: NodeKind::File,
                file_class: Some(crate::scan::typing::FileClass::Audio),
                size: 500,
            },
        ];
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
        let dupe_entries: Vec<DupeEntry> = dupe_entries_from_plan_nodes(&nodes, &merged);
        let groups = detect_duplicates(&dupe_entries);
        assert!(!groups.is_empty(), "fixture must produce a duplicate group");

        let review = build_plan_review(1, 1, &[], &groups);
        let copies = review.groups.iter().find(|g| g.group == "copies").unwrap();
        assert_eq!(copies.status, GroupStatus::Checking);
        assert_eq!(copies.op_count, 1, "one exact duplicate group");
        assert_eq!(copies.byte_size, 1000);
    }

    /// F-504 / AC-18: the disclosure re-derives a matched pattern and
    /// per-field confidence from the op's own source name.
    #[test]
    fn op_view_recovers_matched_pattern_and_fields() {
        let row = op(
            1,
            "loose-root-books",
            "move",
            "E:/lib/Sapiens by Yuval Noah Harari.m4b",
            "E:/lib/Yuval Noah Harari/Sapiens/Sapiens by Yuval Noah Harari.m4b",
            1000,
            "valid",
            None,
            "pending",
        );
        let view = build_plan_op_view(&row);
        assert_eq!(view.group, "loose-books");
        assert_eq!(view.approval, PlanApproval::Pending);
        assert_eq!(view.validation, PlanValidation::Valid);
        let pattern = view.matched_pattern.expect("a pattern should match");
        assert_eq!(pattern.id, "pattern-1");
        let title = view
            .extracted_fields
            .iter()
            .find(|f| f.field == "title")
            .expect("a title field");
        assert_eq!(title.value, "Sapiens");
        let author = view
            .extracted_fields
            .iter()
            .find(|f| f.field == "author")
            .expect("an author field");
        assert_eq!(author.value, "Yuval Noah Harari");
    }

    /// FIX 2 / F-504 honesty: ops in the inheritance-heavy groups (box sets,
    /// bundles) OMIT the re-derived per-field block entirely, because a field a
    /// book inherits from an ancestor folder is not in the item's own name and
    /// would be misreported. The backend returns None/empty for those groups.
    #[test]
    fn inheritance_heavy_ops_omit_the_rederived_disclosure() {
        for op_group in ["split-multi-book", "flatten-packs"] {
            let row = op(
                1,
                op_group,
                "move",
                "E:/lib/Some Box Set/Book One by Some Author.m4b",
                "E:/lib/Some Author/Book One/Book One by Some Author.m4b",
                1000,
                "valid",
                None,
                "pending",
            );
            let view = build_plan_op_view(&row);
            assert!(
                view.matched_pattern.is_none(),
                "{op_group}: matched pattern must be omitted (inheritance-heavy)"
            );
            assert!(
                view.extracted_fields.is_empty(),
                "{op_group}: per-field confidence must be omitted (inheritance-heavy)"
            );
        }
    }

    /// AC-18 "stripped noise": a rename op's before/after basenames diff to
    /// the removed markers.
    #[test]
    fn rename_op_reports_stripped_noise() {
        let row = op(
            1,
            "strip-noise",
            "rename",
            "E:/lib/14.0 - Cold Days [320k 18;48;03 2.52GB]",
            "E:/lib/Cold Days",
            0,
            "valid",
            None,
            "pending",
        );
        let view = build_plan_op_view(&row);
        let noise = view.stripped_noise.expect("noise should be reported");
        assert!(noise.contains("320k"), "{noise}");
        assert!(noise.contains("14.0"), "{noise}");
    }

    /// AC-14: a blocked op carries a plain-language reason (never a bare
    /// machine code), reused from the matching `AppError::remediation`.
    #[test]
    fn blocked_op_carries_plain_language_reason() {
        let row = op(
            1,
            "loose-root-books",
            "move",
            "E:/lib/Gone.m4b",
            "E:/lib/Author/Gone.m4b",
            0,
            "blocked",
            Some("snapshot-stale"),
            "pending",
        );
        let view = build_plan_op_view(&row);
        let text = view.warning_text.expect("a plain-language reason");
        assert!(!text.contains("snapshot-stale"), "must not leak the code");
        assert!(text.to_lowercase().contains("scan"));
    }

    /// F-906 (AC-33): `synthetic_op_rows` projects a [`BuiltPlan`] + verdicts
    /// into rows `build_plan_review` folds into group cards, with synthetic
    /// (never-real) ids and `approval` always `"pending"`.
    #[test]
    fn synthetic_op_rows_carries_pending_approval_and_real_verdicts() {
        use crate::plan::builder::{PlanStats, PlannedOp};
        use crate::plan::validate::OpVerdict;

        let plan = BuiltPlan {
            ops: vec![PlannedOp {
                op_group: "loose-root-books".to_string(),
                kind: "move".to_string(),
                kind_reason: None,
                source_path: r"E:\lib\A.m4b".to_string(),
                target_path: r"E:\lib\Author\A.m4b".to_string(),
                rationale: "test rationale.".to_string(),
                rule_id: "test-rule".to_string(),
                confidence: "high".to_string(),
                byte_size: 500,
                provenance_json: None,
            }],
            stats: PlanStats {
                total_ops: 1,
                manual_review_ops: 0,
                per_group: vec![],
            },
        };
        let verdicts = vec![OpVerdict::valid()];

        let rows = synthetic_op_rows(&plan, &verdicts);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].plan_id, 0, "a preview belongs to no real plan");
        assert_eq!(rows[0].approval, "pending");
        assert_eq!(rows[0].validation_state, "valid");
        assert_eq!(rows[0].byte_size, 500);

        // The synthetic rows fold into the same seven-card review a real
        // plan's rows do.
        let review = build_plan_review(0, 1, &rows, &[]);
        let loose = review
            .groups
            .iter()
            .find(|g| g.group == "loose-books")
            .unwrap();
        assert_eq!(loose.op_count, 1);
        assert_eq!(loose.status, GroupStatus::Included);
    }

    /// `plan_ops_for`/`plan_review_for` sanity: no fixture surprise where two
    /// distinct rows accidentally share a source path (would make later
    /// disclosure tests ambiguous).
    #[test]
    fn fixture_sources_are_distinct() {
        let ops = vec![
            op(
                1,
                "loose-root-books",
                "move",
                "E:/lib/A.m4b",
                "E:/lib/Author/A.m4b",
                1,
                "valid",
                None,
                "pending",
            ),
            op(
                2,
                "loose-root-books",
                "move",
                "E:/lib/B.m4b",
                "E:/lib/Author/B.m4b",
                1,
                "valid",
                None,
                "pending",
            ),
        ];
        assert_eq!(distinct_names(&ops).len(), 2);
    }
}
