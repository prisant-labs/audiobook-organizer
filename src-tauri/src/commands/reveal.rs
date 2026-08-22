//! F-610 (open a folder in the OS file manager) IPC handler, v0.6.0 `P10`.
//!
//! A thin adapter, deliberately. Everything that decides anything lives in
//! [`abo_core::reveal`]: the containment gate, the canonicalization, the
//! platform launch. This file's whole job is to fetch the sanctioned roots from
//! settings and hand them over.
//!
//! That split is the point rather than a style preference. `AC-48`'s refusal is
//! the only thing standing between a minimal-capability WebView and a general
//! "open any path on this machine" primitive, and a gate implemented in the
//! command layer would be re-implemented (or forgotten) by the next caller. The
//! same reasoning moved `AC-12`'s duplicate gate down into `abo_core`, and the
//! same reasoning is why precondition 3 is still open: a rule that lives in the
//! caller is a convention, not a mechanism.

use abo_core::db::settings::get_settings;
use abo_core::ipc::AppError;
use abo_core::reveal::RevealRoot;

use crate::AppState;

/// Open the OS file manager at `path` (`AC-47`), refusing anything outside the
/// library root or the Archive root (`AC-48`).
///
/// The roots are read from settings on every call rather than cached, so
/// re-pointing the library takes effect immediately and a stale root can never
/// widen what this command will open. A file path opens its containing folder
/// with the file selected, which is what `AC-49` needs: the affordance appears
/// wherever a path is shown, and many of those paths are files.
///
/// Returns `RevealRefused` for a path that cannot be proven to sit inside a
/// sanctioned root, including one that no longer exists, and `RevealFailed` if
/// the file manager itself would not start. Never returns `Ok` without having
/// asked the OS to open something.
#[tauri::command]
#[specta::specta]
pub async fn reveal_in_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), AppError> {
    let settings = get_settings(&state.pool).await?;
    abo_core::reveal::reveal_in_file_manager(
        &path,
        settings.library_root.as_deref(),
        settings.set_aside_root.as_deref(),
    )
}

/// Open one of the two permanent destinations the sidebar links to (`AC-50`).
///
/// Takes a NAME, not a path. The Archive root is usually not set in settings,
/// because the plan builder derives it, so a frontend quick link that passed a
/// path would have to reconstruct that derivation and would drift from the
/// builder the first time it changed. Naming the root keeps that rule in one
/// place and means no path crosses the boundary for these two links at all.
#[tauri::command]
#[specta::specta]
pub async fn reveal_root(
    state: tauri::State<'_, AppState>,
    root: RevealRoot,
) -> Result<(), AppError> {
    let settings = get_settings(&state.pool).await?;
    abo_core::reveal::reveal_root(
        root,
        settings.library_root.as_deref(),
        settings.set_aside_root.as_deref(),
    )
}
