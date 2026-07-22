//! Audiobook Organizer Tauri v2 shell library (v0.1.0 spine).
//!
//! This is the thin shell hosting the Tauri-free `abo-core` engine crate.
//! Phase 1 wired only enough Tauri to boot the workspace scaffold; Phase 4
//! added structured logging (F-1003) via `tauri-plugin-log`; Phase 5 wires the
//! typed IPC contract (`#[tauri::command]` + `#[specta::specta]`,
//! `tauri_specta::Builder`, generated `src/lib/bindings.ts`). Phase 6 wires the
//! disposable tracer-slice UI on top of it. All product logic lives in
//! `abo-core`; this shell stays thin (reference architecture Section 4).
//! v0.4.0 Phase 5 adds the F-903 plan review commands (`commands::plan`):
//! `plan_generate`/`plan_get`/`plan_list_ops`/`plan_set_group_approval`/
//! `plan_exclude_op`. Phase 6 adds the F-906 ruleset editor commands
//! (`commands::ruleset`): `ruleset_list`/`ruleset_get`/`ruleset_save`/
//! `ruleset_delete`/`ruleset_count`/`ruleset_preset_examples`, plus the live
//! re-plan preview `commands::plan::plan_preview` and the scan Stop control
//! (`scan_cancel`, already wired since v0.2.0; this phase gives it a real
//! frontend affordance, AC-36).
//! v0.5.0 (acting) lands the full executor: `commands::apply::apply_start`
//! runs an approved plan as a spawned job (DryRun against `MemFs`, Real against
//! `RealFs`), with journal-before-act, single-writer locking, pause/resume and
//! Stop (`job_pause`/`job_resume`), post-apply verification with the
//! block-further-tidying gate, rollback preparation (`rollback_prepare`), and
//! the F-904 apply surface. The v1 UI pins mode to dry-run; a Real apply
//! against the actual library remains a human-only action (D-10).
//!   - commands -> IPC command handlers (payload contract lives in abo-core::ipc)
//!   - events   -> backend -> frontend typed event emission
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

// Re-exported for the panic-safety integration test (`tests/job_terminal.rs`),
// which drives the spawned scan job's terminal-state wrapper directly. Exposing
// just this helper (and its terminal-outcome type) keeps the rest of the command
// layer module-private.
pub use commands::{run_job_to_terminal, JobEnd};

// Re-exported for the apply panic-safety integration test (`tests/apply_terminal.rs`):
// the apply terminal-state wrapper, its pause/Stop control type, and the control
// registry, so the test can drive a spawned apply's terminal path (including a
// panic inside the walk releasing the single-writer in-process flag) directly.
pub use commands::apply::{run_apply_to_terminal, ApplyControl, ApplyControlRegistry};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use abo_core::job::CancelFlag;
use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use tauri_specta::{collect_commands, collect_events};

/// Shared, managed application state injected into every command.
///
/// Holds the long-lived SQLite pool the `abo-core` flows operate on, plus the
/// [`DbOpenOutcome`](abo_core::db::DbOpenOutcome) captured at startup so the
/// `db_status` command can report whether corrupt-DB recovery ran (P2). Built
/// once in [`run`]'s setup and handed to Tauri via `app.manage`.
pub struct AppState {
    /// The migrated SQLite pool (opened once at startup).
    pub pool: sqlx::SqlitePool,
    /// What happened while opening the database (Normal vs Recovered); surfaced
    /// by the `db_status` command as [`abo_core::ipc::DbStatus`].
    pub db_outcome: abo_core::db::DbOpenOutcome,
    /// Cancel flags for in-flight scan jobs, keyed by `jobs.id` (F-104).
    /// `scan_start` inserts a flag when it spawns a job and removes it when the
    /// job reaches a terminal state; `scan_cancel` flips the flag to request a
    /// cooperative stop. A plain `std::sync::Mutex` is enough: every access is a
    /// brief, non-async insert/remove/lookup, never held across an `.await`.
    pub jobs: Arc<Mutex<HashMap<i64, CancelFlag>>>,
    /// The sanctioned library scan root the backend re-allows at startup
    /// (FD-29, F-909): the persisted `settings.library_root`, loaded once in
    /// [`run`]'s setup and updated by `settings_set` when the user re-selects the
    /// library folder. `scan_start` reads it instead of accepting an arbitrary
    /// path from the frontend, which is the mechanism that keeps the WebView from
    /// ever directing the backend at an unsanctioned path. `None` until first-run
    /// picks a library. A plain `std::sync::Mutex`: every access is a brief,
    /// non-async snapshot/replace, never held across an `.await`.
    pub library_root: Arc<Mutex<Option<PathBuf>>>,
    /// The in-process single-writer apply guard (F-601, AC-8): set true while an
    /// apply runs, so a second `apply_start` in THIS process is refused instantly
    /// with `job-already-running` before any database work. An `AtomicBool` (not a
    /// lock guard) so it is safe to read across the apply's `.await` points; the
    /// durable `running` apply `jobs` row is the cross-restart backstop. Reset on
    /// every exit path (including a panic) by an RAII guard living in the SPAWNED
    /// apply task, so the flag frees when the walk ends, not when `apply_start`
    /// returns (which is now immediate).
    pub apply_in_flight: Arc<AtomicBool>,
    /// Live pause/resume/Stop controls for in-flight apply jobs (F-608, F-104),
    /// keyed by `jobs.id`. `apply_start` inserts one when it spawns the walk and it
    /// is removed when the job reaches a terminal state; `job_pause`/`job_resume`/
    /// `job_stop` look one up to control a running apply. A paused apply keeps its
    /// entry here AND its single-writer lock (a paused apply is still THE apply).
    pub apply_controls: commands::apply::ApplyControlRegistry,
    /// The interruption discovered at startup (F-606), if a prior session was
    /// killed mid-apply: the reconciled outcome of the single in-doubt operation
    /// and whether resume or rollback is offered. `None` on a clean start. Captured
    /// once in [`run`]'s setup (before `manage`), read by the `startup_interruption`
    /// command the shell polls on mount.
    pub startup_interruption: Option<abo_core::exec::ReconcileResult>,
}

/// Build the `tauri-specta` [`Builder`](tauri_specta::Builder) for the shell.
///
/// Single source of truth for the command + event surface so both [`run`] and
/// the headless `export_bindings` path register exactly the same set, keeping
/// the generated TypeScript bindings in lockstep with the runtime handlers.
///
/// `.dangerously_cast_bigints_to_number()` is REQUIRED: the IPC payloads carry
/// `i64` ids, counts, and byte totals (`scan_id`, `entry_count`, `total_bytes`,
/// `job_id`, ...), and specta-typescript's default `BigIntExportBehavior` is
/// `Fail`, so without this the export errors. Every such value fits inside JS's
/// 2^53 safe-integer range in practice, so casting to TS `number` is lossless
/// here. It must be set on this shared factory so the runtime invoke surface and
/// the exported bindings agree. (No `serde_json::Value` remap is needed: unlike
/// the reference project, no payload here carries a free-form JSON field.)
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::scan_start,
            commands::scan_cancel,
            commands::scan_entries,
            commands::cover_get,
            commands::classify_overview,
            commands::db_status,
            commands::startup_interruption,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::plan::plan_generate,
            commands::plan::plan_get,
            commands::plan::plan_list_ops,
            commands::plan::plan_set_group_approval,
            commands::plan::plan_exclude_op,
            commands::plan::plan_preview,
            commands::ruleset::ruleset_list,
            commands::ruleset::ruleset_get,
            commands::ruleset::ruleset_get_active,
            commands::ruleset::ruleset_save,
            commands::ruleset::ruleset_delete,
            commands::ruleset::ruleset_count,
            commands::ruleset::ruleset_preset_examples,
            commands::apply::apply_start,
            commands::rollback::rollback_prepare,
            commands::rollback::rollback_prepare_partial,
            commands::job::job_status,
            commands::job::acknowledge_check,
            commands::job::job_pause,
            commands::job::job_resume,
            commands::job::job_stop,
        ])
        .events(collect_events![
            events::JobCompleted,
            events::JobFailed,
            // P8 IMPORTANT 3: apply's DISTINCT cooperative-Stop terminal event, so
            // the activity surface transitions to "stopped between books" reliably.
            events::JobStopped,
            // Frozen but never emitted in the spine (see events::JobProgress).
            events::JobProgress,
            // P8 prelude 0b: per-operation progress event for the apply surface.
            events::JobOpExecuted,
        ])
        .dangerously_cast_bigints_to_number()
}

/// Application entry point invoked by `main.rs` (and the mobile entry point).
///
/// Builds the `tauri-specta` command/event surface, wires the invoke handler,
/// mounts the events, opens the database into managed [`AppState`], then runs the
/// Tauri runtime.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Dev convenience: regenerate the bindings on every debug run so a local
    // contract change is visible immediately. Goes through the shared
    // `export_bindings` helper (same builder + header) so the dev-written file is
    // byte-identical to the one the `export_bindings` test commits; that test is
    // the canonical producer the bindings-drift gate relies on. Gated to debug so
    // release builds do NO file I/O (spec: tauri-specta seam paragraph).
    #[cfg(debug_assertions)]
    export_bindings("../src/lib/bindings.ts").expect("failed to export typescript bindings");

    tauri::Builder::default()
        // Single-instance (F-601, AC-8): registered FIRST per the plugin's contract.
        // A SECOND launch runs this callback in the FIRST (existing) instance and
        // then exits, so only ONE process ever holds the apply single-writer lock -
        // this is what makes that lock process-wide and makes the startup reclaim of
        // a stranded apply row sound (a fresh launch implies no other live instance).
        // Focus and un-minimize the existing window so a relaunch brings the app
        // forward. No event is emitted, so no new JS/IPC surface is added.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(build_log_plugin())
        // Folder selection ONLY (F-909, FD-29): the OS folder picker so the user
        // can choose the library root. This is the one capability the security
        // model adds this release (capabilities/default.json grants exactly
        // `dialog:allow-open`); there is deliberately no fs and no shell plugin.
        // The picked path is handed to the backend via `settings_set`; the
        // WebView is never granted a standing filesystem scope.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // Register the event registry so typed emit/listen resolve names.
            builder.mount_events(app);

            // Open the database synchronously during setup, resolving the
            // production app-data path (%LOCALAPPDATA%\AudiobookOrganizer on
            // Windows; kept out of any OneDrive-synced tree by construction, the
            // structural defense against WAL-sidecar corruption). A corrupt or
            // unopenable existing database is recovered (moved aside, fresh db
            // created) rather than crashing (P2); the outcome is carried into
            // AppState so `db_status` can surface the one-time notice.
            let app_data_dir = abo_core::paths::app_data_dir();
            let (pool, db_outcome) = tauri::async_runtime::block_on(async {
                abo_core::db::open_db(&app_data_dir).await
            })
            .expect("failed to open the application database");

            if let abo_core::db::DbOpenOutcome::Recovered { backup_path } = &db_outcome {
                log::warn!(
                    "the database was unreadable and has been reset; the previous \
                     database was preserved at {}",
                    backup_path.display()
                );
            }

            // Startup re-allowance (FD-29, F-909, AC-31): read the persisted
            // library root and hold it as the sanctioned scan root, so operations
            // resume against the same library on relaunch without re-picking. In
            // this architecture the backend owns all filesystem access via
            // std::fs (there is no tauri-plugin-fs and thus no WebView fs scope to
            // re-add), so "re-allowance" concretely means loading the sanctioned
            // root here; scan_start then uses it rather than any frontend path.
            // A settings read failure at startup is non-fatal: fall back to "no
            // library configured" (the shell shows first-run) rather than refusing
            // to launch. Whether a persisted root still exists on disk is checked
            // at scan time (root-not-found -> F-908 re-pick surface, Phase 7).
            let library_root = tauri::async_runtime::block_on(async {
                abo_core::db::settings::get_settings(&pool).await
            })
            .map(|settings| settings.library_root.map(PathBuf::from))
            .unwrap_or_else(|e| {
                log::warn!("could not read settings at startup: {e}; starting with no library");
                None
            });
            if let Some(root) = &library_root {
                log::info!("re-allowed persisted library root: {}", root.display());
            }

            // Best-effort cover-cache sweep (F-907): reclaim cover thumbnails
            // whose books were deleted or renamed on disk (the content-addressed
            // key already keeps a rescan from orphaning files, but a vanished
            // book still leaves its old file behind). Never touches the library;
            // any failure is swallowed inside the sweep and only defers reclaim
            // to the next launch.
            let cover_cache_dir = app_data_dir.join("covers");
            let swept = abo_core::scan::sweep_cover_cache(&cover_cache_dir);
            if swept > 0 {
                log::info!("swept {swept} stale cover cache file(s) at startup");
            }

            // Interruption safety (F-606): before reclaiming a stranded apply lock,
            // reconcile its journal - verify the real on-disk outcome of the single
            // in-doubt operation and write the terminal row the kill prevented - and
            // capture the interruption so the shell can offer resume-or-rollback
            // (the `startup_interruption` command reads it on mount). Reads the real
            // filesystem (RealFs) but never mutates it. Best-effort: a failure only
            // means no resume offer this launch; the reclaim below still runs.
            let startup_interruption = tauri::async_runtime::block_on(async {
                abo_core::exec::reconcile_stranded_apply_jobs(
                    &pool,
                    &abo_core::exec::RealFs::new(),
                    &abo_core::scan::walk::now_iso8601_utc(),
                )
                .await
            })
            .unwrap_or_else(|e| {
                log::warn!("could not reconcile a stranded apply at startup: {e}");
                None
            });
            if let Some(interruption) = &startup_interruption {
                log::warn!(
                    "a prior tidy-up (job {}) was interrupted; offering resume-or-rollback",
                    interruption.job_id
                );
            }

            // Crash-detected release of the single-writer apply lock (F-601, AC-8):
            // a `running` apply `jobs` row a previous session left behind (killed
            // mid-apply) is reclaimed here, so it never blocks a fresh apply. Only
            // touches apply jobs; a stranded scan is left as-is. Best-effort: a DB
            // error only defers the reclaim to the next launch.
            let reclaimed = tauri::async_runtime::block_on(async {
                abo_core::exec::lock::reclaim_stranded_apply_jobs(
                    &pool,
                    &abo_core::scan::walk::now_iso8601_utc(),
                )
                .await
            });
            match reclaimed {
                Ok(n) if n > 0 => {
                    log::warn!("reclaimed {n} stranded apply lock(s) left by a prior session");
                }
                Ok(_) => {}
                Err(e) => log::warn!("could not reclaim stranded apply locks at startup: {e}"),
            }

            app.manage(AppState {
                pool,
                db_outcome,
                jobs: Arc::new(Mutex::new(HashMap::new())),
                library_root: Arc::new(Mutex::new(library_root)),
                apply_in_flight: Arc::new(AtomicBool::new(false)),
                apply_controls: Arc::new(Mutex::new(HashMap::new())),
                startup_interruption,
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Audiobook Organizer");
}

/// Export the TypeScript IPC bindings to `path`, headlessly (no GUI launch).
///
/// Canonical producer of the committed `src/lib/bindings.ts`: it builds the exact
/// same `tauri-specta` surface the runtime uses (via [`specta_builder`]) and
/// writes the TypeScript. Exposed so the headless `export_bindings` integration
/// test (in `tests/`) can call it, which is what `pnpm bindings:check` and the CI
/// bindings-drift gate run.
///
/// OQ-1 / G-5 runner-placement decision (FD-24), resolved empirically here:
/// the bindings-drift gate runs on the WINDOWS runner. The evidence:
///
/// First, the export path LINKS Tauri. `specta_builder()` is generic over the
/// concrete `tauri::Wry` runtime and `collect_commands!` references
/// `#[tauri::command]` functions that take `tauri::State` / `AppHandle`, so the
/// export test binary statically links the whole `tauri` crate graph. There is no
/// "pure-data" export binary to move to Ubuntu; linking Tauri is inherent to
/// collecting the command surface.
///
/// Second, on Windows the Tauri-linked TEST binary needs the comctl32-v6
/// activation manifest embedded by `build.rs` (`rustc-link-arg-tests`), or it
/// fails at process startup with STATUS_ENTRYPOINT_NOT_FOUND before any test
/// runs. This is demonstrated in this workspace (removing the manifest embed makes
/// `cargo test -p abo --test export_bindings` fail to start; restoring it passes),
/// confirming the reference project's comctl32 issue applies here.
///
/// Third, the `export` call itself is pure specta reflection + TS emission: it
/// does NOT initialize a GTK/webview runtime, so on Linux it would not need a
/// DISPLAY. But the binary still links libwebkit2gtk-4.1 on Linux and needs those
/// system libraries present to load. Whether a headless Ubuntu runner loads and
/// runs it cleanly cannot be verified from this Windows dev box.
///
/// Per FD-24, when placement is undecidable without an actual Ubuntu run, default
/// to Windows (the safe choice); it also matches the last-known-good reference
/// pattern (repo-sync runs its bindings gate on Windows). P7 wires real CI and MAY
/// move this to Ubuntu if a headless run there proves the codegen loads without a
/// display (which would save Windows-runner minutes).
pub fn export_bindings(path: &str) -> Result<(), specta_typescript::Error> {
    // The generated file uses `any` in its runtime shim and imports `invoke`,
    // both of which the project's eslint config would flag. It is
    // machine-generated and never hand-edited, so a leading `/* eslint-disable */`
    // exempts the whole file. It type-checks cleanly under tsc, so no
    // `@ts-nocheck` is needed.
    let ts = specta_typescript::Typescript::default().header("/* eslint-disable */\n");
    specta_builder().export(ts, path)
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
