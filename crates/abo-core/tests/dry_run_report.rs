//! F-506 (dry-run HTML report) full-chain integration coverage, plus the
//! operator-run real-library gate (G-05/G-06 evidence generator).
//!
//! The first test drives the real orchestration
//! ([`abo_core::plan::report::generate_and_report`]) end to end over a
//! materialized synthetic library: generate a fixture tree on disk -> live
//! read-only scan into a temp DB -> seed the default ruleset -> build, detect
//! duplicates, validate, persist, export, and render the HTML report into a temp
//! Reports folder. It asserts the report is self-contained (AC-20), carries the
//! FD-10 canon (AC-23), has exactly one complete-list row per plan op (AC-21),
//! and leaks none of the banned internal vocabulary.
//!
//! The second test is `#[ignore]`: it reads `E:\Books - Audio`, which does not
//! exist on the CI runners, so it never runs in the CI `test` job. It is the
//! operator-run signature-gate evidence generator; it writes the real report
//! into the real Reports folder AND copies it to the session scratchpad, and
//! prints the plan's headline numbers.

use std::path::Path;

use abo_core::db::open_db;
use abo_core::db::plans::get_plan_ops;
use abo_core::fixtures::{generate, standard_library_manifest};
use abo_core::plan::report::generate_and_report;
use abo_core::ruleset::seed_default_ruleset;
use abo_core::scan::run_scan;
use abo_core::scan::walk::now_iso8601_utc;
use tempfile::TempDir;

/// The real library root (operator-run gate; strictly read-only).
const LIBRARY_ROOT: &str = r"E:\Books - Audio";
/// Where the real (personal-path) report is copied for this session's evidence.
const SCRATCH_REAL_REPORT: &str = r"C:\Users\jpris\AppData\Local\Temp\claude\E--Projects-prisant-labs-audiobook-organizer\03dc034e-2e4f-4956-8757-b72e22d5c485\scratchpad\real-dryrun-report.html";

/// AC-20/AC-21/AC-23: the full chain produces a self-contained report with one
/// complete-list row per plan op and the FD-10 canon, over a fixture library.
#[tokio::test]
async fn full_chain_generates_a_self_contained_report_over_a_fixture() {
    // 1. Materialize the standard synthetic library on disk.
    let lib = TempDir::new().expect("library tempdir");
    let generated =
        generate(&standard_library_manifest(), lib.path()).expect("materialize fixture");
    assert!(
        !generated.entries.is_empty(),
        "the fixture materialized entries"
    );

    // 2. Live, read-only scan into a temp database.
    let db_dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db_dir.path()).await.expect("open_db");
    let scan = run_scan(&pool, lib.path()).await.expect("scan");
    let ruleset_id = seed_default_ruleset(&pool, "Default", &now_iso8601_utc())
        .await
        .expect("seed ruleset");

    // 3. Full-chain orchestration into a temp Reports folder.
    let app_data = TempDir::new().expect("app data tempdir");
    let report = generate_and_report(&pool, scan.scan_id, ruleset_id, app_data.path())
        .await
        .expect("generate_and_report");

    // Every artifact landed (the five exports + the HTML report).
    for p in [
        &report.written.csv,
        &report.written.json,
        &report.written.markdown,
        &report.written.provenance_json,
        &report.written.provenance_markdown,
        &report.html_path,
    ] {
        assert!(p.exists(), "artifact not written: {p:?}");
    }

    let html = std::fs::read_to_string(&report.html_path).expect("read report");

    // AC-20: self-contained, zero network.
    assert!(!html.contains("://"), "no scheme://host in the report");
    assert!(!html.contains("<link"), "no external stylesheet link");
    assert!(!html.contains("<script"), "no script tag");
    assert!(!html.contains("@import"), "no CSS @import");
    assert!(
        html.contains("data:font/woff2;base64,"),
        "font is embedded as a data URI"
    );

    // AC-23: FD-10 canon verbatim.
    assert!(html.contains(
        "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone."
    ));

    // All the required sections are present.
    assert!(html.contains("The seven groups at a glance"));
    assert!(html.contains("What will not happen"));
    assert!(html.contains("Where these books came from"));
    assert!(html.contains("Complete change list"));
    assert!(html.contains("@media print"), "print rules present");

    // AC-21: one complete-list row per persisted plan op.
    let ops = get_plan_ops(&pool, report.plan_id).await.expect("plan ops");
    let op_rows = html.matches("class=\"op-row\"").count();
    assert_eq!(op_rows, ops.len(), "one complete-list row per plan op");
    assert!(op_rows > 0, "the fixture produced plan ops");

    // Banned internal vocabulary never reaches the report.
    let lower = html.to_lowercase();
    for word in [
        "quarantine",
        "dedupe",
        "operations",
        "manifest",
        "dashboard",
    ] {
        assert!(!lower.contains(word), "banned word {word:?} leaked");
    }
    // AC-26: no ABS-side write is ever promised.
    assert!(!lower.contains("written to audiobookshelf"));
    assert!(!lower.contains("tag written"));
    assert!(!lower.contains("write back to"));

    // The persisted plan's stats agree with the report's total-change headline
    // being a coherent sum of the seven groups.
    let sum: u64 = report.numbers.per_group.iter().map(|(_, c)| *c).sum();
    assert_eq!(sum, report.numbers.total_changes);
}

/// G-05/G-06 evidence generator: the REAL 297 GB library, strictly read-only.
/// Writes the report into the real Reports folder and copies it to the session
/// scratchpad (it carries personal paths, so it is never committed), and prints
/// the plan's headline numbers.
#[tokio::test]
#[ignore = "reads E:\\Books - Audio (absent in CI). Run locally with --ignored --nocapture."]
async fn real_library_dry_run_report() {
    let root = Path::new(LIBRARY_ROOT);
    if !root.is_dir() {
        println!("SKIP: {LIBRARY_ROOT} not present; the real-library gate is operator-run only.");
        return;
    }

    let db_dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db_dir.path()).await.expect("open_db");

    println!("Scanning {LIBRARY_ROOT} (read-only)...");
    let scan = run_scan(&pool, root).await.expect("live scan");
    println!(
        "  scan {} recorded {} entries ({:.1} GB)",
        scan.scan_id,
        scan.entry_count,
        scan.total_bytes as f64 / 1e9
    );

    let ruleset_id = seed_default_ruleset(&pool, "Default", &now_iso8601_utc())
        .await
        .expect("seed ruleset");

    // Write into the REAL Reports folder (personal paths, never committed).
    let app_data = abo_core::paths::app_data_dir();
    let report = generate_and_report(&pool, scan.scan_id, ruleset_id, &app_data)
        .await
        .expect("generate_and_report");

    // Copy the report to the session scratchpad for render evidence.
    match std::fs::copy(&report.html_path, SCRATCH_REAL_REPORT) {
        Ok(_) => println!("Copied report to {SCRATCH_REAL_REPORT}"),
        Err(e) => println!("WARN: could not copy report to scratchpad: {e}"),
    }

    let n = &report.numbers;
    println!("\n=== REAL PLAN HEADLINE NUMBERS ===");
    println!("plan {} over scan {}", report.plan_id, scan.scan_id);
    println!("total changes: {}", n.total_changes);
    println!("per group:");
    for (g, c) in &n.per_group {
        println!("  {g:<14} {c}");
    }
    println!("duplicate groups (held, FD-08): {}", n.duplicate_groups);
    println!("version candidates: {}", n.version_candidates);
    println!("manual-review books: {}", n.manual_review);
    println!("warnings (need a decision): {}", n.warnings);
    println!("report folder: {}", report.written.dir.display());
    println!("html report: {}", report.html_path.display());

    // Read the written report back and prove it is self-contained.
    let html = std::fs::read_to_string(&report.html_path).expect("read real report");
    assert!(
        !html.contains("://"),
        "the real report must be self-contained"
    );
    assert!(html.contains("data:font/woff2;base64,"));
    assert!(scan.entry_count > 0, "the live scan recorded entries");
}
