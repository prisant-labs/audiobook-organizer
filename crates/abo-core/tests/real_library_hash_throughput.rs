//! v0.6.0 hardening `P2`, `AC-16` (hash throughput on real data): measure what
//! `F-702` (hash verification) actually costs against the real library, so the
//! descope decision is made on a number rather than an intuition.
//!
//! # What `AC-16` decides
//!
//! `AC-16` says the release ships either way: if hashing is too slow to be
//! usable, the campaign runs duplicates as flag-only and set-aside-by-hash
//! becomes post-release work. That is a decision about a measured rate, and
//! nothing had measured it. This file is the measurement.
//!
//! # Every test here is `#[ignore]` on purpose
//!
//! Two of them read `E:\Books - Audio`, which does not exist on the CI runners
//! (ubuntu and windows GitHub Actions), so they must never run in the CI `test`
//! job. The third writes a gigabyte of scratch, which is rude in CI for a
//! different reason. All three are operator-run evidence generators, following
//! the precedent set by `real_library_scan.rs`. Run them locally with:
//!
//! ```text
//! cargo test -p abo-core --test real_library_hash_throughput -- --ignored --nocapture
//! ```
//!
//! Strictly READ-ONLY against the library (`D-09`): `run_scan` is a walk and a
//! stat, and hashing opens files for reading. Nothing under `E:\Books - Audio`
//! is created, modified, renamed, or deleted. The only writes are into a
//! throwaway temp-dir SQLite database.
//!
//! # Why the measurement hashes EXACT FILE groups only
//!
//! Detection returns two shapes of group since `F-1110` (book-level duplicate
//! comparison): exact basename+size groups whose members are FILES, and book
//! groups whose members are FOLDERS. [`verify_groups`] is the `F-702` job and it
//! hashes member paths directly, so handing it a folder member would record a
//! read failure and quietly corrupt both the failure count and the byte total.
//! Book groups reach content comparison through `verify_book_group` instead
//! (`AC-54`, on request only), and their volume is REPORTED here rather than
//! hashed: it is a different, opt-in path, and one measured throughput figure
//! projects onto it by arithmetic.
//!
//! # The population is the point, not the library size
//!
//! `AC-10` forbids a hash-everything path, so "how long to hash 296 GB" is the
//! wrong question and answering it would argue for a descope that the product's
//! own design already avoids. The right question is how long to hash the
//! CANDIDATES, which the size-and-name detector has already narrowed down for
//! free. Both figures are printed, so the contrast is visible.

use std::path::Path;
use std::time::Instant;

use abo_core::db::dupes::insert_duplicate_groups;
use abo_core::db::open_db;
use abo_core::dupes::detect::dupe_entries_from_plan_nodes;
use abo_core::dupes::{
    book_folders_from_plan_nodes, detect_duplicates, verify_groups, DuplicateGroup,
    FsContentSource, READ_BUFFER_BYTES,
};
use abo_core::job::JobContext;
use abo_core::parse::extract::{extract, EntryInput};
use abo_core::plan::builder::plan_nodes_from_snapshot;
use abo_core::scan::{get_scan_entries, run_scan};
use tempfile::TempDir;

/// The real library root (read-only).
const LIBRARY_ROOT: &str = r"E:\Books - Audio";

/// Optional ceiling on the bytes the throughput test will hash, in gigabytes,
/// read from `ABO_AC16_MAX_GB`. Unset means hash every exact candidate.
///
/// A cap exists so the measurement can be taken without committing to a
/// multi-hour run, and whatever it excludes is printed rather than silently
/// dropped: a throughput figure whose population was quietly truncated is the
/// kind of number that reads as thorough and is not.
const MAX_GB_ENV: &str = "ABO_AC16_MAX_GB";

async fn temp_db() -> (TempDir, sqlx::SqlitePool) {
    let dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(dir.path()).await.expect("open_db");
    (dir, pool)
}

fn gb(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_048_576.0
}

/// Scan the real library and run the production detector over it.
///
/// Mirrors `plan::query::duplicate_groups_for_scan`, which is private: the same
/// steps in the same order, so what is measured here is what the product does
/// rather than a parallel implementation of it.
async fn detect_over_real_library(
    pool: &sqlx::SqlitePool,
    root: &Path,
) -> (Vec<DuplicateGroup>, u64, u64) {
    let summary = run_scan(pool, root).await.expect("scan the real library");
    let rows = get_scan_entries(pool, summary.scan_id)
        .await
        .expect("read scan entries");

    let file_count = rows.iter().filter(|r| r.kind == "file").count() as u64;
    let total_bytes: u64 = rows
        .iter()
        .filter(|r| r.kind == "file")
        .map(|r| r.size.max(0) as u64)
        .sum();

    let nodes = plan_nodes_from_snapshot(&rows);
    let entry_inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    let merged = extract(&entry_inputs);
    let dupe_entries = dupe_entries_from_plan_nodes(&nodes, &merged);
    let books = book_folders_from_plan_nodes(&nodes, &merged);

    (
        detect_duplicates(&dupe_entries, &books),
        file_count,
        total_bytes,
    )
}

/// Print the candidate population without hashing anything.
///
/// Evidence in its own right, and the thing to read BEFORE the throughput run:
/// it says how much work `AC-16` is actually asking about. If the exact
/// candidate population turns out to be small, that is the answer to `AC-16`
/// and no descope argument survives it.
#[tokio::test]
#[ignore = "reads E:\\Books - Audio, which does not exist in CI. Run locally with --ignored --nocapture."]
async fn ac16_candidate_population() {
    let root = Path::new(LIBRARY_ROOT);
    if !root.is_dir() {
        println!("SKIP: {LIBRARY_ROOT} not present; this gate is operator-run only.");
        return;
    }

    let (_dir, pool) = temp_db().await;
    let started = Instant::now();
    let (groups, file_count, total_bytes) = detect_over_real_library(&pool, root).await;
    let detect_elapsed = started.elapsed();

    let exact: Vec<&DuplicateGroup> = groups
        .iter()
        .filter(|g| g.is_exact() && g.is_duplicate_candidate())
        .collect();
    let subsumed = groups.iter().filter(|g| g.is_exact()).count() - exact.len();
    let book_candidates: Vec<&DuplicateGroup> = groups
        .iter()
        .filter(|g| !g.is_exact() && g.is_duplicate_candidate())
        .collect();

    let exact_members: usize = exact.iter().map(|g| g.members.len()).sum();
    let exact_bytes: u64 = exact.iter().map(|g| g.total_bytes).sum();
    let book_members: usize = book_candidates.iter().map(|g| g.members.len()).sum();
    let book_bytes: u64 = book_candidates.iter().map(|g| g.total_bytes).sum();

    println!("\n=== AC-16: the candidate population on real data ===");
    println!("Library root      : {LIBRARY_ROOT}");
    println!(
        "Files / bytes     : {file_count} files, {:.2} GB",
        gb(total_bytes)
    );
    println!("Detection time    : {:.1}s", detect_elapsed.as_secs_f64());
    println!("\n-- What F-702 would hash (AC-10: candidates only) --");
    println!(
        "Exact file groups : {} groups, {exact_members} members, {:.2} GB  <-- the AC-16 population",
        exact.len(),
        gb(exact_bytes)
    );
    println!(
        "  as a share of the library: {:.3}% of bytes",
        if total_bytes == 0 {
            0.0
        } else {
            100.0 * exact_bytes as f64 / total_bytes as f64
        }
    );
    println!(
        "Book folder groups: {} groups, {book_members} members, {:.2} GB (AC-54 content tier, ON REQUEST ONLY, not hashed here)",
        book_candidates.len(),
        gb(book_bytes)
    );
    println!(
        "Subsumed exact grp: {subsumed} (true, but not counted: a book group already reports the same duplication)"
    );
    println!("=== end population ===\n");
}

/// What the read path itself can sustain, with the mechanical disk taken out of
/// the picture.
///
/// # Why this exists
///
/// The library measurement answers "how long does a user wait". It does NOT
/// answer "is the code the reason", and those call for different responses: a
/// slow medium is a fact to design around, while a slow read loop or a slow
/// hash is a defect to fix. Reporting the first number as though it settled the
/// second is how a descope decision gets made against the wrong cause.
///
/// So this hashes a temp file through the SAME [`FsContentSource`], twice. The
/// file lands in the system temp directory (an NVMe SSD on the development
/// machine, not the library's 7200 RPM SATA drive) and the second pass reads it
/// warm from the OS cache. The warm figure is the ceiling: it is what the read
/// loop plus BLAKE3 achieve when nothing is waiting on a platter.
///
/// Self-contained, and `#[ignore]` only because writing a gigabyte of scratch is
/// rude in CI, not because it needs the real library.
#[tokio::test]
#[ignore = "writes ~1 GB of scratch to the temp dir. Run locally with --ignored --nocapture."]
async fn ac16_read_path_ceiling() {
    const SCRATCH_BYTES: usize = 1 << 30; // 1 GiB

    let dir = TempDir::new().expect("scratch tempdir");
    let path = dir.path().join("ceiling.bin");

    // Not a repeating pattern and not random: cheap to generate, and neither
    // the filesystem nor the hasher gets to shortcut it.
    let block: Vec<u8> = (0..(1 << 20)).map(|i| (i % 251) as u8).collect();
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&path).expect("create scratch");
        for _ in 0..(SCRATCH_BYTES / block.len()) {
            f.write_all(&block).expect("write scratch");
        }
        f.sync_all().expect("flush scratch");
    }

    let p = path.to_str().unwrap();
    let src = FsContentSource;

    let started = Instant::now();
    let first = abo_core::dupes::hash_member(&src, p).expect("hash cold");
    let cold = started.elapsed().as_secs_f64();

    let started = Instant::now();
    let second = abo_core::dupes::hash_member(&src, p).expect("hash warm");
    let warm = started.elapsed().as_secs_f64();

    assert_eq!(first, second, "the same file must hash to the same digest");

    println!("\n=== AC-16: read-path ceiling (library NOT involved) ===");
    println!(
        "Scratch file      : {:.2} GB in the system temp dir",
        gb(SCRATCH_BYTES as u64)
    );
    println!("Read buffer       : {} KiB", READ_BUFFER_BYTES / 1024);
    println!(
        "First pass        : {cold:.2}s = {:.0} MB/s",
        mb(SCRATCH_BYTES as u64) / cold
    );
    println!(
        "Second pass (warm): {warm:.2}s = {:.0} MB/s   <-- the ceiling: read loop + BLAKE3, no platter",
        mb(SCRATCH_BYTES as u64) / warm
    );
    println!("=== end ceiling ===\n");
}

/// The `AC-16` measurement: hash the exact candidate population through the
/// real job and the real read path, and report the rate.
#[tokio::test]
#[ignore = "reads E:\\Books - Audio, which does not exist in CI. Run locally with --ignored --nocapture."]
async fn ac16_hash_throughput() {
    let root = Path::new(LIBRARY_ROOT);
    if !root.is_dir() {
        println!("SKIP: {LIBRARY_ROOT} not present; this gate is operator-run only.");
        return;
    }

    let (_dir, pool) = temp_db().await;
    let (groups, _file_count, total_bytes) = detect_over_real_library(&pool, root).await;

    // EXACT groups only. A book group's members are folders, and handing a
    // folder path to the file hasher records a read failure that pollutes both
    // the failure count and the rate.
    let mut exact: Vec<DuplicateGroup> = groups
        .iter()
        .filter(|g| g.is_exact() && g.is_duplicate_candidate())
        .cloned()
        .collect();

    // Largest groups first, so a capped run measures the files that dominate
    // the cost rather than a tail of small ones.
    exact.sort_by_key(|g| std::cmp::Reverse(g.total_bytes));

    let full_group_count = exact.len();
    let full_bytes: u64 = exact.iter().map(|g| g.total_bytes).sum();

    // Apply the optional cap, and remember exactly what it left out.
    let cap_bytes = std::env::var(MAX_GB_ENV)
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|g| (g * 1_073_741_824.0) as u64);
    if let Some(cap) = cap_bytes {
        let mut running = 0u64;
        exact.retain(|g| {
            if running + g.total_bytes <= cap {
                running += g.total_bytes;
                true
            } else {
                false
            }
        });
    }

    let measured_group_count = exact.len();
    let measured_bytes: u64 = exact.iter().map(|g| g.total_bytes).sum();
    let measured_members: usize = exact.iter().map(|g| g.members.len()).sum();

    if exact.is_empty() {
        println!(
            "\n=== AC-16 ===\nNo exact duplicate candidates in the real library: there is nothing for F-702 to hash, \
             and the throughput question does not arise. Population report: run ac16_candidate_population.\n"
        );
        return;
    }

    let group_ids = insert_duplicate_groups(&pool, 1, &exact, "2026-08-15T00:00:00Z")
        .await
        .expect("persist candidate groups into the temp db");

    // The real job, the real read path, timed end to end.
    let ctx = JobContext::inert();
    let started = Instant::now();
    let outcome = verify_groups(&pool, &FsContentSource, &group_ids, &ctx)
        .await
        .expect("verify");
    let elapsed = started.elapsed();

    let secs = elapsed.as_secs_f64();
    let rate_mb_s = if secs > 0.0 {
        mb(measured_bytes) / secs
    } else {
        0.0
    };

    println!("\n=== AC-16: hash throughput on real data ===");
    println!("Library root      : {LIBRARY_ROOT}");
    println!("Read buffer       : {} KiB", READ_BUFFER_BYTES / 1024);
    println!("Path              : verify_groups + FsContentSource (the shipping code)");
    if let Some(cap) = cap_bytes {
        println!(
            "CAP APPLIED       : {} = {:.2} GB. Measured {measured_group_count} of {full_group_count} groups; \
             EXCLUDED {} groups totalling {:.2} GB.",
            MAX_GB_ENV,
            gb(cap),
            full_group_count - measured_group_count,
            gb(full_bytes - measured_bytes)
        );
    } else {
        println!("Cap               : none; every exact candidate was hashed");
    }
    println!("\n-- Measured --");
    println!("Groups / members  : {measured_group_count} groups, {measured_members} members");
    println!("Bytes hashed      : {:.2} GB", gb(measured_bytes));
    println!("Wall time         : {secs:.1}s");
    println!("THROUGHPUT        : {rate_mb_s:.0} MB/s");
    println!(
        "Outcome           : {} hashed, {} failed, {} skipped, cancelled={}",
        outcome.hashed, outcome.failed, outcome.skipped, outcome.cancelled
    );

    println!("\n-- Projected at the measured rate --");
    let project = |bytes: u64| -> f64 {
        if rate_mb_s > 0.0 {
            mb(bytes) / rate_mb_s
        } else {
            f64::INFINITY
        }
    };
    println!(
        "Every exact candidate ({:.2} GB): {:.1}s   <-- what a user waits for",
        gb(full_bytes),
        project(full_bytes)
    );
    println!(
        "The whole library ({:.2} GB)    : {:.0}s  <-- the hash-everything path AC-10 forbids, for contrast",
        gb(total_bytes),
        project(total_bytes)
    );
    println!("=== end AC-16 ===\n");

    // The measurement is the deliverable, but a run that could not read the
    // library is not a measurement. Failing here rather than printing a
    // confident zero is the difference between evidence and a plausible number.
    assert!(
        outcome.hashed > 0,
        "AC-16 measured nothing: {} members failed to read. A throughput figure from a run that \
         read no bytes would be a fabrication.",
        outcome.failed
    );
}
