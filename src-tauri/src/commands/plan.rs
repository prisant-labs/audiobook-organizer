//! F-903 (plan review surface) IPC command handlers.
//!
//! v0.4.0 (seeing) Phase 5. Thin adapters over `abo_core::plan::{report,
//! query, validate}`, the same pattern as [`super::settings`]: pull the
//! shared pool out of managed [`AppState`](crate::AppState), call into the
//! core, return the typed result verbatim. No product logic lives here.
//!
//! [`plan_generate`] wraps the v0.3.0 full chain (build -> detect duplicates
//! -> validate -> persist) MINUS the F-505/F-507/F-506 Reports-folder file
//! writing (`abo_core::plan::report::build_and_persist_plan`), so the review
//! surface renders a REAL generated plan without also writing artifacts to
//! disk on every open; exporting the report stays a separate, explicit
//! action. It is a direct async command rather than a `job:*`-event flow:
//! the dedicated "Building the tidy-up plan" loading state and its Stop
//! control are P7 scope (T-29..T-32, AC-26), not this phase's AC-10..AC-20;
//! see this module's test coverage note and the task report for the
//! follow-up this leaves for P7.
//!
//! v0.4.0 Phase 6 (F-906) adds the live re-plan preview (`plan_preview`) and
//! switches `plan_generate` from its own ad hoc "first ruleset row" bootstrap
//! to the shared `commands::ruleset::ensure_active_ruleset` (the ACTIVE
//! ruleset, the one the F-906 editor's Apply/save semantics point at).

use abo_core::ipc::{AppError, PlanOpView, PlanOpsPage, PlanPreview, PlanReview};
use abo_core::plan::report::{build_and_persist_plan, preview_plan_review};
use abo_core::plan::validate::{
    set_group_approval, set_op_approval, ApprovalAction, ApprovalError,
};
use abo_core::plan::{builder::CampaignGroup, query};
use abo_core::ruleset::Ruleset;
use abo_core::scan::walk::now_iso8601_utc;

use crate::commands::ruleset::ensure_active_ruleset;
use crate::AppState;

/// Generate a REAL plan from a completed scan and return its review surface
/// (F-903): build -> detect duplicates -> validate -> persist, then the
/// seven group cards. `scan_id` is a completed snapshot's id (from
/// `classify_overview` or `job:completed`'s scan payload). Always builds
/// against the ACTIVE ruleset (F-906, [`ensure_active_ruleset`]), seeding the
/// shipped default (D-02 `abs-author-first`) on a brand-new database.
#[tauri::command]
#[specta::specta]
pub async fn plan_generate(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<PlanReview, AppError> {
    let active = ensure_active_ruleset(&state.pool).await?;
    let built = build_and_persist_plan(&state.pool, scan_id, active.id)
        .await
        .map_err(AppError::from)?;
    query::plan_review_for(&state.pool, built.plan_id).await
}

/// The F-906 ruleset editor's live re-plan preview (AC-33): re-plan `scan_id`
/// against `ruleset` (a DRAFT that may not yet be saved) and return the same
/// seven-card counts shape the review screen renders - WITHOUT persisting
/// anything (no `plans`/`plan_ops` row is ever written for a preview; see
/// [`abo_core::plan::report::preview_plan_review`]'s doc comment). Debounced
/// and cancellable on the frontend side (the same re-entrancy guard pattern
/// `usePlanReview` already establishes for `plan_generate`); this command
/// itself is a plain, side-effect-free read plus in-memory compute, so
/// calling it repeatedly and discarding stale responses is always safe.
#[tauri::command]
#[specta::specta]
pub async fn plan_preview(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
    ruleset: Ruleset,
) -> Result<PlanPreview, AppError> {
    ruleset.validate()?;
    preview_plan_review(&state.pool, scan_id, &ruleset)
        .await
        .map_err(AppError::from)
}

/// Re-fetch a previously generated plan's review surface (F-903), for
/// example after navigating back to review without regenerating.
#[tauri::command]
#[specta::specta]
pub async fn plan_get(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
) -> Result<PlanReview, AppError> {
    query::plan_review_for(&state.pool, plan_id).await
}

/// The flat, filterable op listing for one plan (F-503/F-504): every
/// non-`mkdir` row, capped defensively. The group-detail pane and the filter
/// box both read from this one list client-side.
#[tauri::command]
#[specta::specta]
pub async fn plan_list_ops(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
) -> Result<PlanOpsPage, AppError> {
    query::plan_ops_for(&state.pool, plan_id).await
}

/// Toggle a campaign group's include/skip switch (F-502, AC-11): `included =
/// true` approves every non-blocked, non-excluded op in the group (blocked
/// ops are skipped, never force-approved, AC-14/AC-17); `included = false`
/// rejects them (a deliberate "left out this round", reversible by toggling
/// back on). Returns the refreshed [`PlanReview`] so the caller's card state
/// and footer totals stay in lockstep with what was actually persisted.
#[tauri::command]
#[specta::specta]
pub async fn plan_set_group_approval(
    state: tauri::State<'_, AppState>,
    plan_id: i64,
    group: String,
    included: bool,
) -> Result<PlanReview, AppError> {
    let campaign_group =
        CampaignGroup::from_slug(&group).ok_or_else(|| AppError::PlanGenerationFailed {
            detail: format!("unknown campaign group: {group}"),
        })?;
    let action = if included {
        ApprovalAction::Approve
    } else {
        ApprovalAction::Reject
    };
    let now = now_iso8601_utc();
    set_group_approval(&state.pool, plan_id, campaign_group, action, &now)
        .await
        .map_err(approval_err_to_app)?;
    query::plan_review_for(&state.pool, plan_id).await
}

/// Exclude one operation from a plan (F-502/AC-13): drops it to
/// `no-op(user-excluded)` on the approval axis (the descriptive columns are
/// never rewritten, only the mutable `approval` pair). Works on a `blocked`
/// op too (AC-14: exclude is always available even when include is not).
/// Returns the refreshed [`PlanOpView`] so the caller can patch just that
/// row without re-fetching the whole listing.
#[tauri::command]
#[specta::specta]
pub async fn plan_exclude_op(
    state: tauri::State<'_, AppState>,
    plan_op_id: i64,
) -> Result<PlanOpView, AppError> {
    let now = now_iso8601_utc();
    set_op_approval(&state.pool, plan_op_id, ApprovalAction::Exclude, &now)
        .await
        .map_err(approval_err_to_app)?;
    let row = abo_core::db::plans::get_plan_op(&state.pool, plan_op_id)
        .await
        .map_err(|e| AppError::PlanGenerationFailed {
            detail: e.to_string(),
        })?
        .ok_or(AppError::PlanNotFound {
            plan_id: plan_op_id,
        })?;
    Ok(query::build_plan_op_view(&row))
}

/// Map an [`ApprovalError`] onto the stable IPC taxonomy. `BlockedCannotBeApproved`
/// is not reachable through this module's own call sites today (group
/// approval skips blocked ops rather than erroring, and exclude is always
/// allowed), but the mapping is total so a future direct-approve command can
/// reuse it safely.
fn approval_err_to_app(e: ApprovalError) -> AppError {
    match e {
        ApprovalError::BlockedCannotBeApproved => AppError::PlanGenerationFailed {
            detail: "a blocked operation cannot be approved; exclude it or fix the ruleset and \
                     regenerate"
                .to_string(),
        },
        ApprovalError::Db(e) => AppError::PlanGenerationFailed {
            detail: e.to_string(),
        },
    }
}
