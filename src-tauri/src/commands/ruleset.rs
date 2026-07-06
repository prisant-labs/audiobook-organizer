//! F-906 (ruleset editor with live re-plan) IPC command handlers.
//!
//! v0.4.0 (seeing) Phase 6. Thin adapters over `abo_core::{ruleset,
//! db::rulesets}`, the same pattern as [`super::settings`] and [`super::plan`]:
//! pull the shared pool out of managed [`AppState`](crate::AppState), call
//! into the core, return the typed result verbatim. No product logic lives
//! here beyond the one policy decision CRUD plumbing does not own itself:
//! [`ruleset_delete`]'s "not the active ruleset" guard (see its doc comment).
//!
//! "Apply/save semantics" (AC-32, AC-33): [`ruleset_save`] always makes the
//! saved row the ACTIVE ruleset, so [`super::plan::plan_generate`] (which
//! reads the active ruleset) regenerates from exactly what was just saved -
//! this is the mechanism behind "a saved change persists the active ruleset;
//! the review screen regenerates from it."

use abo_core::db::rulesets::{
    count_rulesets, delete_ruleset, get_active_ruleset, get_ruleset, insert_ruleset, list_rulesets,
    set_active_ruleset, update_ruleset_body, NewRuleset,
};
use abo_core::ipc::{
    AppError, PresetExampleView, RulesetDetail, RulesetSaveRequest, RulesetSummary,
};
use abo_core::plan::templates::{example_path, Preset};
use abo_core::ruleset::{parse_and_validate, Ruleset};
use abo_core::scan::walk::now_iso8601_utc;

use crate::AppState;

/// List every saved ruleset (F-801 CRUD, AC-32), lightest-weight shape (no
/// policy body) for the editor's "load a different saved ruleset" affordance.
#[tauri::command]
#[specta::specta]
pub async fn ruleset_list(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RulesetSummary>, AppError> {
    let rows = list_rulesets(&state.pool).await.map_err(db_err)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let ruleset = parse_and_validate(&row.body_json)?;
        out.push(RulesetSummary {
            id: row.id,
            name: row.name,
            preset: ruleset.naming.preset,
            is_active: row.is_active,
            updated_at: row.updated_at,
        });
    }
    Ok(out)
}

/// Read one ruleset's full editable detail (F-801 CRUD, AC-32).
#[tauri::command]
#[specta::specta]
pub async fn ruleset_get(
    state: tauri::State<'_, AppState>,
    ruleset_id: i64,
) -> Result<RulesetDetail, AppError> {
    let row = get_ruleset(&state.pool, ruleset_id)
        .await
        .map_err(db_err)?
        .ok_or(AppError::RulesetNotFound { ruleset_id })?;
    let ruleset = parse_and_validate(&row.body_json)?;
    Ok(RulesetDetail {
        id: row.id,
        name: row.name,
        is_active: row.is_active,
        ruleset,
    })
}

/// The active ruleset the review screen currently regenerates from, seeding
/// the shipped default (D-02 `abs-author-first`) on a brand-new database
/// that has never saved or activated any ruleset yet. Shared with
/// `commands::plan::plan_generate`'s bootstrap so the two never disagree on
/// what "active" means.
pub async fn ensure_active_ruleset(
    pool: &sqlx::SqlitePool,
) -> Result<abo_core::db::rulesets::RulesetRow, AppError> {
    if let Some(row) = get_active_ruleset(pool).await.map_err(db_err)? {
        return Ok(row);
    }
    let now = now_iso8601_utc();
    // A pre-F-906 database may already hold a ruleset row (from the old
    // `ensure_default_ruleset` bootstrap) with none marked active yet;
    // activate the first one rather than seeding a redundant second row.
    if let Some(first) = list_rulesets(pool)
        .await
        .map_err(db_err)?
        .into_iter()
        .next()
    {
        set_active_ruleset(pool, first.id, &now)
            .await
            .map_err(db_err)?;
        return get_ruleset(pool, first.id).await.map_err(db_err)?.ok_or(
            AppError::RulesetNotFound {
                ruleset_id: first.id,
            },
        );
    }
    let body = Ruleset::default().to_body_json();
    let id = insert_ruleset(
        pool,
        &NewRuleset {
            name: "Default",
            body_json: &body,
            schema_version: abo_core::ruleset::RULESET_SCHEMA_VERSION,
        },
        &now,
    )
    .await
    .map_err(db_err)?;
    set_active_ruleset(pool, id, &now).await.map_err(db_err)?;
    get_ruleset(pool, id)
        .await
        .map_err(db_err)?
        .ok_or(AppError::RulesetNotFound { ruleset_id: id })
}

/// Read the active ruleset's full editable detail, seeding the shipped
/// default (D-02) if nothing has ever been saved or activated yet - so the
/// F-906 editor always has something to load, even before the user's first
/// scan/plan (which is what previously bootstrapped the one ruleset row).
#[tauri::command]
#[specta::specta]
pub async fn ruleset_get_active(
    state: tauri::State<'_, AppState>,
) -> Result<RulesetDetail, AppError> {
    let row = ensure_active_ruleset(&state.pool).await?;
    let ruleset = parse_and_validate(&row.body_json)?;
    Ok(RulesetDetail {
        id: row.id,
        name: row.name,
        is_active: row.is_active,
        ruleset,
    })
}

/// Create or update a ruleset and make it the ACTIVE one (F-801 CRUD, AC-32;
/// AC-33's "apply/save" semantics). Validates the body BEFORE writing
/// anything (AC-29, `Ruleset::validate`), so a malformed draft is never
/// persisted half-valid.
#[tauri::command]
#[specta::specta]
pub async fn ruleset_save(
    state: tauri::State<'_, AppState>,
    request: RulesetSaveRequest,
) -> Result<RulesetDetail, AppError> {
    request.ruleset.validate()?;
    let body = request.ruleset.to_body_json();
    let now = now_iso8601_utc();

    let id = match request.id {
        Some(id) => {
            update_ruleset_body(
                &state.pool,
                id,
                &request.name,
                &body,
                request.ruleset.schema_version,
                &now,
            )
            .await
            .map_err(db_err)?;
            id
        }
        None => insert_ruleset(
            &state.pool,
            &NewRuleset {
                name: &request.name,
                body_json: &body,
                schema_version: request.ruleset.schema_version,
            },
            &now,
        )
        .await
        .map_err(db_err)?,
    };

    set_active_ruleset(&state.pool, id, &now)
        .await
        .map_err(db_err)?;

    Ok(RulesetDetail {
        id,
        name: request.name,
        is_active: true,
        ruleset: request.ruleset,
    })
}

/// Delete a saved ruleset (F-801 CRUD, AC-32). Refuses to delete the
/// currently ACTIVE ruleset ([`AppError::RulesetInUse`]): deleting it would
/// leave nothing for `plan_generate` to build against, so the caller must
/// activate a different ruleset first (`ruleset_save` on another row, or a
/// future dedicated activate action).
#[tauri::command]
#[specta::specta]
pub async fn ruleset_delete(
    state: tauri::State<'_, AppState>,
    ruleset_id: i64,
) -> Result<(), AppError> {
    let row = get_ruleset(&state.pool, ruleset_id)
        .await
        .map_err(db_err)?
        .ok_or(AppError::RulesetNotFound { ruleset_id })?;
    if row.is_active {
        return Err(AppError::RulesetInUse { ruleset_id });
    }
    delete_ruleset(&state.pool, ruleset_id)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// How many rulesets are currently saved (used by the frontend to decide
/// whether deleting the active one is even a reachable action - it never is
/// with only one saved, since that one is always active).
#[tauri::command]
#[specta::specta]
pub async fn ruleset_count(state: tauri::State<'_, AppState>) -> Result<i64, AppError> {
    count_rulesets(&state.pool).await.map_err(db_err)
}

/// The three F-401 shipped presets, each rendered against one fixed sample
/// book (`abo_core::plan::templates::example_path`), for the editor's preset
/// picker (F-906: "three presets with plain-language descriptions + example
/// path preview rendered from each template" - the plain-language
/// descriptions live in the frontend's centralized strings module, FD-23;
/// this command supplies only the rendered example path per preset).
#[tauri::command]
#[specta::specta]
pub fn ruleset_preset_examples(series_index_width: usize) -> Vec<PresetExampleView> {
    Preset::ALL
        .into_iter()
        .map(|preset| PresetExampleView {
            preset,
            example_path: example_path(preset, series_index_width),
        })
        .collect()
}

fn db_err(e: sqlx::Error) -> AppError {
    AppError::RulesetOperationFailed {
        detail: e.to_string(),
    }
}
