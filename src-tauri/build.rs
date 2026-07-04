// src-tauri build script (v0.1.0 spine).
//
// Phase 1: runs the Tauri build step that generates the context, embeds the
// config, and produces the permission/capability schemas consumed at runtime.
// Phase 5 (tauri-specta seam) adds the test-binary manifest embed below.

fn main() {
    // Embed a comctl32-v6 activation manifest into the crate's TEST binaries on
    // Windows + MSVC. The Tauri runtime imports comctl32 v6 symbols (e.g.
    // TaskDialogIndirect) that live only in the side-by-side v6 assembly. The
    // shipped abo.exe receives a v6 manifest from tauri-build, but `cargo test`
    // executables do not, so the headless `export_bindings` test (a Tauri-linked
    // binary) would otherwise fail to start with STATUS_ENTRYPOINT_NOT_FOUND
    // before any test runs. This only affects test binaries
    // (`rustc-link-arg-tests`), never the real app.
    //
    // This is the load-bearing reason the bindings export lives in `tests/`
    // rather than as a `--lib` unit test (the lib test harness is not a `[[test]]`
    // target, so this arg would not reach it) AND the reason the bindings-drift
    // gate runs on Windows (OQ-1 / FD-24; see lib.rs `export_bindings`).
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tracer-test.manifest");
        println!("cargo::rerun-if-changed=tracer-test.manifest");
        // Scope the manifest to integration `[[test]]` binaries only via
        // `rustc-link-arg-tests`. The real abo.exe must NOT get this arg:
        // tauri-build already embeds a MANIFEST resource into it, and a second
        // /MANIFESTINPUT triggers `CVT1100: duplicate resource type:MANIFEST`.
        println!("cargo::rustc-link-arg-tests=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg-tests=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }

    tauri_build::build()
}
