//! Smoke test for the `tracing` -> `log` facade bridge (F-1003, Phase 4).
//!
//! abo-core emits its scan-lifecycle events with `tracing`. The shell captures
//! the `log` facade: `tauri-plugin-log` installs a global `log::Log` logger via
//! `log::set_boxed_logger` and does NOT install a `tracing` Subscriber (nor does
//! the shell install one anywhere). abo-core enables tracing's `log` cargo
//! feature, which makes `tracing` events ALSO emit `log` records precisely when
//! no `tracing` Subscriber is present - which is exactly the shell's runtime
//! configuration.
//!
//! This test reproduces that setup with a capturing `log::Log` logger (standing
//! in for the plugin's fern logger, which is likewise just a `log::Log` impl)
//! and asserts that a `tracing::info!` event arrives as a `log::Record`. That is
//! the load-bearing link in the chain `tracing` (core) -> `log` facade ->
//! `tauri-plugin-log` -> file. Proving it here means a broken bridge fails
//! `cargo test`, not a `pnpm tauri dev` session.
//!
//! It lives in its own integration-test binary (a separate process) so the
//! once-per-process global `log::set_logger` call cannot collide with any other
//! test's logger.

use std::sync::{Arc, Mutex};

use log::{Level, Log, Metadata, Record};

/// A minimal `log::Log` that records every log record it receives, so the test
/// can assert what crossed the facade. This is the same shape of sink that
/// tauri-plugin-log's fern logger is at runtime: a global `log::Log`.
struct CapturingLogger {
    records: Arc<Mutex<Vec<(Level, String)>>>,
}

impl Log for CapturingLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        self.records
            .lock()
            .expect("capture lock")
            .push((record.level(), record.args().to_string()));
    }

    fn flush(&self) {}
}

#[test]
fn tracing_event_reaches_the_log_facade() {
    let captured: Arc<Mutex<Vec<(Level, String)>>> = Arc::new(Mutex::new(Vec::new()));

    // Install the capturing logger as THE global `log` logger, mirroring how
    // tauri-plugin-log installs fern. No `tracing` Subscriber is installed, so
    // tracing's `log` feature is what makes the event below cross the facade.
    log::set_boxed_logger(Box::new(CapturingLogger {
        records: captured.clone(),
    }))
    .expect("no other global logger should be set in this test binary");
    log::set_max_level(log::LevelFilter::Trace);

    // The same event shape abo-core emits at the end of a scan (see
    // crate::scan::persist::run_scan).
    tracing::info!(scan_id = 7_i64, entry_count = 3_i64, "scan: completed");

    let records = captured.lock().expect("capture lock");
    assert!(
        records
            .iter()
            .any(|(level, message)| *level == Level::Info && message.contains("scan: completed")),
        "a tracing::info! event must surface as an INFO log::Record on the facade \
         (this is the tracing -> log bridge tauri-plugin-log depends on); captured: {records:?}"
    );
}
