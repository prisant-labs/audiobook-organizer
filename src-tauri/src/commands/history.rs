//! History command handler - v0.6.0 (hardening): the record of past tidy-ups.
//!
//! Thin-adapter rule (same as [`super::apply`]): the read model and every decision
//! about what can be put back live in `abo_core::exec::history`; this module
//! orchestrates the pool and returns the typed result verbatim.
//!
//! The undo OFFER is deliberately computed in the engine rather than derived in
//! the shell. Which undo path applies depends on whether a manifest was exported,
//! whether its operations are reversible, whether anything landed, and whether
//! reconciliation left an operation ambiguous - engine invariants, not view state.
//! Re-deriving them in TypeScript would put a safety decision in the layer with
//! the least context.

use abo_core::exec::{list_history, HistoryEntry};
use abo_core::ipc::AppError;

use crate::AppState;

/// How many past tidy-ups a single History read returns by default.
///
/// Bounded so a long-lived library cannot make the screen an unbounded read; the
/// surface asks for more by passing a larger `limit`.
const DEFAULT_HISTORY_LIMIT: i64 = 50;

/// The most recent tidy-ups, newest first, each with its undo offer resolved
/// (v0.6.0). Practice runs are included and labelled as such - hiding them would
/// make the record lie by omission - but are never offered an undo.
///
/// `limit` is clamped to a sane range so a malformed or hostile caller cannot turn
/// this into an unbounded table scan.
#[tauri::command]
#[specta::specta]
pub async fn history_list(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<HistoryEntry>, AppError> {
    let limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT).clamp(1, 500);
    list_history(&state.pool, limit).await
}
