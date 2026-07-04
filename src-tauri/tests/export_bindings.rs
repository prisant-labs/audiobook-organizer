//! Headless generator for the committed TypeScript IPC bindings.
//!
//! v0.1.0 spine, Phase 5. This is the canonical producer of
//! `src/lib/bindings.ts`: it builds the same `tauri-specta` command/event surface
//! the runtime uses and exports the TypeScript WITHOUT launching the GUI. Run
//! with:
//!   cargo test -p abo --test export_bindings
//!
//! It is an integration test (in `tests/`) rather than a `--lib` unit test on
//! purpose: the src-tauri build script attaches a comctl32-v6 activation manifest
//! to `[[test]]` binaries only, and without that manifest a Tauri-linked test
//! executable fails to start on Windows (STATUS_ENTRYPOINT_NOT_FOUND) before any
//! test runs. See `build.rs` and the OQ-1 note beside `abo_lib::export_bindings`.
//!
//! `pnpm bindings:check` runs this, then `git diff --exit-code` on the generated
//! file; CI's bindings-drift gate (windows-latest) does the same.

/// Generate `src/lib/bindings.ts` from the live `tauri-specta` surface.
///
/// The path is relative to the `src-tauri` crate root, matching where `cargo`
/// runs the test binary's working directory.
#[test]
fn export_bindings() {
    abo_lib::export_bindings("../src/lib/bindings.ts")
        .expect("failed to export typescript bindings");
}
