//! Backend to frontend event emission (the typed event surface).
//!
//! v0.1.0 spine, Phase 5. Long-running work in `abo-core` surfaces to the UI as
//! typed Tauri events emitted from here. Each event payload type lives in the
//! Tauri-free core ([`abo_core::ipc`]); the `tauri_specta::Event` wrapper and the
//! emit helpers are the only Tauri-aware pieces and stay in this shell, so the
//! core keeps its zero-tauri invariant (AC-3).
//!
//! Wire names are pinned explicitly (`job:completed`, `job:failed`,
//! `job:progress`); without `event_name` the derive would emit the struct
//! identifier. The frontend listens via the generated `events.jobCompleted`
//! bindings, never a raw string.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use abo_core::ipc::{JobCompletedPayload, JobFailedPayload, JobProgressPayload};

/// Typed `job:completed` event, emitted when a spawned scan finishes cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:completed")]
pub struct JobCompleted(pub JobCompletedPayload);

/// Typed `job:failed` event, emitted when a spawned scan errors.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:failed")]
pub struct JobFailed(pub JobFailedPayload);

/// Typed `job:progress` event.
///
/// DEFINED but NEVER EMITTED in the v0.1.0 spine (this phase's brief): the spine
/// scan is one spawned unit with no progress reporting or cancellation. It is
/// collected into the `tauri_specta::Builder` alongside the other two so the
/// generated bindings surface freezes the whole job-event contract now; a later
/// release adds the emitter as an additive change. There is deliberately no
/// `emit_*` helper for it here, precisely because nothing emits it yet.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:progress")]
pub struct JobProgress(pub JobProgressPayload);

/// Emit `job:completed` for a finished scan job. Best-effort: an emit failure
/// (for example no webview yet) is swallowed so a scan that already succeeded in
/// the core is never reported as failed to the caller.
pub fn emit_job_completed(app: &AppHandle, job_id: i64, scan_id: i64) {
    let _ = JobCompleted(JobCompletedPayload { job_id, scan_id }).emit(app);
}

/// Emit `job:failed` for a scan job that errored, carrying the stable machine
/// `code`. Best-effort, like [`emit_job_completed`].
pub fn emit_job_failed(app: &AppHandle, job_id: i64, code: &str) {
    let _ = JobFailed(JobFailedPayload {
        job_id,
        code: code.to_string(),
    })
    .emit(app);
}
