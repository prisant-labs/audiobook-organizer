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

use abo_core::ipc::{
    ApplyOpExecutedPayload, JobCompletedPayload, JobFailedPayload, JobProgressPayload,
    JobStoppedPayload,
};

// Re-emit note: `job:progress` is now genuinely emitted (F-104), unlike the
// v0.1.0 spine where it was frozen-but-never-emitted. The event, payload, and
// wire name are unchanged, so the generated bindings surface is unperturbed;
// only the emitter below is new.

/// Typed `job:completed` event, emitted when a spawned scan OR apply finishes
/// cleanly. Listeners filter by `job_id` (a job id is unique across scan and
/// apply), so an apply completion never disturbs a scan listener and vice versa.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:completed")]
pub struct JobCompleted(pub JobCompletedPayload);

/// Typed `job:failed` event, emitted when a spawned scan OR apply errors. Carries
/// the stable machine `code`; listeners filter by `job_id`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:failed")]
pub struct JobFailed(pub JobFailedPayload);

/// Typed `job:stopped` event (P8, IMPORTANT 3), emitted after an apply job's
/// DISTINCT `stopped` terminal state is durably marked. Mirrors the completed /
/// failed terminal events so the activity surface transitions reliably rather than
/// racing the walk's final state write. Fire-and-forget, post-durable-state.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "job:stopped")]
pub struct JobStopped(pub JobStoppedPayload);

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

/// Emit `job:failed` for a scan or apply job that errored, carrying the stable
/// machine `code`. Best-effort, like [`emit_job_completed`].
pub fn emit_job_failed(app: &AppHandle, job_id: i64, code: &str) {
    let _ = JobFailed(JobFailedPayload {
        job_id,
        code: code.to_string(),
    })
    .emit(app);
}

/// Emit `job:stopped` for an apply job that ended via a cooperative Stop (P8,
/// IMPORTANT 3). Best-effort, like [`emit_job_completed`]: emitted only after the
/// `stopped` state is durably marked, so a dropped event never perturbs the walk
/// and the status poll remains the fallback.
pub fn emit_job_stopped(app: &AppHandle, job_id: i64) {
    let _ = JobStopped(JobStoppedPayload { job_id }).emit(app);
}

/// Typed `apply:op-executed` event (P8 prelude 0b), emitted after each operation's
/// `done` journal row is committed in a running apply job. The event is
/// fire-and-forget: a dropped event never fails the apply.
///
/// Wire name pinned explicitly so a rename of the Rust struct does not silently
/// break the frontend listener. The frontend listens via the generated
/// `events.applyOpExecuted` binding, never a raw string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "apply:op-executed")]
pub struct JobOpExecuted(pub ApplyOpExecutedPayload);

/// Emit `apply:op-executed` for one completed operation. Best-effort: a dropped
/// event (for example before the webview exists) never fails the apply.
pub fn emit_apply_op_executed(app: &AppHandle, payload: ApplyOpExecutedPayload) {
    let _ = JobOpExecuted(payload).emit(app);
}

/// Emit `job:progress` for a running scan (F-104), carrying the units done so
/// far, the best-known total (`None` while indeterminate), and a short label
/// (the current path). Best-effort, like [`emit_job_completed`]: a dropped
/// progress event (for example before the webview exists) never fails the scan.
pub fn emit_job_progress(
    app: &AppHandle,
    job_id: i64,
    done: i64,
    total_estimate: Option<i64>,
    current_label: &str,
) {
    let _ = JobProgress(JobProgressPayload {
        job_id,
        done,
        total_estimate,
        current_label: current_label.to_string(),
    })
    .emit(app);
}
