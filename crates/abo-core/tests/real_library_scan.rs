//! v0.2.0 Phase 7, release gate G-03 + G-07: the first fresh, READ-ONLY look at
//! the real 297 GB library since the 2026-03-25 WizTree baseline (FD-18), run
//! through the exact hardened engine P1-P6 shipped.
//!
//! This test is `#[ignore]` on purpose: it reads `E:\Books - Audio` and imports
//! `_local/prior-work/WizTree_2026-03-25.csv`, NEITHER of which exists on the CI
//! runners (ubuntu + windows GitHub Actions), so it must never run in the CI
//! `test` job. It is the operator-run gate-evidence generator; run it locally
//! with:
//!
//! ```text
//! cargo test -p abo-core --test real_library_scan -- --ignored --nocapture
//! ```
//!
//! It is strictly READ-ONLY against the library: it runs `run_scan` (a walk +
//! stat, no writes) and `run_csv_import` (reads the CSV), persisting only into a
//! throwaway temp-dir SQLite database. No file under `E:\Books - Audio` is ever
//! created, modified, renamed, or deleted (D-09; this phase's absolute
//! constraint). Its printed output feeds the committed baseline-delta report
//! (docs/internal/releases/v0.2.0-understanding/baseline-delta-2026-07-04.md).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use abo_core::classify::{
    classify, health_metrics, inputs_from_snapshot, ClassifyInput, FolderClass,
};
use abo_core::db::open_db;
use abo_core::ipc::{EntryRow, ScanSummary};
use abo_core::parse::coverage;
use abo_core::parse::extract::NodeKind;
use abo_core::scan::{get_scan_entries, run_csv_import, run_scan};
use regex::Regex;
use tempfile::TempDir;

/// The real library root (this phase's target; read-only).
const LIBRARY_ROOT: &str = r"E:\Books - Audio";
/// The rescued 2026-03-25 WizTree export (FD-18 baseline source), relative to
/// the workspace root.
const WIZTREE_CSV_REL: &str = r"_local\prior-work\WizTree_2026-03-25.csv";

fn workspace_root() -> std::path::PathBuf {
    let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop(); // crates/
    dir.pop(); // workspace root
    dir
}

/// The FD-18 2026-03-25 baselines (every figure labeled "2026-03-25 baseline,
/// pending fresh scan" wherever it surfaces; FD-18).
struct Fd18Baseline;
impl Fd18Baseline {
    const BOOK_LIKE_FOLDERS: u64 = 582;
    const MIXED_FOLDERS: u64 = 11;
    const ESTIMATED_ABS_ITEMS: u64 = 831;
    const BRACKET_TAGS: u64 = 203;
    const BITRATE_MARKERS: u64 = 170;
    const SIZE_MARKERS: u64 = 214;
    const RANK_PREFIXES: u64 = 143;
    const YEAR_PREFIXES: u64 = 116;
    const LOOSE_ROOT_CLEAN: u64 = 237;
    const LOOSE_ROOT_TOTAL: u64 = 238;
}

/// Discovery-comparable noise-PRESENCE detectors: a marker counts if it appears
/// anywhere in the name (matching how the 2026-03-25 counts were derived from
/// the corpus), NOT the product's anchored trailing/leading strip passes. In
/// this library bitrate/size markers live inside the `[64k 16;59;40 471MB]`
/// bracket tags, so a presence detector is the only like-for-like comparison to
/// the FD-18 bitrate/size baselines.
struct NoiseDetectors {
    bracket: Regex,
    bitrate: Regex,
    size: Regex,
    rank: Regex,
    year: Regex,
}
impl NoiseDetectors {
    fn new() -> Self {
        NoiseDetectors {
            bracket: Regex::new(r"\[[^\]]+\]").unwrap(),
            bitrate: Regex::new(r"(?i)\b\d{2,4}\s?k(?:bps)?\b").unwrap(),
            size: Regex::new(r"(?i)\b\d+(?:\.\d+)?\s?(?:KB|MB|GB|TB)\b").unwrap(),
            rank: Regex::new(r"^\s*\d{1,3}\s*-\s").unwrap(),
            year: Regex::new(r"^\s*\d{4}\^?\s*-\s").unwrap(),
        }
    }
}

/// Every figure the baseline-delta report compares, over one snapshot.
#[derive(Debug, Default)]
struct SnapshotStats {
    entry_count: usize,
    folder_count: usize,
    file_count: usize,
    audio_file_count: usize,
    total_bytes: u64,

    book: u64,
    series_container: u64,
    pack_container: u64,
    staging: u64,
    mixed: u64,
    multi_book_suspect: u64,
    empty: u64,
    docs_resources: u64,
    manual_review: u64,

    book_like_folders: u64,   // book + multi-book-suspect
    estimated_abs_items: u64, // book-like folders + loose-root books

    bracket_tags: u64,
    bitrate_markers: u64,
    size_markers: u64,
    rank_prefixes: u64,
    year_prefixes: u64,

    loose_root_total: usize,
    loose_root_clean: usize,
}

fn compute_stats(rows: &[EntryRow], det: &NoiseDetectors) -> SnapshotStats {
    let inputs: Vec<ClassifyInput> = inputs_from_snapshot(rows);
    let cs = classify(&inputs);
    let m = health_metrics(&inputs, &cs);

    let class_count = |c: FolderClass| -> u64 {
        m.per_class
            .iter()
            .find(|x| x.class == c)
            .map(|x| x.folder_count)
            .unwrap_or(0)
    };

    let mut s = SnapshotStats {
        entry_count: inputs.len(),
        total_bytes: m.total_bytes,
        book: class_count(FolderClass::Book),
        series_container: class_count(FolderClass::SeriesContainer),
        pack_container: class_count(FolderClass::PackContainer),
        staging: class_count(FolderClass::Staging),
        mixed: class_count(FolderClass::Mixed),
        multi_book_suspect: class_count(FolderClass::MultiBookSuspect),
        empty: class_count(FolderClass::Empty),
        docs_resources: class_count(FolderClass::DocsResources),
        manual_review: class_count(FolderClass::ManualReview),
        ..Default::default()
    };

    s.folder_count = inputs.iter().filter(|e| e.kind == NodeKind::Folder).count();
    s.file_count = inputs.iter().filter(|e| e.kind == NodeKind::File).count();
    s.audio_file_count = rows
        .iter()
        .filter(|r| r.kind == "file" && r.file_class.as_deref() == Some("audio"))
        .count();

    // Noise PRESENCE over folder names (discovery-comparable).
    for r in rows.iter().filter(|r| r.kind == "dir") {
        if det.bracket.is_match(&r.name) {
            s.bracket_tags += 1;
        }
        if det.bitrate.is_match(&r.name) {
            s.bitrate_markers += 1;
        }
        if det.size.is_match(&r.name) {
            s.size_markers += 1;
        }
        if det.rank.is_match(&r.name) {
            s.rank_prefixes += 1;
        }
        if det.year.is_match(&r.name) {
            s.year_prefixes += 1;
        }
    }

    // Loose-root books: audio files DIRECTLY under the scan root (depth 1). In a
    // real single-rooted scan the root itself is entry depth 0, so "loose root"
    // is depth == 1, computed here from the raw `EntryRow.depth` field for the
    // name-parse-quality (clean/total) breakdown below. `health_metrics`'
    // "loose-root-books" problem metric is the product's own count of the same
    // files (fixed to use `parent`, not `depth`, since `ClassifyInput` carries
    // no depth field); the assertion just below checks the two agree.
    let loose_names: Vec<&str> = rows
        .iter()
        .filter(|r| r.depth == 1 && r.kind == "file" && r.file_class.as_deref() == Some("audio"))
        .map(|r| r.name.as_str())
        .collect();
    let cov = coverage(loose_names.iter().copied());
    s.loose_root_total = cov.total;
    s.loose_root_clean = cov.clean;

    // Cross-check: `health_metrics`' own loose-root-books problem metric (now
    // fixed to be robust to the real single-rooted scan shape, see
    // `classify::metrics`) must agree with the depth==1 count computed above
    // from the raw rows. They are two independent routes to the same number;
    // divergence here would mean the metric regressed.
    let hm_loose_count = m
        .problems
        .iter()
        .find(|p| p.problem == "loose-root-books")
        .map(|p| p.count)
        .unwrap_or(u64::MAX);
    assert_eq!(
        hm_loose_count, s.loose_root_total as u64,
        "health_metrics loose-root-books count must agree with the depth==1 count"
    );

    s.book_like_folders = s.book + s.multi_book_suspect;
    s.estimated_abs_items = s.book_like_folders + s.loose_root_total as u64;

    s
}

async fn temp_db() -> (TempDir, sqlx::SqlitePool) {
    let dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(dir.path()).await.expect("open_db");
    (dir, pool)
}

#[tokio::test]
#[ignore = "reads E:\\Books - Audio and the WizTree CSV; neither exists in CI. Run locally with --ignored --nocapture."]
async fn live_scan_vs_csv_vs_fd18_baseline() {
    let root = Path::new(LIBRARY_ROOT);
    if !root.is_dir() {
        println!("SKIP: {LIBRARY_ROOT} not present; the real-library gate is operator-run only.");
        return;
    }
    let det = NoiseDetectors::new();

    // ---- 1. Live read-only scan, timed ----
    let (_live_dir, live_pool) = temp_db().await;
    let started = Instant::now();
    let live_summary: ScanSummary = run_scan(&live_pool, root).await.expect("live scan");
    let live_elapsed = started.elapsed();

    let live_rows = get_scan_entries(&live_pool, live_summary.scan_id)
        .await
        .expect("read live entries");
    let live = compute_stats(&live_rows, &det);

    // ---- 2. WizTree CSV import of the 2026-03-25 export (G-07) ----
    let csv_path = workspace_root().join(WIZTREE_CSV_REL);
    let (csv, csv_summary) = if csv_path.is_file() {
        let (_csv_dir, csv_pool) = temp_db().await;
        let cs = run_csv_import(&csv_pool, &csv_path)
            .await
            .expect("csv import");
        let csv_rows = get_scan_entries(&csv_pool, cs.scan_id)
            .await
            .expect("read csv entries");
        (Some(compute_stats(&csv_rows, &det)), Some(cs))
    } else {
        println!(
            "NOTE: {} not found; CSV column omitted.",
            csv_path.display()
        );
        (None, None)
    };

    // ---- 3. Report ----
    println!("\n============================================================");
    println!(" v0.2.0 Phase 7 - real-library baseline delta (READ-ONLY)");
    println!("============================================================\n");

    println!("LIVE SCAN TIMING (G-03 NFR Scale, < 60 s soft target):");
    println!("  root                : {}", live_summary.root_path);
    println!(
        "  wall time           : {:.2} s",
        live_elapsed.as_secs_f64()
    );
    println!("  entries             : {}", live_summary.entry_count);
    println!(
        "  total bytes         : {} ({:.1} GB)",
        live_summary.total_bytes,
        live_summary.total_bytes as f64 / 1e9
    );
    println!("  skipped (walk)      : {}", live_summary.skipped_count);

    // Warnings SUMMARY by kind (not every line), with two examples each.
    println!("  warnings            : {}", live_summary.warnings.len());
    let mut by_kind: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for w in &live_summary.warnings {
        by_kind
            .entry(format!("{:?}", w.kind))
            .or_default()
            .push(&w.path);
    }
    for (kind, paths) in &by_kind {
        println!("    {kind}: {} occurrence(s); e.g.", paths.len());
        for p in paths.iter().take(2) {
            println!("      - {p}");
        }
    }

    if let Some(cs) = &csv_summary {
        println!("\nWIZTREE CSV IMPORT (2026-03-25, G-07):");
        println!("  root (as exported): {}", cs.root_path);
        println!("  entries           : {}", cs.entry_count);
        println!(
            "  total bytes       : {} ({:.1} GB)",
            cs.total_bytes,
            cs.total_bytes as f64 / 1e9
        );
        println!("  skipped rows      : {}", cs.skipped_count);
    }

    let csv_cell = |f: fn(&SnapshotStats) -> u64| -> String {
        csv.as_ref()
            .map(|s| f(s).to_string())
            .unwrap_or_else(|| "-".to_string())
    };
    println!(
        "\n{:<26} {:>18} {:>14} {:>14}",
        "METRIC", "2026-03-25 (FD-18)", "CSV 2026-03-25", "LIVE 2026-07-04"
    );
    println!("{}", "-".repeat(76));
    let row = |label: &str, base: String, csvc: String, live: String| {
        println!("{label:<26} {base:>18} {csvc:>14} {live:>14}");
    };
    row(
        "book-like folders",
        Fd18Baseline::BOOK_LIKE_FOLDERS.to_string(),
        csv_cell(|s| s.book_like_folders),
        live.book_like_folders.to_string(),
    );
    row(
        "  (book class)",
        "-".into(),
        csv_cell(|s| s.book),
        live.book.to_string(),
    );
    row(
        "  (multi-book-suspect)",
        "-".into(),
        csv_cell(|s| s.multi_book_suspect),
        live.multi_book_suspect.to_string(),
    );
    row(
        "mixed folders",
        Fd18Baseline::MIXED_FOLDERS.to_string(),
        csv_cell(|s| s.mixed),
        live.mixed.to_string(),
    );
    row(
        "estimated ABS items",
        Fd18Baseline::ESTIMATED_ABS_ITEMS.to_string(),
        csv_cell(|s| s.estimated_abs_items),
        live.estimated_abs_items.to_string(),
    );
    row(
        "bracket-tag names",
        Fd18Baseline::BRACKET_TAGS.to_string(),
        csv_cell(|s| s.bracket_tags),
        live.bracket_tags.to_string(),
    );
    row(
        "bitrate-marker names",
        Fd18Baseline::BITRATE_MARKERS.to_string(),
        csv_cell(|s| s.bitrate_markers),
        live.bitrate_markers.to_string(),
    );
    row(
        "size-marker names",
        Fd18Baseline::SIZE_MARKERS.to_string(),
        csv_cell(|s| s.size_markers),
        live.size_markers.to_string(),
    );
    row(
        "rank-prefix names",
        Fd18Baseline::RANK_PREFIXES.to_string(),
        csv_cell(|s| s.rank_prefixes),
        live.rank_prefixes.to_string(),
    );
    row(
        "year-prefix names",
        Fd18Baseline::YEAR_PREFIXES.to_string(),
        csv_cell(|s| s.year_prefixes),
        live.year_prefixes.to_string(),
    );
    row(
        "loose-root parses",
        format!(
            "{}/{}",
            Fd18Baseline::LOOSE_ROOT_CLEAN,
            Fd18Baseline::LOOSE_ROOT_TOTAL
        ),
        csv.as_ref()
            .map(|s| format!("{}/{}", s.loose_root_clean, s.loose_root_total))
            .unwrap_or_else(|| "-".into()),
        format!("{}/{}", live.loose_root_clean, live.loose_root_total),
    );

    println!("\nFULL PER-CLASS FOLDER COUNTS (CSV | LIVE):");
    let cls = |label: &str, f: fn(&SnapshotStats) -> u64| {
        println!("  {:<20} {:>8} {:>8}", label, csv_cell(f), f(&live));
    };
    cls("book", |s| s.book);
    cls("series-container", |s| s.series_container);
    cls("pack-container", |s| s.pack_container);
    cls("staging", |s| s.staging);
    cls("mixed", |s| s.mixed);
    cls("multi-book-suspect", |s| s.multi_book_suspect);
    cls("empty", |s| s.empty);
    cls("docs-resources", |s| s.docs_resources);
    cls("manual-review", |s| s.manual_review);

    let shape = |label: &str, c: String, l: String| println!("  {label:<20} {c:>8} {l:>8}");
    println!("\nSNAPSHOT SHAPE (CSV | LIVE):");
    shape(
        "entries",
        csv.as_ref()
            .map(|s| s.entry_count.to_string())
            .unwrap_or_else(|| "-".into()),
        live.entry_count.to_string(),
    );
    shape(
        "folders",
        csv.as_ref()
            .map(|s| s.folder_count.to_string())
            .unwrap_or_else(|| "-".into()),
        live.folder_count.to_string(),
    );
    shape(
        "files",
        csv.as_ref()
            .map(|s| s.file_count.to_string())
            .unwrap_or_else(|| "-".into()),
        live.file_count.to_string(),
    );
    shape(
        "audio files",
        csv.as_ref()
            .map(|s| s.audio_file_count.to_string())
            .unwrap_or_else(|| "-".into()),
        live.audio_file_count.to_string(),
    );
    shape(
        "total bytes",
        csv.as_ref()
            .map(|s| s.total_bytes.to_string())
            .unwrap_or_else(|| "-".into()),
        live.total_bytes.to_string(),
    );

    println!("\n(Deltas are REPORTED, not judged: the library has changed since 2026-03-25;");
    println!(" the delta IS the deliverable per FD-18 / OQ-2. G-03 does not fail on drift.)\n");

    assert!(
        live_summary.entry_count > 0,
        "the live scan recorded entries"
    );
    assert!(
        live.book + live.multi_book_suspect > 0,
        "the library holds books"
    );
    assert_eq!(live_summary.status, "completed", "the scan completed");
}
