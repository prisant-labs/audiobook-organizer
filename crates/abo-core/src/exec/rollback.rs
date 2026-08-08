//! F-604 (rollback as an inverse plan): undo an applied tidy-up by generating,
//! validating, and persisting its INVERSE as an ordinary plan (D-09, R-7).
//!
//! # Rollback is NOT a special code path (D-09, R-7)
//!
//! The ratified invariant is that an undo is *just another plan*. This module
//! never applies inverse operations directly. It reads what an apply did, decides
//! the inverse operation list, and persists it as a NEW draft plan through the
//! SAME [`crate::plan::validate`] pipeline a forward plan uses (F-404). That
//! inverse plan is then previewed on the same review surface, approved through the
//! same approval state machine, and applied by the same [`Executor`](super::Executor)
//! walk over the same journal - with its own manifest. There is deliberately no
//! shortcut that walks reverse ops without persisting, validating, and approving a
//! plan first: an undo carries exactly the same safety spine as the change it
//! reverses.
//!
//! # Two entry points, one core (P3/P4 obligation)
//!
//! - [`rollback_prepare`] takes a `manifest_id` (a COMPLETED apply's undo file).
//!   The undo file is self-contained and carries the REAL executed paths - the
//!   `{job-id}` set-aside placeholder is already substituted in it (P4) - so the
//!   inverse of a set-aside op moves the item from its real `<set-aside>\<job-id>\`
//!   location back into the library.
//! - [`rollback_prepare_partial`] takes a `job_id` plus a contiguous tail of
//!   completed operations (AC-16). A HALTED apply exports NO undo file by design,
//!   so a partial undo must work from the append-only journal's `done` rows plus
//!   the frozen `plan_ops`. For a set-aside op the frozen `plan_ops.target_path`
//!   still carries the `{job-id}` placeholder; it is reconstructed to the real
//!   location by substituting the job id (which reproduces exactly what the
//!   executor did, avoiding the set-aside-root settings race a
//!   resolve-root-now-plus-relative-path reconstruction would risk), and the
//!   reconstructed source is verified to exist through the [`Vfs`](super::Vfs)
//!   before an inverse op is built - a missing reconstructed path is a validation
//!   failure, never a guess.
//!
//! Both funnel into [`persist_inverse_plan`], which builds the [`BuiltPlan`],
//! runs F-404, and persists (one `insert_plan`; no plan is ever mutated).
//!
//! # Per-kind inverse semantics (FD-10)
//!
//! - `move` / `rename` / `quarantine` invert to a MOVE back (target -> source);
//!   a `rename` keeps kind `rename` (same-directory), the others become `move`.
//! - `mkdir` inverts to `rmdir-empty` of the created directory (removed only if
//!   empty - never clobbers).
//! - `rmdir-empty` inverts to `mkdir` of the removed directory.
//! - `no-op` inverts to nothing.
//!
//! A dry-run apply moved nothing, so its manifest refuses to reverse; this module
//! surfaces that as the plain-language [`AppError::RollbackNotReversible`].

use std::collections::{HashMap, HashSet};
use std::path::Path;

use sqlx::SqlitePool;

use crate::db::plans::{get_plan, get_plan_ops, PlanOpRow};
use crate::error::AppError;
use crate::exec::manifest::{get_manifest_row, Manifest};
use crate::exec::{ApplyMode, Vfs};
use crate::ipc::RollbackPrepared;
use crate::plan::builder::{
    default_set_aside_root, group_for_op_group, BuiltPlan, CampaignGroup, GroupCount, PlanStats,
    PlannedOp, QUARANTINE_JOB_PLACEHOLDER,
};
use crate::plan::validate::{persist_validated_plan, validate_plan, RealFreeSpace, ValidationEnv};

/// The stable, kebab-case `plan_ops.rule_id` every inverse op carries, so an undo
/// op is identifiable as one without inventing a per-forward-op rule id.
///
/// Public because the F-604 forward-tidying gate (P6,
/// [`crate::exec::verify::ensure_forward_tidying_allowed`]) keys off it to tell a
/// forward plan from an undo plan: an undo is the REMEDY for a discrepancy, so it
/// is never gated by the block.
pub const ROLLBACK_RULE_ID: &str = "rollback-inverse";

/// Whether `ops` is an UNDO (inverse) plan rather than a forward one: non-empty
/// and every op carries [`ROLLBACK_RULE_ID`]. Used by the P6 forward-tidying gate
/// to structurally exempt undo from the discrepancy block (undo is the remedy).
/// A forward plan never uses this rule id, so the distinction cannot be spoofed
/// by an ordinary tidy-up.
pub fn is_undo_plan_ops(ops: &[PlanOpRow]) -> bool {
    !ops.is_empty() && ops.iter().all(|o| o.rule_id == ROLLBACK_RULE_ID)
}

/// One inverse operation, decided from a forward op before it becomes a
/// [`PlannedOp`]. The `source`/`target` here are the REAL executed paths (from the
/// undo file, or reconstructed for a journal-tail undo), already inverted, never
/// the `{job-id}` placeholder form.
struct InverseOp {
    /// The forward op's campaign group, carried through so the inverse plan renders
    /// under the same review cards a forward plan does (it is just a plan).
    op_group: String,
    kind: &'static str,
    source_path: String,
    target_path: String,
    byte_size: i64,
    rationale: String,
}

/// The inverse of one forward op, given its REAL executed from/to paths. Returns
/// `Ok(None)` for a `no-op` (nothing to undo). An unknown kind is an honest
/// failure rather than a silent skip (the reversible-kind set is pinned to the
/// dispatch set, so this only fires on a corrupted record).
fn invert(
    forward_kind: &str,
    forward_source: &str,
    forward_target: &str,
    op_group: &str,
    byte_size: i64,
) -> Result<Option<InverseOp>, AppError> {
    let inv = match forward_kind {
        // A move/set-aside inverts to a plain move back into the item's original
        // location; a rename inverts to a rename back (same directory).
        "move" | "quarantine" => InverseOp {
            op_group: op_group.to_string(),
            kind: "move",
            source_path: forward_target.to_string(),
            target_path: forward_source.to_string(),
            byte_size,
            rationale: "Undo the last tidy-up: put this back where it was.".to_string(),
        },
        "rename" => InverseOp {
            op_group: op_group.to_string(),
            kind: "rename",
            source_path: forward_target.to_string(),
            target_path: forward_source.to_string(),
            byte_size,
            rationale: "Undo the last tidy-up: change this name back to what it was.".to_string(),
        },
        // A created folder is removed again - only if empty, so an undo never
        // clobbers anything that ended up inside it.
        "mkdir" => InverseOp {
            op_group: op_group.to_string(),
            kind: "rmdir-empty",
            source_path: forward_target.to_string(),
            target_path: String::new(),
            byte_size: 0,
            rationale: "Undo the last tidy-up: remove the folder that was created (only if it is \
                        empty)."
                .to_string(),
        },
        // A removed empty folder is recreated.
        "rmdir-empty" => InverseOp {
            op_group: op_group.to_string(),
            kind: "mkdir",
            source_path: String::new(),
            target_path: forward_source.to_string(),
            byte_size: 0,
            rationale: "Undo the last tidy-up: put back the folder that was removed.".to_string(),
        },
        "no-op" => return Ok(None),
        other => {
            return Err(AppError::RollbackPrepareFailed {
                detail: format!("cannot undo an unrecognized change kind: {other}"),
            })
        }
    };
    Ok(Some(inv))
}

/// Turn the decided inverse ops (already in undo order: latest forward change
/// first) into a [`BuiltPlan`]. Every op is `high` confidence and carries the
/// forward op's campaign group; the per-group stats mirror the forward builder's
/// shape (all seven groups, in canonical order) so the persisted plan is complete.
fn built_plan_from_inverses(inverses: &[InverseOp]) -> BuiltPlan {
    let ops: Vec<PlannedOp> = inverses
        .iter()
        .map(|inv| PlannedOp {
            op_group: inv.op_group.clone(),
            kind: inv.kind.to_string(),
            kind_reason: None,
            source_path: inv.source_path.clone(),
            target_path: inv.target_path.clone(),
            rationale: inv.rationale.clone(),
            rule_id: ROLLBACK_RULE_ID.to_string(),
            confidence: "high".to_string(),
            byte_size: inv.byte_size,
            provenance_json: None,
        })
        .collect();

    let mut counts: HashMap<CampaignGroup, u64> = HashMap::new();
    for inv in inverses {
        if let Some(g) = group_for_op_group(&inv.op_group) {
            *counts.entry(g).or_default() += 1;
        }
    }
    let per_group = CampaignGroup::ALL
        .iter()
        .map(|&g| GroupCount {
            group: g.label().to_string(),
            ops: counts.get(&g).copied().unwrap_or(0),
        })
        .collect();

    BuiltPlan {
        ops,
        stats: PlanStats {
            total_ops: inverses.len() as u64,
            manual_review_ops: 0,
            per_group,
        },
    }
}

/// The plan's scope identity: `(scan_id, ruleset_id, library_root)`. The inverse
/// plan is a plan over the SAME scan and ruleset as the forward plan (F-405
/// immutability makes a new plan the only way to record it), so its foreign keys
/// resolve and it renders through the same surface.
async fn plan_scope_identity(
    pool: &SqlitePool,
    plan_id: i64,
) -> Result<(i64, i64, String), AppError> {
    let plan = get_plan(pool, plan_id)
        .await
        .map_err(|e| AppError::RollbackPrepareFailed {
            detail: e.to_string(),
        })?
        .ok_or_else(|| AppError::RollbackPrepareFailed {
            detail: format!("the tidy-up plan {plan_id} to undo is no longer recorded"),
        })?;
    let root: Option<String> = sqlx::query_scalar("SELECT root_path FROM scans WHERE id = ?")
        .bind(plan.scan_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::RollbackPrepareFailed {
            detail: e.to_string(),
        })?;
    let library_root = root.ok_or_else(|| AppError::RollbackPrepareFailed {
        detail: "the plan's scan has no recorded library folder".to_string(),
    })?;
    Ok((plan.scan_id, plan.ruleset_id, library_root))
}

/// Resolve the set-aside root the same way an apply does (the F-803 setting when
/// configured, else the FD-34 default sibling). Only used for the inverse plan's
/// F-404 scope check; the inverse of a set-aside op targets the LIBRARY, so this
/// root only ever affects a defensive out-of-scope verdict, never the paths.
async fn resolve_set_aside_root(pool: &SqlitePool, library_root: &str) -> String {
    crate::db::settings::get_settings(pool)
        .await
        .ok()
        .and_then(|s| s.set_aside_root)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_set_aside_root(library_root))
}

/// The shared tail (P3/P4 obligation "one core"): validate the inverse plan
/// through F-404 with the FD-34 scope check enabled, then persist it as a new
/// draft plan (one `insert_plan`; nothing is mutated). `existing_paths` is the
/// set of paths present on disk AFTER the forward apply (the forward targets), so
/// the validator sees the inverse sources as present and the inverse targets as
/// vacant - exactly what a real undo faces.
#[allow(clippy::too_many_arguments)]
async fn persist_inverse_plan(
    pool: &SqlitePool,
    scan_id: i64,
    ruleset_id: i64,
    library_root: &str,
    set_aside_root: &str,
    existing_paths: &HashSet<String>,
    inverses: &[InverseOp],
    now: &str,
) -> Result<RollbackPrepared, AppError> {
    let built = built_plan_from_inverses(inverses);
    let free_space = RealFreeSpace;
    let long_paths = crate::scan::longpath::long_paths_enabled();
    let env = ValidationEnv::new(existing_paths, long_paths, &free_space)
        .with_scope(library_root, set_aside_root);
    let verdicts = validate_plan(&built, &env);
    let plan_id =
        persist_validated_plan(pool, scan_id, ruleset_id, "draft", &built, &verdicts, now)
            .await
            .map_err(|e| AppError::RollbackPrepareFailed {
                detail: e.to_string(),
            })?;
    Ok(RollbackPrepared {
        plan_id,
        op_count: built.ops.len() as i64,
    })
}

/// A map from `plan_ops.id` to the metadata the undo file does not carry: the
/// campaign group (so the inverse renders under the same card) and the byte size
/// (so a cross-volume inverse move sizes the free-space check).
fn op_meta_by_id(ops: &[PlanOpRow]) -> HashMap<i64, (String, i64)> {
    ops.iter()
        .map(|o| (o.id, (o.op_group.clone(), o.byte_size)))
        .collect()
}

/// One forward operation to invert, carrying its REAL executed paths (the
/// `{job-id}` placeholder already substituted). Both entry points build these in
/// REVERSE seq order (latest forward change first) and hand them to
/// [`assemble_inverse_ops`].
struct ForwardOp {
    kind: String,
    source: String,
    target: String,
    op_group: String,
    byte_size: i64,
}

/// Normalize a path for set-aside boundary comparison: backslashes to forward
/// slashes, lowercased (NTFS), no trailing separator. Mirrors the executor's
/// scope normalization so "under the set-aside root" means the same thing here.
fn norm_path(p: &str) -> String {
    let mut s = p.replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// Whether `child` is a STRICT descendant of `root` (under it, not equal to it).
fn is_strict_descendant(child: &str, root: &str) -> bool {
    let c = norm_path(child);
    let r = norm_path(root);
    c != r && c.starts_with(&format!("{r}/"))
}

/// Whether `child` is AT or under `root`.
fn is_at_or_under(child: &str, root: &str) -> bool {
    let c = norm_path(child);
    let r = norm_path(root);
    c == r || c.starts_with(&format!("{r}/"))
}

/// The set-aside root a FROZEN plan op was built against, recovered from the
/// `{job-id}` placeholder in its target.
///
/// A frozen `plan_ops.target_path` for a set-aside op is
/// `<set_aside_root>\{job-id}\<original relative path>`, so everything before the
/// placeholder IS the root, exactly, at plan time. `None` when the target carries
/// no placeholder (an ordinary in-library op) or when nothing precedes it.
///
/// Deriving from the placeholder rather than from the SUBSTITUTED job number is
/// deliberate: after substitution the job id is just a path segment, and a book
/// folder named for that same number would make the cut ambiguous. The placeholder
/// cannot be spoofed by a real path.
///
/// **On keeping only the FIRST recovered prefix.** An adversarial review flagged
/// this as "mixed roots are not handled". It is unreachable by construction rather
/// than unhandled, and the reason is worth recording so nobody adds speculative
/// code for it: [`crate::plan::builder::Builder`] holds ONE `set_aside_root`
/// field, and every set-aside target is built from it through
/// `set_aside_job_dir`, so a single plan cannot mix roots. A partial undo takes
/// one `job_id`, which has one plan. Two roots in one inverse plan would require
/// ops from two plans, which this path never assembles.
///
/// The one shape that genuinely yields `None` here is a set-aside op whose frozen
/// target carries no placeholder at all, which means a plan built before FD-34
/// introduced the per-job segment. Those cannot exist in the wild: no real apply
/// has ever been reachable (the frontend hardcodes `"dry-run"`, and a dry run
/// executes against `MemFs`), so no pre-FD-34 job was ever written. If real
/// applies are enabled and old plans could survive a schema migration, this
/// fallback needs revisiting.
fn set_aside_root_from_frozen_target(frozen_target: &str) -> Option<String> {
    let idx = frozen_target.find(QUARANTINE_JOB_PLACEHOLDER)?;
    let root = frozen_target[..idx].trim_end_matches(['\\', '/']);
    (!root.is_empty()).then(|| root.to_string())
}

/// The effective set-aside root for teardown detection. The FD-34 ensure-mkdir is
/// the ONE op that creates a directory OUTSIDE the library (the per-job folder
/// `<set_aside_root>\<job-id>\`), so its target's PARENT is the real set-aside root
/// as it was baked into the substituted paths - which stays correct even if the
/// `set_aside_root` SETTING changed between apply and undo (the P4 settings race).
/// Falls back to the resolved `set_aside_root` when no set-aside mkdir is present
/// (e.g. a plan whose set-aside items were all top-level so no per-job folder op
/// was needed); a mismatch there degrades safely to no teardown, never a wrong
/// removal, and the library is restored regardless.
fn effective_set_aside_root(
    ordered: &[ForwardOp],
    library_root: &str,
    resolved_set_aside_root: &str,
) -> String {
    ordered
        .iter()
        .filter(|o| {
            o.kind == "mkdir" && !o.target.is_empty() && !is_at_or_under(&o.target, library_root)
        })
        .find_map(|o| {
            Path::new(&o.target)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| resolved_set_aside_root.to_string())
}

/// Collect `leaf_dir` and every ancestor directory that is a strict descendant of
/// `set_aside_root` into `out` (stopping at the set-aside root itself, which
/// pre-exists and is never torn down). Used to derive the per-job folder and the
/// executor's mkdir-first intermediate directories (which have no forward plan op)
/// so the inverse plan can drain them.
fn collect_set_aside_dirs(leaf_dir: &str, set_aside_root: &str, out: &mut HashSet<String>) {
    for anc in Path::new(leaf_dir).ancestors() {
        if anc.as_os_str().is_empty() {
            break;
        }
        let s = anc.to_string_lossy().to_string();
        if is_strict_descendant(&s, set_aside_root) {
            out.insert(s);
        } else {
            break;
        }
    }
}

/// Assemble the full inverse operation list from the forward ops being undone
/// (already in reverse-seq order), plus the set of set-aside directories the
/// inverse plan must additionally drain.
///
/// # Ordering (the safety-critical part)
///
/// 1. **All library restorations first**, in reverse-seq order: moves-back
///    (including set-aside items moving back INTO the library), library folder
///    recreations, and library folder removals. These fully restore the library
///    tree before anything in the set-aside area is touched.
/// 2. **Set-aside directory teardown last**: `rmdir-empty` of the per-job folder
///    AND the executor's mkdir-first intermediate directories (derived from the
///    set-aside ops' real targets, since they have no forward plan op), ordered
///    DEEPEST-FIRST so a child is removed before its parent, with the per-job
///    folder naturally last (shallowest).
///
/// Why teardown is last: every removal is empty-only (it never deletes content and
/// halts safely if a foreign file appeared under the set-aside root). Placing the
/// whole teardown AFTER every library restoration means a teardown halt can never
/// strand a library restoration - the library is already whole. A set-aside op's
/// own inverse (the item moving back into the library) is a library restoration and
/// so runs before the teardown, which is exactly what leaves the set-aside dirs
/// empty and removable. The FD-34 ensure-mkdir is NOT inverted directly; its per-job
/// folder is covered by the synthesized teardown so it is deduped with the
/// intermediates.
///
/// The set-aside boundary is derived via [`effective_set_aside_root`] (from the
/// per-job folder's own mkdir when present), so teardown stays correct even if the
/// `set_aside_root` setting changed after the apply.
fn assemble_inverse_ops(
    ordered: &[ForwardOp],
    library_root: &str,
    resolved_set_aside_root: &str,
) -> Result<(Vec<InverseOp>, HashSet<String>), AppError> {
    let set_aside_root = effective_set_aside_root(ordered, library_root, resolved_set_aside_root);
    let mut restorations: Vec<InverseOp> = Vec::new();
    let mut teardown_dirs: HashSet<String> = HashSet::new();
    // The campaign group the synthesized teardown ops render under (the set-aside
    // group), so an undo plan's cards stay coherent. Falls back if no set-aside op.
    let mut set_aside_group = String::from("dedupe-quarantine");

    for op in ordered {
        let target_is_set_aside =
            !op.target.is_empty() && is_strict_descendant(&op.target, &set_aside_root);
        if op.kind == "mkdir" && target_is_set_aside {
            // A set-aside directory mkdir (the FD-34 ensure-mkdir): its removal is
            // the synthesized teardown, so do not invert it directly.
            set_aside_group = op.op_group.clone();
            collect_set_aside_dirs(&op.target, &set_aside_root, &mut teardown_dirs);
            continue;
        }
        if op.kind == "quarantine" && target_is_set_aside {
            set_aside_group = op.op_group.clone();
            // The item moves back (a library restoration, below); its parent
            // directory chain under the set-aside root must be drained.
            if let Some(parent) = Path::new(&op.target).parent() {
                collect_set_aside_dirs(
                    &parent.to_string_lossy(),
                    &set_aside_root,
                    &mut teardown_dirs,
                );
            }
        }
        if let Some(inv) = invert(&op.kind, &op.source, &op.target, &op.op_group, op.byte_size)? {
            restorations.push(inv);
        }
    }

    // Teardown ops, deepest-first (child before parent; per-job folder last).
    let mut dirs: Vec<String> = teardown_dirs.iter().cloned().collect();
    dirs.sort_by(|a, b| {
        let da = norm_path(a).matches('/').count();
        let db = norm_path(b).matches('/').count();
        db.cmp(&da).then_with(|| norm_path(b).cmp(&norm_path(a)))
    });
    let mut all = restorations;
    for dir in &dirs {
        all.push(InverseOp {
            op_group: set_aside_group.clone(),
            kind: "rmdir-empty",
            source_path: dir.clone(),
            target_path: String::new(),
            byte_size: 0,
            rationale: "Undo the last tidy-up: remove the now-empty Archive folder.".to_string(),
        });
    }
    Ok((all, teardown_dirs))
}

/// Prepare an undo for a COMPLETED apply from its undo file (AC-14).
///
/// Reads the exported undo file (self-contained, real substituted paths), decides
/// the inverse operation list in undo order (latest forward change first), and
/// persists it as a new draft plan that passes the SAME F-404 checks and is
/// previewable through the SAME review surface a forward plan uses. Refuses a
/// rehearsal (dry-run) undo file with the plain-language
/// [`AppError::RollbackNotReversible`] (P2 safety semantic).
///
/// # Undo of an undo (redo), by design
///
/// A rollback is itself an ordinary apply: applying the inverse plan exports its
/// own real, reversible undo file (a `real` manifest), so `rollback_prepare` can be
/// run again on THAT manifest to undo the undo - a redo. The chain is unbounded and
/// carries no special "is-a-redo" state; each link is just another plan.
///
/// Set-aside interaction to be aware of: an inverse plan carries LITERAL,
/// already-substituted paths (it inverts the undo file's real paths, which have no
/// `{job-id}` placeholder). So when a redo re-sets-aside an item, it moves it back
/// to the ORIGINAL apply's per-job set-aside folder (that literal
/// `<set-aside>\<original-job-id>\...` path, recreated by mkdir-first), NOT to a
/// fresh folder named for the redo apply's own (new) job id. The redo runs under a
/// new `jobs.id`, but the set-aside DESTINATION path is the original job's, because
/// the placeholder was resolved once, at the first apply, and never re-minted.
pub async fn rollback_prepare(
    pool: &SqlitePool,
    manifest_id: i64,
    now: &str,
) -> Result<RollbackPrepared, AppError> {
    // 1. Locate the undo file record, then read the self-contained undo file.
    let row = get_manifest_row(pool, manifest_id).await?.ok_or_else(|| {
        AppError::RollbackPrepareFailed {
            detail: format!("no undo file is recorded for id {manifest_id}"),
        }
    })?;
    let text =
        std::fs::read_to_string(&row.json_path).map_err(|e| AppError::RollbackPrepareFailed {
            detail: format!("could not read the undo file: {e}"),
        })?;
    let manifest = Manifest::from_json(&text).map_err(|e| AppError::RollbackPrepareFailed {
        detail: e.to_string(),
    })?;

    // 2. A rehearsal moved nothing, and an unreversible kind cannot be undone: both
    //    surface as the plain-language "nothing to undo" rather than a false offer.
    if manifest.mode == ApplyMode::DryRun || !manifest.reversible {
        return Err(AppError::RollbackNotReversible);
    }

    // 3. Op metadata (group + byte size) the undo file does not carry.
    let plan_ops = get_plan_ops(pool, manifest.plan_id).await.map_err(|e| {
        AppError::RollbackPrepareFailed {
            detail: e.to_string(),
        }
    })?;
    let meta = op_meta_by_id(&plan_ops);

    // 4. Scope: the library root the plan was built over and the resolved set-aside
    //    root (needed to recognize set-aside targets and derive their teardown).
    let (scan_id, ruleset_id, library_root) = plan_scope_identity(pool, manifest.plan_id).await?;
    let set_aside_root = resolve_set_aside_root(pool, &library_root).await;

    // 5. The set of paths present on disk after the forward apply: every forward op
    //    that CREATED a target left that target present (files, set-aside items, and
    //    created directories). This is the validator's existing-paths view.
    let mut present: HashSet<String> = manifest
        .ops
        .iter()
        .filter(|o| {
            matches!(o.kind.as_str(), "move" | "rename" | "quarantine" | "mkdir")
                && !o.target_path.is_empty()
        })
        .map(|o| o.target_path.clone())
        .collect();

    // 6. Build the forward ops to invert in REVERSE seq order (undo the latest
    //    change first), then assemble the inverse plan (library restorations first,
    //    set-aside directory teardown last).
    let mut ordered = manifest.ops.clone();
    ordered.sort_by_key(|o| std::cmp::Reverse(o.seq));
    let mut forward: Vec<ForwardOp> = Vec::new();
    for op in &ordered {
        // A manifest op with no matching frozen `plan_ops` row is corrupt state, not
        // a default: refuse loudly rather than inventing an empty group / zero size.
        let (op_group, byte_size) =
            meta.get(&op.op_id)
                .cloned()
                .ok_or_else(|| AppError::RollbackPrepareFailed {
                    detail: format!(
                        "a change to undo (op {}) has no recorded plan row",
                        op.op_id
                    ),
                })?;
        forward.push(ForwardOp {
            kind: op.kind.clone(),
            source: op.source_path.clone(),
            target: op.target_path.clone(),
            op_group,
            byte_size,
        });
    }
    let (inverses, teardown_dirs) = assemble_inverse_ops(&forward, &library_root, &set_aside_root)?;
    // The synthesized teardown dirs are present on disk too (the executor created
    // them), so the validator sees the rmdir sources as present, not stale.
    present.extend(teardown_dirs);

    // 7. Persist through F-404 (AC-14).
    persist_inverse_plan(
        pool,
        scan_id,
        ruleset_id,
        &library_root,
        &set_aside_root,
        &present,
        &inverses,
        now,
    )
    .await
}

/// Prepare a PARTIAL undo of a contiguous journal tail (AC-16), for a halted or
/// partially-applied job that exported no undo file.
///
/// `tail_op_ids` names the operations to undo. They MUST be a single unbroken run
/// of the most recent completed (`done`) operations - a suffix of the job's done
/// ops in walk order. A selection that skips an operation in the middle, or that
/// is not the latest run, is refused with [`AppError::RollbackSelectionNotContiguous`]
/// (undoing the middle of a run would leave the library in a state no forward plan
/// describes). Operations earlier than the tail stay applied.
///
/// Reconstruction reads the append-only journal's `done` rows plus the frozen
/// `plan_ops`. For a set-aside (`quarantine`) op the frozen target still carries
/// the `{job-id}` placeholder; it is substituted with the real job id (reproducing
/// exactly what the executor did) and the reconstructed source is verified present
/// through `vfs` before the inverse op is built.
pub async fn rollback_prepare_partial<V: Vfs>(
    pool: &SqlitePool,
    vfs: &V,
    job_id: i64,
    tail_op_ids: &[i64],
    now: &str,
) -> Result<RollbackPrepared, AppError> {
    if tail_op_ids.is_empty() {
        return Err(AppError::RollbackPrepareFailed {
            detail: "no changes were selected to undo".to_string(),
        });
    }

    // 1. The job's completed operations, in walk (seq) order. A `done` row is the
    //    terminal that marks an op as actually applied.
    let done: Vec<(i64, i64)> = {
        use sqlx::Row as _;
        let rows = sqlx::query(
            "SELECT op_id, seq FROM journal WHERE job_id = ? AND phase = 'done' ORDER BY seq, id",
        )
        .bind(job_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::RollbackPrepareFailed {
            detail: e.to_string(),
        })?;
        rows.into_iter()
            .map(|r| (r.get::<i64, _>("op_id"), r.get::<i64, _>("seq")))
            .collect()
    };
    if done.is_empty() {
        return Err(AppError::RollbackPrepareFailed {
            detail: "this tidy-up completed no changes, so there is nothing to undo".to_string(),
        });
    }

    // 2. Contiguity (AC-16): the selection must be EXACTLY the last k done ops (a
    //    suffix). Compare the selection as a set against the trailing k done op ids.
    let k = tail_op_ids.len();
    if k > done.len() {
        return Err(AppError::RollbackSelectionNotContiguous);
    }
    let tail_slice = &done[done.len() - k..];
    let tail_set: HashSet<i64> = tail_slice.iter().map(|(op_id, _)| *op_id).collect();
    let selected_set: HashSet<i64> = tail_op_ids.iter().copied().collect();
    if selected_set.len() != k || selected_set != tail_set {
        return Err(AppError::RollbackSelectionNotContiguous);
    }

    // 3. The plan the tail belongs to (via any selected op's frozen row), and the
    //    frozen op rows for the selected ops keyed by id.
    let first_op = tail_slice[0].0;
    let plan_id = crate::db::plans::get_plan_op(pool, first_op)
        .await
        .map_err(|e| AppError::RollbackPrepareFailed {
            detail: e.to_string(),
        })?
        .ok_or_else(|| AppError::RollbackPrepareFailed {
            detail: "the changes to undo are no longer recorded".to_string(),
        })?
        .plan_id;
    let plan_ops =
        get_plan_ops(pool, plan_id)
            .await
            .map_err(|e| AppError::RollbackPrepareFailed {
                detail: e.to_string(),
            })?;
    let by_id: HashMap<i64, PlanOpRow> = plan_ops.into_iter().map(|o| (o.id, o)).collect();

    // 4. Scope (needed to recognize set-aside targets and derive their teardown).
    let (scan_id, ruleset_id, library_root) = plan_scope_identity(pool, plan_id).await?;
    let set_aside_root = resolve_set_aside_root(pool, &library_root).await;

    // 5. Reconstruct each selected op's REAL executed paths (reverse seq order, undo
    //    the latest first) into forward ops to invert, and build the present-on-disk
    //    set from the real forward targets.
    let mut ordered: Vec<(i64, i64)> = tail_slice.to_vec();
    ordered.sort_by_key(|(_, seq)| std::cmp::Reverse(*seq));

    let job_seg = job_id.to_string();
    let mut forward: Vec<ForwardOp> = Vec::new();
    let mut present: HashSet<String> = HashSet::new();
    // The set-aside root THIS JOB actually used, recovered from the frozen plan
    // rather than from the current setting.
    //
    // Why this exists (found by adversarial review of the FD-42 Archive rename):
    // `effective_set_aside_root` derives the boundary from a selected out-of-library
    // `mkdir`, and falls back to the resolved CURRENT root when the selection has
    // none. A contiguous tail holding only later quarantine ops has no mkdir, so
    // after a rename of the default root the fallback pointed at the NEW folder
    // while the files sat under the OLD one: the restore still worked (it replays
    // frozen paths) but no teardown was synthesized, stranding the emptied legacy
    // folder forever.
    //
    // The frozen target carries both the plan-time root AND the placeholder
    // (`<root>\{job-id}\<relative path>`), so the root is the prefix BEFORE the
    // placeholder. That is exact and needs no guessing, unlike cutting at the
    // substituted job number, which a book folder named for that number could spoof.
    let mut frozen_set_aside_root: Option<String> = None;
    for (op_id, _seq) in &ordered {
        let op = by_id
            .get(op_id)
            .ok_or_else(|| AppError::RollbackPrepareFailed {
                detail: "a change to undo is no longer recorded".to_string(),
            })?;
        // The REAL executed target. It is substituted EXACTLY as the executor's
        // `with_scope` does: whenever the frozen target contains the {job-id}
        // placeholder, regardless of kind. This covers BOTH a set-aside (quarantine)
        // move AND the FD-34 per-job ensure-mkdir (`mkdir` targeting
        // `<set_aside_root>\{job-id}\`); gating on `kind == "quarantine"` would build
        // the ensure-mkdir's inverse against a literal `{job-id}` path. P4 obligation:
        // when substitution happened, verify the reconstructed path (the forward op's
        // real target, which the inverse acts on) exists via the Vfs before building
        // the inverse op - a missing reconstructed path is a validation failure, not a
        // guess.
        let real_target = if op.target_path.contains(QUARANTINE_JOB_PLACEHOLDER) {
            if frozen_set_aside_root.is_none() {
                frozen_set_aside_root = set_aside_root_from_frozen_target(&op.target_path);
            }
            let reconstructed = op.target_path.replace(QUARANTINE_JOB_PLACEHOLDER, &job_seg);
            if !vfs.exists(Path::new(&reconstructed)) {
                return Err(AppError::RollbackPrepareFailed {
                    detail: format!(
                        "the Archive location to restore from could not be found: {reconstructed}"
                    ),
                });
            }
            reconstructed
        } else {
            op.target_path.clone()
        };

        if matches!(op.kind.as_str(), "move" | "rename" | "quarantine" | "mkdir")
            && !real_target.is_empty()
        {
            present.insert(real_target.clone());
        }
        forward.push(ForwardOp {
            kind: op.kind.clone(),
            source: op.source_path.clone(),
            target: real_target,
            op_group: op.op_group.clone(),
            byte_size: op.byte_size,
        });
    }

    // 6. Assemble (library restorations first, set-aside teardown last) and persist.
    //    The synthesized teardown ops exist ONLY in this inverse plan, not in the
    //    forward job's journal; a partial tail's teardown covers exactly the set-aside
    //    dirs of the SELECTED set-aside ops. A set-aside dir still holding a
    //    non-selected item's file is not empty, so its `rmdir-empty` halts SAFELY
    //    after the library restorations complete (never deleting the other item).
    // Prefer the root this job actually used over the current setting, so a tail
    // with no selected mkdir still tears down the folder it really filled.
    let job_set_aside_root = frozen_set_aside_root.as_deref().unwrap_or(&set_aside_root);
    let (inverses, teardown_dirs) =
        assemble_inverse_ops(&forward, &library_root, job_set_aside_root)?;
    present.extend(teardown_dirs);
    persist_inverse_plan(
        pool,
        scan_id,
        ruleset_id,
        &library_root,
        job_set_aside_root,
        &present,
        &inverses,
        now,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::db::plans::{insert_plan, set_approval, NewPlan, NewPlanOp};
    use crate::exec::lock::acquire_apply_job;
    use crate::exec::manifest::export_after_apply;
    use crate::exec::{ApplyScope, Executor, RealFs, SqliteJournal};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const NOW: &str = "2026-07-18T00:00:00Z";

    /// A deterministic signature of a directory tree: every entry's path RELATIVE
    /// to `root` (forward-slashed, sorted by a `BTreeMap`, so no directory-iteration
    /// order or timing can leak in), mapped to its content - directories to a fixed
    /// marker, files to their exact bytes. Two trees compare equal iff they are
    /// byte-for-byte identical in structure and file content.
    fn tree_signature(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
                .expect("read_dir")
                .map(|e| e.expect("dir entry").path())
                .collect();
            entries.sort();
            for path in entries {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    out.insert(format!("dir:{rel}"), b"<dir>".to_vec());
                    walk(root, &path, out);
                } else {
                    let bytes = std::fs::read(&path).expect("read file");
                    out.insert(format!("file:{rel}"), bytes);
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    /// Write a file and its parent directories, with fixed content.
    fn write_file(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, content).expect("write file");
    }

    /// One forward op spec for the fixture plan.
    struct Spec {
        op_group: &'static str,
        kind: &'static str,
        source: String,
        target: String,
        byte_size: i64,
    }

    /// Seed the DB rows a plan needs (scan + ruleset), returning `(scan_id,
    /// ruleset_id)`. The scan's `root_path` is the library root the inverse plan's
    /// scope resolves from.
    async fn seed_scan_and_ruleset(pool: &SqlitePool, library_root: &str) -> (i64, i64) {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', ?, ?, 'completed')",
        )
        .bind(library_root)
        .bind(NOW)
        .execute(pool)
        .await
        .expect("scan")
        .last_insert_rowid();
        let ruleset_id = crate::db::rulesets::insert_ruleset(
            pool,
            &crate::db::rulesets::NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            NOW,
        )
        .await
        .expect("ruleset");
        (scan_id, ruleset_id)
    }

    /// Persist the forward plan from `specs` and approve every op, returning the
    /// plan id. Ops are inserted in `specs` order, which becomes their `seq`.
    async fn persist_forward_plan(
        pool: &SqlitePool,
        scan_id: i64,
        ruleset_id: i64,
        specs: &[Spec],
    ) -> i64 {
        let ops: Vec<NewPlanOp> = specs
            .iter()
            .map(|s| NewPlanOp {
                op_group: s.op_group,
                kind: s.kind,
                kind_reason: None,
                source_path: &s.source,
                target_path: &s.target,
                rationale: "forward op.",
                rule_id: "test-rule",
                confidence: "high",
                byte_size: s.byte_size,
                validation_state: "valid",
                validation_reason: None,
                provenance_json: None,
            })
            .collect();
        let plan_id = insert_plan(
            pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &ops,
            NOW,
        )
        .await
        .expect("insert forward plan");
        for row in get_plan_ops(pool, plan_id).await.expect("ops") {
            set_approval(pool, row.id, "approved", NOW)
                .await
                .expect("approve");
        }
        plan_id
    }

    /// Apply an approved plan for REAL against `RealFs` with the FD-34 scope, and
    /// return the executor's (substituted) ops so a manifest can be built from
    /// exactly what was applied. Marks the apply job terminal so a later apply can
    /// acquire the single-writer lock.
    async fn apply_for_real(
        pool: &SqlitePool,
        plan_id: i64,
        scope: &ApplyScope,
    ) -> (i64, Vec<PlanOpRow>) {
        let job_id = acquire_apply_job(pool, ApplyMode::Real, NOW)
            .await
            .expect("acquire apply job");
        let ops = get_plan_ops(pool, plan_id).await.expect("ops");
        let executor = Executor::with_scope(RealFs::new(), job_id, ops, scope.clone());
        let journal = SqliteJournal::new(pool.clone());
        let outcome = executor.run(&journal, NOW).await.expect("walk");
        assert!(outcome.halt.is_none(), "the forward walk must not halt");
        let applied = executor.ops().to_vec();
        sqlx::query("UPDATE jobs SET state = 'completed', finished_at = ? WHERE id = ?")
            .bind(NOW)
            .bind(job_id)
            .execute(pool)
            .await
            .expect("mark job completed");
        (job_id, applied)
    }

    /// The six-op fixture that exercises every reversible kind AND the hard case:
    /// mkdir, an EARLY move (seq 1, before the ensure-mkdir), a folder rename,
    /// rmdir-empty, the FD-34 ensure-mkdir (seq 4), and a set-aside of a NESTED item
    /// (seq 5) whose relative path has three intermediate directories. The executor
    /// creates those intermediates via mkdir-first with no forward plan op, so the
    /// inverse plan must synthesize their teardown; and because the early move is
    /// undone AFTER the set-aside move in reverse-seq order, a teardown that halted
    /// mid-sequence would strand it - the ordering guarantees it cannot.
    fn build_fixture(library_root: &Path, set_aside_root: &Path) -> Vec<Spec> {
        // Original tree.
        write_file(&library_root.join("loose-book.m4b"), b"loose book bytes");
        write_file(
            &library_root.join("Messy Name [128k]").join("track.m4b"),
            b"messy track bytes",
        );
        // A NESTED duplicate: its library-relative path is `Genre/Series/Book/dup.m4b`,
        // so the set-aside target has three intermediate directories under the per-job
        // folder that only the executor's mkdir-first creates (no forward op).
        write_file(
            &library_root
                .join("Genre")
                .join("Series")
                .join("Book")
                .join("dup.m4b"),
            b"duplicate copy bytes",
        );
        std::fs::create_dir_all(library_root.join("EmptyShell")).expect("empty shell");

        let lib = library_root.to_string_lossy().to_string();
        let aside = set_aside_root.to_string_lossy().to_string();
        // The set-aside group is emitted LAST (its ensure-mkdir at seq 4 and the
        // set-aside move at seq 5), so a contiguous tail can undo exactly that group.
        vec![
            Spec {
                op_group: "loose-root-books",
                kind: "mkdir",
                source: String::new(),
                target: format!("{lib}/Author"),
                byte_size: 0,
            },
            // An EARLY move (seq 1, lower than the ensure-mkdir at seq 4): its inverse
            // runs late in the undo, so it is the op a stranding teardown halt would skip.
            Spec {
                op_group: "loose-root-books",
                kind: "move",
                source: format!("{lib}/loose-book.m4b"),
                target: format!("{lib}/Author/loose-book.m4b"),
                byte_size: 16,
            },
            Spec {
                op_group: "strip-noise",
                kind: "rename",
                source: format!("{lib}/Messy Name [128k]"),
                target: format!("{lib}/Messy Name"),
                byte_size: 0,
            },
            Spec {
                op_group: "empty-cleanup",
                kind: "rmdir-empty",
                source: format!("{lib}/EmptyShell"),
                target: String::new(),
                byte_size: 0,
            },
            // FD-34 ensure-mkdir: the per-job set-aside folder `<aside>\{job-id}\`.
            // Kind is `mkdir` (NOT `quarantine`), but its target carries the
            // placeholder, so the executor substitutes the job id into it - and so
            // must a journal-tail undo.
            Spec {
                op_group: "dedupe-quarantine",
                kind: "mkdir",
                source: String::new(),
                target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}"),
                byte_size: 0,
            },
            Spec {
                op_group: "dedupe-quarantine",
                kind: "quarantine",
                source: format!("{lib}/Genre/Series/Book/dup.m4b"),
                target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}/Genre/Series/Book/dup.m4b"),
                byte_size: 20,
            },
        ]
    }

    /// The legacy-root recovery that keeps a partial undo's teardown pointed at the
    /// folder the job actually filled (found by adversarial review of FD-42).
    #[test]
    fn frozen_target_yields_the_root_the_plan_was_built_against() {
        // Windows and POSIX separators both, since plans are built with either.
        assert_eq!(
            set_aside_root_from_frozen_target(r"E:\Set Aside\{job-id}\Some Book\part01.mp3"),
            Some(r"E:\Set Aside".to_string()),
            "a pre-rename plan still resolves to its ORIGINAL root"
        );
        assert_eq!(
            set_aside_root_from_frozen_target("E:/Audiobook Archive/{job-id}/B/x.mp3"),
            Some("E:/Audiobook Archive".to_string())
        );
        // No placeholder: an ordinary in-library op contributes no root.
        assert_eq!(
            set_aside_root_from_frozen_target(r"E:\Books - Audio\Author\Book\part01.mp3"),
            None
        );
        // Never yields an empty root, which would make every path a descendant.
        assert_eq!(set_aside_root_from_frozen_target("{job-id}/B/x.mp3"), None);
    }

    /// Approve every op of the inverse plan and apply it for REAL, so the round
    /// trip completes through the same executor walk. Returns nothing; the caller
    /// re-signatures the tree.
    async fn apply_inverse_plan(pool: &SqlitePool, plan_id: i64, scope: &ApplyScope) {
        for row in get_plan_ops(pool, plan_id).await.expect("inverse ops") {
            assert_ne!(
                row.validation_state, "blocked",
                "an inverse op must not be blocked by F-404: {row:?}"
            );
            set_approval(pool, row.id, "approved", NOW)
                .await
                .expect("approve inverse");
        }
        let (_job, _applied) = apply_for_real(pool, plan_id, scope).await;
    }

    /// Approve every op of a plan, apply it for REAL, and export its undo file,
    /// returning the new manifest id. Used to build the undo/redo chain (each apply,
    /// including a rollback, exports its own real reversible undo file).
    async fn approve_apply_export(
        pool: &SqlitePool,
        plan_id: i64,
        scope: &ApplyScope,
        reports_dir: &Path,
    ) -> i64 {
        for row in get_plan_ops(pool, plan_id).await.expect("ops") {
            set_approval(pool, row.id, "approved", NOW)
                .await
                .expect("approve");
        }
        let (job_id, applied) = apply_for_real(pool, plan_id, scope).await;
        export_after_apply(
            pool,
            reports_dir,
            ApplyMode::Real,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export undo file")
        .manifest_id
    }

    /// AC-15 (the release signature gate): apply the full fixture plan for real in
    /// a temp dir, roll back through `rollback_prepare` (an inverse plan, validated
    /// and applied by the same walk), and prove the tree is byte-identical. Fully
    /// deterministic: a fixed fixture, fixed content, no timing, no reliance on
    /// directory iteration order.
    #[tokio::test]
    async fn round_trip_is_byte_identical() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");

        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };

        // Signature BEFORE any change.
        let original = tree_signature(&library_root);

        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;

        // Apply the forward plan for real; export the undo file.
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;
        let reports = TempDir::new().expect("reports dir");
        let export = export_after_apply(
            &pool,
            reports.path(),
            ApplyMode::Real,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export undo file");

        // The tree really changed (so the round trip proves something).
        let after_forward = tree_signature(&library_root);
        assert_ne!(
            original, after_forward,
            "the forward apply must change the tree"
        );
        assert!(
            library_root.join("Author").join("loose-book.m4b").exists(),
            "the loose book moved into its author folder"
        );
        assert!(
            !library_root.join("EmptyShell").exists(),
            "the empty folder was removed"
        );

        // Prepare the undo (an inverse PLAN, validated by F-404, previewable), then
        // apply it through the same executor walk.
        let prepared = rollback_prepare(&pool, export.manifest_id, NOW)
            .await
            .expect("rollback_prepare");
        // Five library restorations (move-back of the nested set-aside item, the
        // empty-folder recreate, the rename-back, the early move-back, the author-dir
        // removal) plus four set-aside teardown rmdir-empty ops (Book, Series, Genre,
        // per-job folder), deepest-first.
        assert_eq!(
            prepared.op_count, 9,
            "5 library restorations + 4 set-aside teardown ops"
        );
        apply_inverse_plan(&pool, prepared.plan_id, &scope).await;

        // Signature AFTER the round trip: byte-identical to the original.
        let after_rollback = tree_signature(&library_root);
        // The whole per-job set-aside tree is gone (deepest-first teardown drained the
        // executor's mkdir-first intermediates too): no residue.
        assert!(
            !set_aside_root.join(job_id.to_string()).exists(),
            "the per-job set-aside folder (and its intermediates) are fully removed"
        );
        assert_eq!(
            original, after_rollback,
            "the library tree must be byte-identical after the round trip"
        );
    }

    /// Undo of an undo (redo), by design: applying an inverse plan exports its own
    /// real reversible undo file, so `rollback_prepare` on THAT manifest re-applies
    /// the original change. Also pins the set-aside interaction documented on
    /// `rollback_prepare`: the redo re-sets-aside the item under the ORIGINAL apply's
    /// per-job folder (a literal path carried through the inverse plans), not a fresh
    /// folder for the redo's own job id.
    #[tokio::test]
    async fn undo_of_an_undo_redoes_the_change() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;

        let original = tree_signature(&library_root);

        // Forward apply (job 1) + its undo file.
        let reports1 = TempDir::new().expect("reports1");
        let (job1, applied1) = apply_for_real(&pool, plan_id, &scope).await;
        let m1 = export_after_apply(
            &pool,
            reports1.path(),
            ApplyMode::Real,
            job1,
            plan_id,
            &applied1,
        )
        .await
        .expect("export forward undo file")
        .manifest_id;
        let after_forward = tree_signature(&library_root);

        // Undo (job 2) + its own undo file. Restores the original.
        let undo = rollback_prepare(&pool, m1, NOW).await.expect("undo");
        let reports2 = TempDir::new().expect("reports2");
        let m2 = approve_apply_export(&pool, undo.plan_id, &scope, reports2.path()).await;
        assert_eq!(
            original,
            tree_signature(&library_root),
            "the undo restores the original tree"
        );

        // Redo = undo of the undo. It re-sets-aside dup under the ORIGINAL job's
        // per-job folder (literal path carried through), not a fresh one.
        let redo = rollback_prepare(&pool, m2, NOW).await.expect("redo");
        let redo_ops = get_plan_ops(&pool, redo.plan_id).await.expect("redo ops");
        let requarantine = redo_ops
            .iter()
            .find(|o| o.kind == "move" && o.target_path.replace('\\', "/").ends_with("/dup.m4b"))
            .expect("the redo re-sets-aside dup");
        let original_job_dir = set_aside_root
            .join(job1.to_string())
            .to_string_lossy()
            .replace('\\', "/");
        assert!(
            requarantine
                .target_path
                .replace('\\', "/")
                .starts_with(&original_job_dir),
            "redo re-sets-aside under the ORIGINAL job's folder ({original_job_dir}), got {}",
            requarantine.target_path
        );

        apply_inverse_plan(&pool, redo.plan_id, &scope).await;
        assert_eq!(
            after_forward,
            tree_signature(&library_root),
            "the redo re-applies the forward change"
        );
    }

    /// AC-14: `rollback_prepare` produces a real, previewable inverse plan - it
    /// persists as a `plans` row over the same scan/ruleset, its ops carry F-404
    /// verdicts (none blocked), and it is fetchable through the same review query a
    /// forward plan uses.
    #[tokio::test]
    async fn inverse_plan_is_a_real_previewable_plan() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;
        let reports = TempDir::new().expect("reports dir");
        let export = export_after_apply(
            &pool,
            reports.path(),
            ApplyMode::Real,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export");

        let prepared = rollback_prepare(&pool, export.manifest_id, NOW)
            .await
            .expect("rollback_prepare");

        // It is a distinct, real plan over the same scan + ruleset.
        assert_ne!(prepared.plan_id, plan_id, "the inverse plan is a new plan");
        let header = get_plan(&pool, prepared.plan_id)
            .await
            .expect("get_plan")
            .expect("inverse plan exists");
        assert_eq!(header.scan_id, scan_id);
        assert_eq!(header.ruleset_id, ruleset_id);

        // Its ops passed F-404 (none blocked) and render through the same review
        // surface a forward plan uses.
        let review = crate::plan::query::plan_review_for(&pool, prepared.plan_id)
            .await
            .expect("review the inverse plan");
        assert_eq!(
            review.groups.len(),
            7,
            "the seven cards render for an undo plan too"
        );
        let ops = get_plan_ops(&pool, prepared.plan_id)
            .await
            .expect("inverse ops");
        assert!(
            ops.iter().all(|o| o.validation_state != "blocked"),
            "no inverse op is blocked by F-404 (AC-14)"
        );
    }

    /// A dry-run (rehearsal) undo file is refused with the plain-language
    /// "nothing to undo" error, never a panic or a generic failure (P2 semantic).
    #[tokio::test]
    async fn a_dry_run_undo_file_is_refused_plainly() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, "E:/lib").await;
        let plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &[NewPlanOp {
                op_group: "loose-root-books",
                kind: "move",
                kind_reason: None,
                source_path: "E:/lib/a.m4b",
                target_path: "E:/lib/Author/a.m4b",
                rationale: "r.",
                rule_id: "t",
                confidence: "high",
                byte_size: 1,
                validation_state: "valid",
                validation_reason: None,
                provenance_json: None,
            }],
            NOW,
        )
        .await
        .expect("plan");
        for row in get_plan_ops(&pool, plan_id).await.unwrap() {
            set_approval(&pool, row.id, "approved", NOW).await.unwrap();
        }
        let job_id = acquire_apply_job(&pool, ApplyMode::DryRun, NOW)
            .await
            .expect("job");
        let reports = TempDir::new().expect("reports");
        let applied = get_plan_ops(&pool, plan_id).await.unwrap();
        // Export a DRY-RUN undo file (a rehearsal).
        let export = export_after_apply(
            &pool,
            reports.path(),
            ApplyMode::DryRun,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export dry-run undo file");

        let err = rollback_prepare(&pool, export.manifest_id, NOW)
            .await
            .expect_err("a rehearsal has nothing to undo");
        assert_eq!(err.code(), "rollback-not-reversible");
    }

    /// AC-16 + the P4 substitution fix: a partial undo of the contiguous set-aside
    /// tail (the FD-34 ensure-mkdir at seq 4 and the set-aside move at seq 5)
    /// restores the item AND removes the per-job set-aside folder, leaving earlier
    /// ops applied. The ensure-mkdir carries the `{job-id}` placeholder in a `mkdir`
    /// target: the journal-tail reconstruction must substitute it exactly like the
    /// executor does (target-based, not kind-gated), or its inverse `rmdir-empty`
    /// would point at a literal `{job-id}` path and silently no-op.
    #[tokio::test]
    async fn partial_undo_of_the_set_aside_tail_reconstructs_the_ensure_mkdir() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;

        let dup_path = library_root
            .join("Genre")
            .join("Series")
            .join("Book")
            .join("dup.m4b");
        let dup_before = std::fs::read(&dup_path).unwrap();

        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;
        let per_job_dir = set_aside_root.join(job_id.to_string());
        assert!(
            per_job_dir.exists(),
            "the per-job set-aside folder was created by the forward apply"
        );

        // The set-aside group is the contiguous tail (seq 4 ensure-mkdir + seq 5
        // set-aside move).
        let tail_op_ids: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq >= 4)
            .map(|o| o.id)
            .collect();
        assert_eq!(tail_op_ids.len(), 2);

        let prepared = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &tail_op_ids, NOW)
            .await
            .expect("partial undo of the set-aside tail");
        // One move-back plus four teardown rmdir-empty ops (Book, Series, Genre,
        // per-job folder) synthesized from the nested set-aside target.
        assert_eq!(
            prepared.op_count, 5,
            "1 move-back + 4 set-aside teardown ops"
        );

        // Every set-aside teardown op targets a REAL substituted path, never the
        // literal `{job-id}` placeholder, and they are ordered deepest-first with the
        // per-job folder last among them.
        let inverse_ops = get_plan_ops(&pool, prepared.plan_id).await.expect("ops");
        let rmdirs: Vec<&PlanOpRow> = inverse_ops
            .iter()
            .filter(|o| o.kind == "rmdir-empty")
            .collect();
        assert_eq!(rmdirs.len(), 4, "four set-aside dirs are torn down");
        for r in &rmdirs {
            assert!(
                !r.source_path.contains(QUARANTINE_JOB_PLACEHOLDER),
                "no teardown op may carry the literal placeholder: {}",
                r.source_path
            );
        }
        assert_eq!(
            rmdirs.last().unwrap().source_path.replace('\\', "/"),
            per_job_dir.to_string_lossy().replace('\\', "/"),
            "the per-job folder is torn down last (shallowest)"
        );
        // Deepest-first: each rmdir is at least as deep as the next.
        let depths: Vec<usize> = rmdirs
            .iter()
            .map(|r| r.source_path.replace('\\', "/").matches('/').count())
            .collect();
        assert!(
            depths.windows(2).all(|w| w[0] >= w[1]),
            "teardown ops are ordered deepest-first: {depths:?}"
        );

        apply_inverse_plan(&pool, prepared.plan_id, &scope).await;

        // The tail was undone: the set-aside copy is back at its original nested
        // location, and the WHOLE per-job set-aside tree is removed (no residue).
        let dup_after = std::fs::read(&dup_path)
            .expect("the set-aside copy is restored to its original location");
        assert_eq!(dup_before, dup_after, "restored byte-for-byte");
        assert!(
            !per_job_dir.exists(),
            "the per-job set-aside folder and its intermediates are fully removed"
        );

        // The EARLIER ops stay applied.
        assert!(
            library_root.join("Author").join("loose-book.m4b").exists(),
            "an earlier op is left applied"
        );
        assert!(
            !library_root.join("EmptyShell").exists(),
            "the earlier empty-folder removal stays applied"
        );
        assert!(
            library_root.join("Messy Name").exists()
                && !library_root.join("Messy Name [128k]").exists(),
            "the earlier rename stays applied"
        );
    }

    /// AC-16: a NON-contiguous selection (skipping an op in the middle of the run)
    /// is refused, so no inverse plan is built for an unrepresentable state.
    #[tokio::test]
    async fn a_non_contiguous_selection_is_refused() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;

        // Select seq 0 and seq 5 - a gap in the middle, not a suffix.
        let non_contiguous: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq == 0 || o.seq == 5)
            .map(|o| o.id)
            .collect();
        assert_eq!(non_contiguous.len(), 2);

        let err = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &non_contiguous, NOW)
            .await
            .expect_err("a non-contiguous selection is refused");
        assert_eq!(err.code(), "rollback-selection-not-contiguous");

        // A tail that is not the LATEST run (seq 0..4, missing seq 5) is also refused.
        let not_latest: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq <= 4)
            .map(|o| o.id)
            .collect();
        let err2 = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &not_latest, NOW)
            .await
            .expect_err("a run that is not the latest is refused");
        assert_eq!(err2.code(), "rollback-selection-not-contiguous");
    }

    /// Safety of the teardown-last ordering: if a FOREIGN file appears under the
    /// set-aside root, the set-aside teardown `rmdir-empty` halts (never deleting
    /// the foreign file) - but because teardown runs AFTER every library
    /// restoration, the library is ALREADY fully restored (byte-identical), so the
    /// halt strands nothing. This is exactly the failure mode the reposition fixes:
    /// a teardown halt can no longer skip the early-seq library move-backs.
    #[tokio::test]
    async fn a_foreign_file_halts_teardown_without_stranding_restorations() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Audiobook Archive");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };

        let original = tree_signature(&library_root);
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;

        // A foreign file appears in the deepest set-aside folder (beside the set-aside
        // item), so the deepest teardown `rmdir-empty` cannot complete.
        let deepest = set_aside_root
            .join(job_id.to_string())
            .join("Genre")
            .join("Series")
            .join("Book");
        write_file(&deepest.join("foreign.txt"), b"not ours");

        let reports = TempDir::new().expect("reports");
        let export = export_after_apply(
            &pool,
            reports.path(),
            ApplyMode::Real,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export");
        let prepared = rollback_prepare(&pool, export.manifest_id, NOW)
            .await
            .expect("rollback_prepare");

        // Apply the inverse plan directly so we can observe the halt (the shared
        // helper asserts no halt).
        for row in get_plan_ops(&pool, prepared.plan_id).await.unwrap() {
            set_approval(&pool, row.id, "approved", NOW).await.unwrap();
        }
        let inv_ops = get_plan_ops(&pool, prepared.plan_id).await.unwrap();
        let undo_job = acquire_apply_job(&pool, ApplyMode::Real, NOW)
            .await
            .expect("undo job");
        let executor = Executor::with_scope(RealFs::new(), undo_job, inv_ops, scope.clone());
        let journal = SqliteJournal::new(pool.clone());
        let outcome = executor.run(&journal, NOW).await.expect("walk");

        // The teardown halted on the non-empty (foreign-file-holding) folder, and it
        // never deleted the foreign file.
        let halt = outcome
            .halt
            .expect("the teardown halts on the foreign file");
        assert_eq!(halt.code, "target-appeared");
        assert!(
            deepest.join("foreign.txt").exists(),
            "the foreign file is untouched"
        );

        // Decisively: the LIBRARY is fully restored (byte-identical), because every
        // library restoration ran before the teardown - the halt stranded nothing.
        assert_eq!(
            original,
            tree_signature(&library_root),
            "the library is fully restored despite the teardown halt"
        );
    }

    /// A memory-safe tree signature for AC-17's large real files: every entry's
    /// path relative to `root` (forward-slashed, sorted) mapped to `(size, digest)`
    /// where `digest` is a 64-bit hash of the file's bytes streamed in 64 KiB
    /// chunks. Never holds a whole file in memory, so it scales to multi-hundred-MB
    /// audiobooks. Deterministic (sorted keys, fixed chunking).
    #[cfg(test)]
    fn tree_stream_signature(root: &Path) -> BTreeMap<String, (u64, u64)> {
        use std::hash::Hasher as _;
        use std::io::Read as _;
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, (u64, u64)>) {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
                .expect("read_dir")
                .map(|e| e.expect("entry").path())
                .collect();
            entries.sort();
            for path in entries {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/");
                if path.is_dir() {
                    out.insert(format!("dir:{rel}"), (0, 0));
                    walk(root, &path, out);
                } else {
                    let mut file = std::fs::File::open(&path).expect("open");
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    let mut buf = vec![0u8; 64 * 1024];
                    let mut size = 0u64;
                    loop {
                        let n = file.read(&mut buf).expect("read");
                        if n == 0 {
                            break;
                        }
                        size += n as u64;
                        hasher.write(&buf[..n]);
                    }
                    out.insert(format!("file:{rel}"), (size, hasher.finish()));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    /// AC-17 manual evidence harness (ignored by default): perform the full
    /// round-trip on a COPY of a real library folder. Point it at a copy with:
    ///
    /// ```text
    /// ABO_RT_LIB=E:\tmp\abo-rt\top100\lib \
    /// ABO_RT_ASIDE=E:\tmp\abo-rt\top100\aside \
    ///   cargo test -p abo-core --lib exec::rollback::tests::ac17_real_copy_round_trip -- --ignored --exact --nocapture
    /// ```
    ///
    /// It builds a real forward plan over the copy's top-level books (mkdir a shelf,
    /// move each book onto it, set aside the last one - exercising the {job-id}
    /// substitution on real gnarly names), applies it for real, prepares the undo
    /// through `rollback_prepare` (a validated inverse plan), applies that, and
    /// asserts the tree is byte-identical by streaming hash. NEVER touches the real
    /// library: it only reads `ABO_RT_LIB`, which the caller has already pointed at
    /// a copy outside the library.
    #[tokio::test]
    #[ignore = "AC-17 manual evidence: needs a real copied folder via ABO_RT_LIB/ABO_RT_ASIDE"]
    async fn ac17_real_copy_round_trip() {
        let library_root = PathBuf::from(std::env::var("ABO_RT_LIB").expect("ABO_RT_LIB"));
        let set_aside_root = PathBuf::from(std::env::var("ABO_RT_ASIDE").expect("ABO_RT_ASIDE"));
        assert!(library_root.is_dir(), "ABO_RT_LIB must be a copied folder");
        // Loud refusal BEFORE any write: this harness applies REAL changes, so it
        // must never point at the real library. Guard against the literal real
        // library path (case- and separator-insensitive); the harness is manual, so
        // a hard-coded guard is sufficient.
        let lib_norm = library_root
            .to_string_lossy()
            .to_lowercase()
            .replace('\\', "/");
        assert!(
            !lib_norm.contains("books - audio"),
            "refusing to run: ABO_RT_LIB must be a COPY outside the real library \
             (E:\\Books - Audio), never the library itself"
        );

        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };

        // Enumerate the copy's top-level book folders (sorted, deterministic).
        let mut books: Vec<PathBuf> = std::fs::read_dir(&library_root)
            .expect("read copy")
            .map(|e| e.expect("entry").path())
            .filter(|p| p.is_dir())
            .collect();
        books.sort();
        assert!(
            books.len() >= 2,
            "need at least two book folders in the copy"
        );

        let lib = scope.library_root.clone();
        let aside = scope.set_aside_root.clone();
        let shelf = format!("{lib}/__abo_shelf__");
        let mut specs = vec![Spec {
            op_group: "loose-root-books",
            kind: "mkdir",
            source: String::new(),
            target: shelf.clone(),
            byte_size: 0,
        }];
        // Move every book but the last onto the shelf.
        for book in &books[..books.len() - 1] {
            let name = book.file_name().unwrap().to_string_lossy().to_string();
            specs.push(Spec {
                op_group: "loose-root-books",
                kind: "move",
                source: book.to_string_lossy().to_string(),
                target: format!("{shelf}/{name}"),
                byte_size: 0,
            });
        }
        // The FD-34 per-job set-aside folder (ensure-mkdir), then set the last book
        // aside under it - so the undo exercises the real set-aside teardown.
        specs.push(Spec {
            op_group: "dedupe-quarantine",
            kind: "mkdir",
            source: String::new(),
            target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}"),
            byte_size: 0,
        });
        let last = books.last().unwrap();
        let last_name = last.file_name().unwrap().to_string_lossy().to_string();
        specs.push(Spec {
            op_group: "dedupe-quarantine",
            kind: "quarantine",
            source: last.to_string_lossy().to_string(),
            target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}/{last_name}"),
            byte_size: 0,
        });

        let original = tree_stream_signature(&library_root);

        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;
        let after_forward = tree_stream_signature(&library_root);
        assert_ne!(
            original, after_forward,
            "the forward apply changed the copy"
        );

        let reports = TempDir::new().expect("reports");
        let export = export_after_apply(
            &pool,
            reports.path(),
            ApplyMode::Real,
            job_id,
            plan_id,
            &applied,
        )
        .await
        .expect("export undo file");

        let prepared = rollback_prepare(&pool, export.manifest_id, NOW)
            .await
            .expect("rollback_prepare");
        apply_inverse_plan(&pool, prepared.plan_id, &scope).await;

        let after_rollback = tree_stream_signature(&library_root);
        let identical = original == after_rollback;
        // The per-job set-aside folder is fully drained by the teardown (no residue).
        let per_job_drained = !set_aside_root.join(job_id.to_string()).exists();
        println!(
            "AC-17 ROUND-TRIP lib={lib} books={} forward_ops={} undo_ops={} BYTE_IDENTICAL={} SET_ASIDE_DRAINED={}",
            books.len(),
            applied.len(),
            prepared.op_count,
            if identical { "yes" } else { "no" },
            if per_job_drained { "yes" } else { "no" }
        );
        assert!(
            identical,
            "the copied tree must be byte-identical after the round trip"
        );
        assert!(
            per_job_drained,
            "the per-job set-aside folder must be torn down (no residue)"
        );
    }
}
