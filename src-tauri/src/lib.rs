//! Audiobook Organizer Tauri v2 shell library (v0.1.0 spine).
//!
//! This is the thin shell hosting the Tauri-free `abo-core` engine crate.
//! Phase 1 wired only enough Tauri to boot the workspace scaffold; Phase 4
//! adds structured logging (F-1003) via `tauri-plugin-log`, kept strictly
//! additive. Phase 5 wires the typed IPC contract (`#[tauri::command]` +
//! `#[specta::specta]`, `tauri_specta::Builder`, generated
//! `src/lib/bindings.ts`); Phase 6 wires the disposable tracer-slice UI on top
//! of it. All product logic lives in `abo-core`; this shell stays thin
//! (reference architecture Section 4).
//!   - commands -> IPC command handlers (Phase 5 owns the payload contract)
//!   - events   -> backend -> frontend event emission (Phase 5)
//!
//! Logging bridge (F-1003). `tauri-plugin-log` installs a global `log::Log`
//! logger (fern) via `log::set_boxed_logger`; it captures the `log` facade. It
//! is NOT a `tracing` Subscriber, and this shell installs no `tracing`
//! Subscriber. abo-core emits its scan-lifecycle events with `tracing`, and
//! abo-core enables tracing's `log` cargo feature, so - with no Subscriber
//! present - those `tracing` events are ALSO emitted as `log` records and are
//! captured by this plugin. That is the whole bridge: `tracing` (core) ->
//! `log` facade -> `tauri-plugin-log` -> file + (debug) stdout. It is proven by
//! `crates/abo-core/tests/tracing_log_bridge.rs`. The plugin's own `tracing`
//! cargo feature is deliberately left OFF: it does the reverse (emits the
//! plugin's events INTO tracing), which is not what the core-to-file flow needs.

mod commands;
mod events;

use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

/// Application entry point invoked by `main.rs` (and the mobile entry
/// point).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(build_log_plugin())
        .run(tauri::generate_context!())
        .expect("error while running Audiobook Organizer");
}

/// Build the `tauri-plugin-log` instance (F-1003): a rotating log file in the
/// OS app log directory, plus stdout in debug builds only.
///
/// - File target: `TargetKind::LogDir` resolves to Tauri's per-app log dir at
///   plugin init; `file_name = "abo"` yields `abo.log`. `KeepSome(5)` keeps the
///   five most recent rotated files and `max_file_size` rotates at ~5 MB, so the
///   log footprint is bounded (no unbounded growth, no network, no telemetry -
///   NFR Privacy).
/// - Stdout is added only under `debug_assertions` (dev / `pnpm tauri dev`); a
///   release build logs to the file alone and does not spawn console I/O.
/// - `level(Info)` passes through abo-core's INFO scan-lifecycle events while
///   filtering the noisier DEBUG/TRACE spans.
fn build_log_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    // ~5 MB per file before rotation; keep the 5 most recent files.
    const MAX_LOG_FILE_BYTES: u128 = 5 * 1024 * 1024;
    const KEEP_ROTATED_FILES: usize = 5;

    let mut builder = tauri_plugin_log::Builder::new()
        .level(log::LevelFilter::Info)
        .rotation_strategy(RotationStrategy::KeepSome(KEEP_ROTATED_FILES))
        .max_file_size(MAX_LOG_FILE_BYTES)
        .target(Target::new(TargetKind::LogDir {
            file_name: Some("abo".to_string()),
        }));

    // Console output is a dev convenience only; release builds log to file.
    #[cfg(debug_assertions)]
    {
        builder = builder.target(Target::new(TargetKind::Stdout));
    }

    builder.build()
}
