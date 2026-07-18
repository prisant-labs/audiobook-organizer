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
const ROLLBACK_RULE_ID: &str = "rollback-inverse";

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

/// Prepare an undo for a COMPLETED apply from its undo file (AC-14).
///
/// Reads the exported undo file (self-contained, real substituted paths), decides
/// the inverse operation list in undo order (latest forward change first), and
/// persists it as a new draft plan that passes the SAME F-404 checks and is
/// previewable through the SAME review surface a forward plan uses. Refuses a
/// rehearsal (dry-run) undo file with the plain-language
/// [`AppError::RollbackNotReversible`] (P2 safety semantic).
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

    // 4. The set of paths present on disk after the forward apply: every forward op
    //    that CREATED a target left that target present (files, set-aside items, and
    //    created directories). This is the validator's existing-paths view.
    let present: HashSet<String> = manifest
        .ops
        .iter()
        .filter(|o| {
            matches!(o.kind.as_str(), "move" | "rename" | "quarantine" | "mkdir")
                && !o.target_path.is_empty()
        })
        .map(|o| o.target_path.clone())
        .collect();

    // 5. Invert in REVERSE seq order (undo the latest forward change first).
    let mut ordered = manifest.ops.clone();
    ordered.sort_by_key(|o| std::cmp::Reverse(o.seq));
    let mut inverses = Vec::new();
    for op in &ordered {
        let (op_group, byte_size) = meta
            .get(&op.op_id)
            .cloned()
            .unwrap_or_else(|| (String::new(), 0));
        if let Some(inv) = invert(
            &op.kind,
            &op.source_path,
            &op.target_path,
            &op_group,
            byte_size,
        )? {
            inverses.push(inv);
        }
    }

    // 6. Scope + persist through F-404 (AC-14).
    let (scan_id, ruleset_id, library_root) = plan_scope_identity(pool, manifest.plan_id).await?;
    let set_aside_root = resolve_set_aside_root(pool, &library_root).await;
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

    // 4. Reconstruct each selected op's REAL executed paths, invert them, and build
    //    the present-on-disk set from the real forward targets. Reverse seq order
    //    (undo the latest first).
    let mut ordered: Vec<(i64, i64)> = tail_slice.to_vec();
    ordered.sort_by_key(|(_, seq)| std::cmp::Reverse(*seq));

    let job_seg = job_id.to_string();
    let mut inverses = Vec::new();
    let mut present: HashSet<String> = HashSet::new();
    for (op_id, _seq) in &ordered {
        let op = by_id
            .get(op_id)
            .ok_or_else(|| AppError::RollbackPrepareFailed {
                detail: "a change to undo is no longer recorded".to_string(),
            })?;
        // The REAL executed source/target. Every path but a set-aside target is
        // literal in the frozen row; a set-aside target carries the {job-id}
        // placeholder, reconstructed by the SAME substitution the executor did.
        let real_target = if op.kind == "quarantine" {
            let reconstructed = op.target_path.replace(QUARANTINE_JOB_PLACEHOLDER, &job_seg);
            // P4 obligation: verify the reconstructed set-aside source exists before
            // building the inverse op - a missing reconstructed path is a validation
            // failure, not a guess.
            if !vfs.exists(Path::new(&reconstructed)) {
                return Err(AppError::RollbackPrepareFailed {
                    detail: format!(
                        "the set-aside location to restore from could not be found: {reconstructed}"
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
        if let Some(inv) = invert(
            &op.kind,
            &op.source_path,
            &real_target,
            &op.op_group,
            op.byte_size,
        )? {
            inverses.push(inv);
        }
    }

    let (scan_id, ruleset_id, library_root) = plan_scope_identity(pool, plan_id).await?;
    let set_aside_root = resolve_set_aside_root(pool, &library_root).await;
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

    /// The five-op fixture that exercises every reversible kind: mkdir, move,
    /// rename (a folder), quarantine (set-aside, with the {job-id} placeholder),
    /// and rmdir-empty. Builds the on-disk tree under `library_root` and returns
    /// the forward specs.
    fn build_fixture(library_root: &Path, set_aside_root: &Path) -> Vec<Spec> {
        // Original tree.
        write_file(&library_root.join("loose-book.m4b"), b"loose book bytes");
        write_file(
            &library_root.join("Messy Name [128k]").join("track.m4b"),
            b"messy track bytes",
        );
        write_file(
            &library_root.join("Duplicates").join("extra.m4b"),
            b"extra copy bytes",
        );
        std::fs::create_dir_all(library_root.join("EmptyShell")).expect("empty shell");

        let lib = library_root.to_string_lossy().to_string();
        let aside = set_aside_root.to_string_lossy().to_string();
        vec![
            Spec {
                op_group: "loose-root-books",
                kind: "mkdir",
                source: String::new(),
                target: format!("{lib}/Author"),
                byte_size: 0,
            },
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
                op_group: "dedupe-quarantine",
                kind: "quarantine",
                source: format!("{lib}/Duplicates/extra.m4b"),
                target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}/Duplicates/extra.m4b"),
                byte_size: 16,
            },
            Spec {
                op_group: "empty-cleanup",
                kind: "rmdir-empty",
                source: format!("{lib}/EmptyShell"),
                target: String::new(),
                byte_size: 0,
            },
        ]
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
        let set_aside_root = work.path().join("Set Aside");
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
        assert_eq!(
            prepared.op_count, 5,
            "five reversible ops invert to five undo ops"
        );
        apply_inverse_plan(&pool, prepared.plan_id, &scope).await;

        // Signature AFTER the round trip: byte-identical to the original.
        let after_rollback = tree_signature(&library_root);
        assert_eq!(
            original, after_rollback,
            "the library tree must be byte-identical after the round trip"
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
        let set_aside_root = work.path().join("Set Aside");
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

    /// AC-16: a partial undo of a contiguous journal tail restores exactly the tail
    /// (here the set-aside and the empty-folder removal), reconstructs the set-aside
    /// location from the job id + placeholder, and leaves the earlier ops applied.
    #[tokio::test]
    async fn partial_undo_of_a_contiguous_tail_restores_only_the_tail() {
        let db = TempDir::new().expect("db dir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let work = TempDir::new().expect("work dir");
        let library_root = work.path().join("library");
        let set_aside_root = work.path().join("Set Aside");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;

        // Signature of the tail's inputs BEFORE apply (the two things the tail undo
        // must restore: the Duplicates copy and the empty shell).
        let dup_before = std::fs::read(library_root.join("Duplicates").join("extra.m4b")).unwrap();

        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;

        // The last two forward ops (seq 3 quarantine, seq 4 rmdir-empty) are the
        // contiguous tail. Their op ids:
        let tail_op_ids: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq >= 3)
            .map(|o| o.id)
            .collect();
        assert_eq!(tail_op_ids.len(), 2);

        let prepared = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &tail_op_ids, NOW)
            .await
            .expect("partial undo of the tail");
        assert_eq!(prepared.op_count, 2, "only the two tail ops invert");
        apply_inverse_plan(&pool, prepared.plan_id, &scope).await;

        // The tail was undone: the set-aside copy is back, and the empty folder is
        // recreated.
        assert!(
            library_root.join("EmptyShell").exists(),
            "the empty folder is restored"
        );
        let dup_after = std::fs::read(library_root.join("Duplicates").join("extra.m4b"))
            .expect("the set-aside copy is restored to its original location");
        assert_eq!(dup_before, dup_after, "restored byte-for-byte");

        // The EARLIER ops stay applied: the loose book stays in its author folder,
        // and the messy folder stays renamed.
        assert!(
            library_root.join("Author").join("loose-book.m4b").exists(),
            "an earlier op is left applied"
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
        let set_aside_root = work.path().join("Set Aside");
        std::fs::create_dir_all(&library_root).expect("library");
        let specs = build_fixture(&library_root, &set_aside_root);
        let scope = ApplyScope {
            library_root: library_root.to_string_lossy().to_string(),
            set_aside_root: set_aside_root.to_string_lossy().to_string(),
        };
        let (scan_id, ruleset_id) = seed_scan_and_ruleset(&pool, &scope.library_root).await;
        let plan_id = persist_forward_plan(&pool, scan_id, ruleset_id, &specs).await;
        let (job_id, applied) = apply_for_real(&pool, plan_id, &scope).await;

        // Select seq 0 and seq 4 - a gap in the middle, not a suffix.
        let non_contiguous: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq == 0 || o.seq == 4)
            .map(|o| o.id)
            .collect();
        assert_eq!(non_contiguous.len(), 2);

        let err = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &non_contiguous, NOW)
            .await
            .expect_err("a non-contiguous selection is refused");
        assert_eq!(err.code(), "rollback-selection-not-contiguous");

        // A tail that is not the LATEST run (seq 0..3, missing seq 4) is also refused.
        let not_latest: Vec<i64> = applied
            .iter()
            .filter(|o| o.seq <= 3)
            .map(|o| o.id)
            .collect();
        let err2 = rollback_prepare_partial(&pool, &RealFs::new(), job_id, &not_latest, NOW)
            .await
            .expect_err("a run that is not the latest is refused");
        assert_eq!(err2.code(), "rollback-selection-not-contiguous");
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
        // Move every book but the last onto the shelf; set aside the last book.
        for (i, book) in books.iter().enumerate() {
            let name = book.file_name().unwrap().to_string_lossy().to_string();
            let src = book.to_string_lossy().to_string();
            if i + 1 < books.len() {
                specs.push(Spec {
                    op_group: "loose-root-books",
                    kind: "move",
                    source: src,
                    target: format!("{shelf}/{name}"),
                    byte_size: 0,
                });
            } else {
                specs.push(Spec {
                    op_group: "dedupe-quarantine",
                    kind: "quarantine",
                    source: src,
                    target: format!("{aside}/{QUARANTINE_JOB_PLACEHOLDER}/{name}"),
                    byte_size: 0,
                });
            }
        }

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
        println!(
            "AC-17 ROUND-TRIP lib={lib} books={} forward_ops={} undo_ops={} BYTE_IDENTICAL={}",
            books.len(),
            applied.len(),
            prepared.op_count,
            if identical { "yes" } else { "no" }
        );
        assert!(
            identical,
            "the copied tree must be byte-identical after the round trip"
        );
    }
}
