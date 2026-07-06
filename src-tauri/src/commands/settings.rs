//! F-803 (app settings) IPC command handlers.
//!
//! v0.4.0 (seeing) Phase 2. Two thin adapters over `abo_core::db::settings`,
//! the same pattern as the scan commands in [`super`]: pull the shared pool out
//! of managed [`AppState`](crate::AppState), call into the core, return the
//! typed result verbatim. No product logic lives here.
//!
//! [`settings_set`] additionally keeps the backend's in-memory sanctioned scan
//! root in lockstep with the persisted `library_root` (FD-29 re-allowance): when
//! the user picks or changes the library folder, the next scan uses the new root
//! without a restart. The frontend never touches the filesystem to do this - it
//! passes a path (obtained from the OS folder picker via `tauri-plugin-dialog`)
//! as a plain string through this typed IPC, and the backend owns every actual
//! filesystem access (architecture Section 5).

use std::path::PathBuf;

use abo_core::db::settings::{get_settings, set_settings};
use abo_core::ipc::{AppError, AppSettings};

use crate::AppState;

/// Read the singleton application settings (F-803, AC-34).
///
/// Thin wrapper over [`abo_core::db::settings::get_settings`]. The frontend calls
/// this at startup to route first-run (a `None` `library_root` means no library
/// is configured) and to seed the theme from persisted settings (FD-09).
#[tauri::command]
#[specta::specta]
pub async fn settings_get(state: tauri::State<'_, AppState>) -> Result<AppSettings, AppError> {
    get_settings(&state.pool).await
}

/// Persist the singleton application settings (F-803, AC-34) and return the
/// stored result.
///
/// After writing, this updates the backend's sanctioned scan root
/// ([`AppState::library_root`](crate::AppState::library_root)) to match the newly
/// persisted `library_root`, so a scan started after the user re-selects the
/// library folder operates on the new root immediately (FD-29 re-allowance). The
/// returned [`AppSettings`] is the normalized, persisted form (empty roots become
/// `None`, the theme is clamped to `day`/`evening`), so the caller sees exactly
/// what was stored.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    state: tauri::State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    let persisted = set_settings(&state.pool, &settings).await?;

    // Re-allow the persisted root for the backend's own use (there is no WebView
    // fs scope to manage; abo-core owns all filesystem access via std::fs). This
    // keeps scan_start's sanctioned root current after a re-selection.
    let next_root = persisted.library_root.as_ref().map(PathBuf::from);
    *state
        .library_root
        .lock()
        .expect("library_root mutex poisoned") = next_root;

    Ok(persisted)
}
