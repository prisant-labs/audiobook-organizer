//! Audiobook Organizer Tauri v2 shell library (v0.1.0 spine, Phase 1).
//!
//! This is the thin shell hosting the Tauri-free `abo-core` engine crate.
//! Phase 1 wires only enough Tauri to boot the workspace scaffold: no
//! commands, no events, and no tauri-specta seam yet. Phase 5 wires the
//! typed IPC contract (`#[tauri::command]` + `#[specta::specta]`,
//! `tauri_specta::Builder`, generated `src/lib/bindings.ts`); Phase 6 wires
//! the disposable tracer-slice UI on top of it. All product logic lives in
//! `abo-core`; this shell stays thin (reference architecture Section 4).
//!   - commands -> IPC command handlers (Phase 5 owns the payload contract)
//!   - events   -> backend -> frontend event emission (Phase 5)

mod commands;
mod events;

/// Application entry point invoked by `main.rs` (and the mobile entry
/// point).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Audiobook Organizer");
}
