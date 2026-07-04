//! v0.2.0 Phase 7, release gate G-08: the FD-14 tag-quality probe.
//!
//! The whole file is guarded by `#![cfg(feature = "probe")]`, so in the default
//! (no-probe) build - including the CI `test` job's `cargo test --workspace` -
//! it compiles to nothing (no missing `abo_core::probe` symbol). It builds for
//! real only under `--features probe`, and even then the probe itself is
//! `#[ignore]` because it reads `E:\Books - Audio`, which the CI runners do not
//! have. Run it locally with:
//!
//! ```text
//! cargo test -p abo-core --features probe --test fd14_tag_probe -- --ignored --nocapture
//! ```
//!
//! It is strictly READ-ONLY: it runs a live scan (walk + stat), then samples a
//! few hundred real audio files and reads their embedded tags through lofty,
//! which only ever reads. No file under the library is created, modified,
//! renamed, or deleted (D-09; this phase's absolute constraint). Its printed
//! output feeds the committed FD-14 report
//! (docs/internal/releases/v0.2.0-understanding/fd14-tag-probe-2026-07-04.md),
//! whose VERDICT tunes the F-303 confidence weighting.
#![cfg(feature = "probe")]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use abo_core::db::open_db;
use abo_core::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};
use abo_core::probe::{deterministic_sample_indices, pct, run_probe, ProbeInput};
use abo_core::scan::{get_scan_entries, run_scan};
use tempfile::TempDir;

const LIBRARY_ROOT: &str = r"E:\Books - Audio";
/// Cap on sampled audio files (this phase's "cap ~300").
const SAMPLE_CAP: usize = 300;

#[tokio::test]
#[ignore = "reads E:\\Books - Audio embedded tags; the library does not exist in CI. Run locally with --features probe --ignored --nocapture."]
async fn fd14_tag_quality_probe() {
    let root = Path::new(LIBRARY_ROOT);
    if !root.is_dir() {
        println!("SKIP: {LIBRARY_ROOT} not present; the FD-14 probe is operator-run only.");
        return;
    }

    // ---- 1. Fresh read-only scan ----
    let dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(dir.path()).await.expect("open_db");
    let started = Instant::now();
    let summary = run_scan(&pool, root).await.expect("live scan");
    let scan_elapsed = started.elapsed();
    let rows = get_scan_entries(&pool, summary.scan_id)
        .await
        .expect("read entries");

    // ---- 2. Folder-first parse of the whole tree (the product's F-303 merge) ----
    let entry_inputs: Vec<EntryInput> = rows
        .iter()
        .map(|r| EntryInput {
            id: r.id as usize,
            parent: r.parent_id.map(|p| p as usize),
            name: r.name.clone(),
            kind: if r.kind == "dir" {
                NodeKind::Folder
            } else {
                NodeKind::File
            },
        })
        .collect();
    let merged_by_id: HashMap<usize, MergedEntry> = extract(&entry_inputs)
        .into_iter()
        .map(|m| (m.id, m))
        .collect();

    // ---- 3. Deterministic sample of audio files (path-sorted order) ----
    let audio: Vec<_> = rows
        .iter()
        .filter(|r| r.kind == "file" && r.file_class.as_deref() == Some("audio"))
        .collect();
    let idx = deterministic_sample_indices(audio.len(), SAMPLE_CAP);

    let probe_inputs: Vec<ProbeInput> = idx
        .iter()
        .map(|&i| {
            let r = audio[i];
            let merged = merged_by_id.get(&(r.id as usize));
            let folder_title = merged
                .and_then(|m| m.fields.title.as_ref())
                .map(|f| f.value.clone());
            let folder_author = merged
                .and_then(|m| m.fields.author.as_ref())
                .map(|f| f.value.clone());
            ProbeInput {
                path: PathBuf::from(&r.path),
                folder_title,
                folder_author,
            }
        })
        .collect();

    // ---- 4. Read tags (READ-ONLY) and aggregate ----
    let probe_started = Instant::now();
    let report = run_probe(&probe_inputs);
    let probe_elapsed = probe_started.elapsed();

    // ---- 5. Report ----
    println!("\n============================================================");
    println!(" v0.2.0 Phase 7 - FD-14 tag-quality probe (READ-ONLY, G-08)");
    println!("============================================================\n");
    println!(
        "scan: {} entries in {:.2} s",
        summary.entry_count,
        scan_elapsed.as_secs_f64()
    );
    println!("audio files in library : {}", audio.len());
    println!("sample cap             : {SAMPLE_CAP}");
    println!(
        "sampled (every ~{}th)  : {}",
        if audio.len() > SAMPLE_CAP {
            audio.len() / SAMPLE_CAP
        } else {
            1
        },
        report.sampled
    );
    println!(
        "tag read wall time     : {:.2} s",
        probe_elapsed.as_secs_f64()
    );
    println!();
    println!("READABILITY:");
    println!(
        "  lofty-readable       : {} ({:.1}%)",
        report.readable,
        pct(report.readable, report.sampled)
    );
    println!(
        "  unreadable           : {} ({:.1}%)",
        report.unreadable,
        pct(report.unreadable, report.sampled)
    );
    println!(
        "  had any usable tag   : {} ({:.1}%)",
        report.with_any_tag,
        pct(report.with_any_tag, report.sampled)
    );
    println!();
    println!(
        "TAG FIELD COMPLETENESS (of {} sampled files):",
        report.sampled
    );
    println!(
        "  title  present       : {} ({:.1}%)",
        report.title_present,
        report.title_present_pct()
    );
    println!(
        "  author present       : {} ({:.1}%)",
        report.author_present,
        report.author_present_pct()
    );
    println!(
        "  album  present       : {} ({:.1}%)",
        report.album_present,
        report.album_present_pct()
    );
    println!(
        "  narrator present     : {} ({:.1}%)",
        report.narrator_present,
        report.narrator_present_pct()
    );
    println!();
    println!("FOLDER-FIRST COMPLETENESS (same sample, for comparison):");
    println!(
        "  folder title present : {} ({:.1}%)",
        report.folder_title_present,
        report.folder_title_present_pct()
    );
    println!(
        "  folder author present: {} ({:.1}%)",
        report.folder_author_present,
        report.folder_author_present_pct()
    );
    println!();
    println!("TAG vs FOLDER AGREEMENT (over files where BOTH exist):");
    println!("  title  comparable    : {}", report.title_comparable);
    println!(
        "  title  agree         : {} ({:.1}%)",
        report.title_agree,
        report.title_agreement_pct()
    );
    println!("  author comparable    : {}", report.author_comparable);
    println!(
        "  author agree         : {} ({:.1}%)",
        report.author_agree,
        report.author_agreement_pct()
    );
    println!();

    // ---- 6. Computed verdict signal (the written verdict lives in the report doc) ----
    // "Materially more complete" trip-wire (this phase's risk table): tags beat
    // folder names on title OR author completeness by a wide margin over a large
    // fraction. If so, DO NOT change the default here - flag OQ-1 for jp.
    let margin = 15.0; // percentage points
    let title_more_complete =
        report.title_present_pct() > report.folder_title_present_pct() + margin;
    let author_more_complete =
        report.author_present_pct() > report.folder_author_present_pct() + margin;
    let tags_materially_more_complete = title_more_complete || author_more_complete;

    println!("VERDICT SIGNAL (folder-first assumption, FD-14):");
    if tags_materially_more_complete {
        println!("  ** tags look MATERIALLY MORE complete than folder names.");
        println!("  ** Per the risk table: DO NOT change the v1 folder-first default here;");
        println!("  ** record and FLAG OQ-1 (preferSource reopen?) for jp at the G-08 gate.");
    } else {
        println!("  folder-first HOLDS: tags are not materially more complete than folder");
        println!("  names (folder names carry title/author at least as reliably). The v1");
        println!("  folder-first default (FD-14) stands; F-303 keeps its default weighting.");
    }
    println!("\n(READ-ONLY: no file under the library was written. Written verdict:");
    println!(" docs/internal/releases/v0.2.0-understanding/fd14-tag-probe-2026-07-04.md)\n");

    // Sanity assertions (not a gate judgment).
    assert!(
        report.sampled > 0,
        "the probe sampled at least one audio file"
    );
    assert!(report.sampled <= SAMPLE_CAP, "the sample respected the cap");
}
