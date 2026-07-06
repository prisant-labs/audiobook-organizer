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
//! No ruleset editor exists yet (F-906 is Phase 6, T-26): `plan_generate`
//! bootstraps the one default ruleset (D-02 `abs-author-first`) on first use
//! via [`ensure_default_ruleset`] rather than taking a `ruleset_id` argument
//! the frontend has no UI to supply yet.

use abo_core::db::rulesets::{insert_ruleset, list_rulesets, NewRuleset};
use abo_core::ipc::{AppError, PlanOpView, PlanOpsPage, PlanReview};
use abo_core::plan::report::build_and_persist_plan;
use abo_core::plan::validate::{
    set_group_approval, set_op_approval, ApprovalAction, ApprovalError,
};
use abo_core::plan::{builder::CampaignGroup, query};
use abo_core::ruleset::default_ruleset;
use abo_core::scan::walk::now_iso8601_utc;

use crate::AppState;

/// Build (or reuse) the one default ruleset row (D-02, FD-05) and return its
/// id. There is no ruleset editor yet (F-906, Phase 6), so this is the
/// bootstrap every `plan_generate` call uses until that surface lands.
async fn ensure_default_ruleset(pool: &sqlx::SqlitePool) -> Result<i64, AppError> {
    let existing = list_rulesets(pool)
        .await
        .map_err(|e| AppError::PlanGenerationFailed {
            detail: e.to_string(),
        })?;
    if let Some(first) = existing.into_iter().next() {
        return Ok(first.id);
    }
    let body = default_ruleset().to_body_json();
    let now = now_iso8601_utc();
    insert_ruleset(
        pool,
        &NewRuleset {
            name: "Default",
            body_json: &body,
            schema_version: 1,
        },
        &now,
    )
    .await
    .map_err(|e| AppError::PlanGenerationFailed {
        detail: e.to_string(),
    })
}

/// Generate a REAL plan from a completed scan and return its review surface
/// (F-903): build -> detect duplicates -> validate -> persist, then the
/// seven group cards. `scan_id` is a completed snapshot's id (from
/// `classify_overview` or `job:completed`'s scan payload).
#[tauri::command]
#[specta::specta]
pub async fn plan_generate(
    state: tauri::State<'_, AppState>,
    scan_id: i64,
) -> Result<PlanReview, AppError> {
    let ruleset_id = ensure_default_ruleset(&state.pool).await?;
    let built = build_and_persist_plan(&state.pool, scan_id, ruleset_id)
        .await
        .map_err(AppError::from)?;
    query::plan_review_for(&state.pool, built.plan_id).await
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
