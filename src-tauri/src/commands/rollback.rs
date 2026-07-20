//! F-604 undo command handlers - v0.5.0 (acting) Phase 5 (rollback as a plan).
//!
//! Preparing an undo builds the INVERSE of an applied tidy-up as an ordinary plan
//! and persists it (D-09: rollback is not a special code path), then returns the
//! new plan id so the caller navigates to the SAME review surface a forward plan
//! uses. Thin-adapter rule (same as [`super::apply`]): the product logic lives in
//! `abo_core::exec::rollback`; this module orchestrates the pool and the clock and
//! returns the typed result verbatim.
//!
//! Two entry points mirror the core's two (see `abo_core::exec::rollback`):
//! [`rollback_prepare`] undoes a COMPLETED apply from its exported undo file;
//! [`rollback_prepare_partial`] undoes a contiguous tail of a halted/partial apply
//! from the journal, reconstructing set-aside locations through the real
//! filesystem (`RealFs`).

use abo_core::exec::RealFs;
use abo_core::ipc::{AppError, RollbackPrepared};
use abo_core::scan::walk::now_iso8601_utc;

use crate::AppState;

/// Prepare an undo of a completed tidy-up (F-604, AC-14): produce a validated,
/// previewable inverse plan from its undo file and return the new plan id.
#[tauri::command]
#[specta::specta]
pub async fn rollback_prepare(
    state: tauri::State<'_, AppState>,
    manifest_id: i64,
) -> Result<RollbackPrepared, AppError> {
    let now = now_iso8601_utc();
    abo_core::exec::rollback_prepare(&state.pool, manifest_id, &now).await
}

/// Prepare a partial undo of a contiguous tail of the most recent changes a
/// tidy-up made (F-604, AC-16): reconstructs the inverse from the journal (for a
/// halted or partially-applied run that exported no undo file) and refuses a
/// non-contiguous selection. Set-aside locations are reconstructed and verified
/// against the real filesystem.
#[tauri::command]
#[specta::specta]
pub async fn rollback_prepare_partial(
    state: tauri::State<'_, AppState>,
    job_id: i64,
    tail_op_ids: Vec<i64>,
) -> Result<RollbackPrepared, AppError> {
    let now = now_iso8601_utc();
    abo_core::exec::rollback_prepare_partial(
        &state.pool,
        &RealFs::new(),
        job_id,
        &tail_op_ids,
        &now,
    )
    .await
}
