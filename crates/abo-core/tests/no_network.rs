//! Zero-network gate (FD-11, AC-14): assert no HTTP client crate is resolved
//! into the DESKTOP dependency graph of this workspace.
//!
//! Why this asserts over `cargo tree` and NOT over Cargo.lock (G-evidence note
//! for the release gate): Cargo.lock is target-agnostic - it records the union
//! of dependencies across ALL targets so any-target builds are reproducible.
//! tauri v2 (2.11.3) declares `reqwest` under
//! `[target.'cfg(any(target_os = "android", all(target_vendor = "apple",
//! not(target_os = "macos"))))'.dependencies]`, i.e. for Android and iOS ONLY,
//! so `reqwest` (and its `hyper`/`hyper-util`) always appear in the lockfile
//! even though they are never compiled into the Windows/Linux/macOS desktop
//! app. A lockfile text scan would therefore fail forever on packages that are
//! provably absent from every desktop build. The authoritative statement of
//! what the desktop app links is the RESOLVED dependency graph for a desktop
//! target triple, which is exactly what
//! `cargo tree --workspace -e normal --target <desktop-triple>` prints; this
//! test asserts over that. Controller-approved deviation from the original
//! "assert over Cargo.lock" plan (task P4, item 4, option A).
//!
//! Coverage across desktop triples: CI runs the test job on both ubuntu and
//! windows, so the windows-msvc and linux-gnu triples are each asserted on
//! their own matrix leg (and apple-darwin on any macOS run). `-e normal`
//! matches the runtime posture: dev- and build-dependencies never ship in the
//! app binary.
//!
//! This test is deliberately ACTIVE everywhere (no `#[ignore]`), and it fails
//! loudly - never skips - if `cargo tree` cannot be spawned, exits nonzero, or
//! returns output that does not look like this workspace's tree.

use std::path::PathBuf;
use std::process::Command;

/// HTTP client crates that must never resolve into the desktop graph
/// (FD-11 zero-network posture; NFR Privacy). Names are compared
/// case-insensitively against the resolved package name of every line.
const DENYLIST: [&str; 10] = [
    "reqwest",
    "hyper",
    "hyper-util",
    "ureq",
    "curl",
    "curl-sys",
    "isahc",
    "attohttpc",
    "surf",
    "awc",
];

/// The desktop target triple this test binary was compiled for, derived from
/// compile-time cfg. Passing it to `cargo tree --target` pins the resolution
/// to THIS desktop platform (rather than cargo's own host default) and keeps
/// the assertion honest if the test is ever cross-compiled.
fn host_triple() -> String {
    let arch = match std::env::consts::ARCH {
        // std arch names equal the triple arch component for our platforms.
        arch @ ("x86_64" | "aarch64") => arch,
        other => panic!(
            "no_network: unexpected desktop architecture {other:?}; \
             add its target-triple mapping here"
        ),
    };
    let rest = if cfg!(all(target_os = "windows", target_env = "msvc")) {
        "pc-windows-msvc"
    } else if cfg!(all(target_os = "linux", target_env = "gnu")) {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else {
        panic!(
            "no_network: unsupported desktop platform (not windows-msvc, \
             linux-gnu, or macos); add its target triple here"
        )
    };
    format!("{arch}-{rest}")
}

/// The workspace root (two levels above this crate's manifest dir), so the
/// spawned `cargo tree` resolves the whole workspace regardless of the test
/// harness's working directory.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // crates/
    dir.pop(); // workspace root
    dir
}

#[test]
fn no_http_client_resolves_into_the_desktop_dependency_tree() {
    let triple = host_triple();

    // Prefer the exact cargo that is running this test; fall back to PATH.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    // `--prefix none` prints one plain "name vX.Y.Z ..." line per resolved
    // package occurrence (no tree-drawing glyphs), so the package name is
    // always the first whitespace-separated token. `--locked` forbids a silent
    // lockfile rewrite: if Cargo.toml and Cargo.lock disagree, the test fails
    // loudly instead of asserting over a tree nobody committed.
    let output = Command::new(&cargo)
        .args([
            "tree",
            "--workspace",
            "-e",
            "normal",
            "--target",
            &triple,
            "--prefix",
            "none",
            "--locked",
        ])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "no_network: could not spawn `{cargo} tree` (the zero-network \
                 gate REQUIRES cargo tree; do not skip this test): {e}"
            )
        });

    assert!(
        output.status.success(),
        "no_network: `cargo tree --workspace -e normal --target {triple}` \
         exited with {}; stderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Guard against a vacuous pass on truncated/empty output: the tree must
    // contain both workspace members' subtrees (abo-core, and tauri under the
    // shell) before "no denylist hit" means anything.
    assert!(
        stdout.contains("abo-core") && stdout.contains("tauri"),
        "no_network: `cargo tree` output does not look like this workspace's \
         resolved tree (missing abo-core and/or tauri); refusing to pass \
         vacuously. Output was:\n{stdout}"
    );

    let hits: Vec<&str> = stdout
        .lines()
        .filter(|line| {
            line.split_whitespace().next().is_some_and(|name| {
                let name = name.to_ascii_lowercase();
                DENYLIST.contains(&name.as_str())
            })
        })
        .collect();

    assert!(
        hits.is_empty(),
        "no_network: HTTP client crate(s) resolved into the desktop ({triple}) \
         dependency tree, violating the FD-11 zero-network posture (AC-14). \
         Offending resolved package lines:\n{}\nRun `cargo tree --workspace \
         -e normal --target {triple} -i <package>` for the dependency chain.",
        hits.join("\n")
    );
}
