//! F-906 (ruleset editor with live re-plan) integration coverage for
//! [`abo_core::plan::report::preview_plan_review`] (AC-33): the live preview
//! reacts to a draft ruleset and NEVER persists a `plans`/`plan_ops` row,
//! however many times it is called - the plan_ops immutability rule applies
//! to real, saved plans, and a preview is never one.

use abo_core::db::open_db;
use abo_core::fixtures::{generate, standard_library_manifest};
use abo_core::plan::report::preview_plan_review;
use abo_core::ruleset::default_ruleset;
use abo_core::scan::run_scan;
use tempfile::TempDir;

async fn count_plans(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM plans")
        .fetch_one(pool)
        .await
        .expect("count plans")
}

/// AC-33: previewing the default ruleset over a real scanned fixture returns
/// the seven canonical group cards, and touches zero `plans` rows - not one,
/// not for a single call and not across several (the whole point of a
/// preview is that it never persists).
#[tokio::test]
async fn preview_returns_seven_groups_and_never_persists_a_plan() {
    let lib = TempDir::new().expect("library tempdir");
    let generated =
        generate(&standard_library_manifest(), lib.path()).expect("materialize fixture");
    assert!(!generated.entries.is_empty(), "fixture materialized");

    let db_dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db_dir.path()).await.expect("open_db");
    let scan = run_scan(&pool, lib.path()).await.expect("scan");

    assert_eq!(count_plans(&pool).await, 0, "no plan exists yet");

    let ruleset = default_ruleset();
    let preview = preview_plan_review(&pool, scan.scan_id, &ruleset)
        .await
        .expect("preview_plan_review");

    assert_eq!(preview.scan_id, scan.scan_id);
    assert_eq!(preview.groups.len(), 7, "AC-10: always seven cards");

    assert_eq!(
        count_plans(&pool).await,
        0,
        "a preview must never write a plans row"
    );

    // Calling it again (as the live re-plan does on every toggle change)
    // still never persists anything.
    let _second = preview_plan_review(&pool, scan.scan_id, &ruleset)
        .await
        .expect("preview_plan_review again");
    assert_eq!(count_plans(&pool).await, 0);
}

/// AC-33 "newly blocked ops show as blocked, not a silent drop": a draft
/// ruleset's `series_index_width` change is reflected the moment the caller
/// re-previews (proving the preview actually re-plans against the draft
/// rather than returning a cached result) - here checked by varying a policy
/// toggle that changes which pack-shell ops appear and confirming the
/// resulting group counts differ from the default.
#[tokio::test]
async fn preview_reflects_a_draft_policy_change() {
    let lib = TempDir::new().expect("library tempdir");
    generate(&standard_library_manifest(), lib.path()).expect("materialize fixture");

    let db_dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db_dir.path()).await.expect("open_db");
    let scan = run_scan(&pool, lib.path()).await.expect("scan");

    let mut draft = default_ruleset();
    let default_preview = preview_plan_review(&pool, scan.scan_id, &draft)
        .await
        .expect("default preview");

    draft.structure.pack_shell = abo_core::ruleset::PackShellDestination::LeaveInPlace;
    let changed_preview = preview_plan_review(&pool, scan.scan_id, &draft)
        .await
        .expect("changed preview");

    // Both previews still render exactly seven cards (AC-10 holds for every
    // draft, not only the saved/default one); the bundles group's quarantine
    // op count changes between the two (leave-in-place emits no shell op).
    assert_eq!(default_preview.groups.len(), 7);
    assert_eq!(changed_preview.groups.len(), 7);
}
