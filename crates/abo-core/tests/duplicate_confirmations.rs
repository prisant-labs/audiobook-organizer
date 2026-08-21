//! F-905 / AC-24: a confirmed duplicate resolution becomes Archive operations,
//! and a fresh scan starts undecided.
//!
//! # What this file proves, and what it does NOT
//!
//! It proves the CHAIN: a confirmation recorded against a scan reaches the plan
//! builder and produces exactly one Archive operation for exactly the copy that
//! was confirmed, an unconfirmed group produces none, and a second scan of the
//! same library plans clean.
//!
//! It does NOT prove the `scan_id` filter in
//! [`abo_core::db::dupes::confirmations_for_scan`], and that was checked rather
//! than assumed: with the filter deliberately removed, this test still passed.
//! The reason is that `entries.id` is a global rowid, so a second scan's ids
//! never coincide with the first's, and the builder drops confirmations naming
//! ids absent from the snapshot it is building against. Two mechanisms guard one
//! hazard, and this fixture exercises the weaker one.
//!
//! The filter is proved where it can fail: `a_confirmation_made_against_one_scan
//! _is_invisible_to_another` in `db::dupes`, which DOES fail when the filter is
//! removed. Both tests are kept, because they are about different things.
//!
//! # Why the filter still matters
//!
//! `entries.id` is a plain rowid, and SQLite reuses rowids once rows are deleted.
//! Nothing deletes them today, but `FD-20` snapshot retention is a configured
//! setting waiting for an implementation, and the day it lands, an old
//! confirmation pointing at a reused id would name a file nobody chose. Step 7
//! below records the third guard against that: the confirmation's foreign key
//! means the entry it names cannot be deleted out from under it, so retention
//! will have to remove confirmations deliberately rather than orphan them by
//! accident.

use abo_core::db::dupes::confirm_resolution;
use abo_core::db::open_db;
use abo_core::dupes::{ensure_duplicate_groups, ConfirmedResolution};
use abo_core::plan::report::build_and_persist_plan;
use abo_core::ruleset::seed_default_ruleset;
use abo_core::scan::run_scan;
use abo_core::scan::walk::now_iso8601_utc;
use sqlx::SqlitePool;
use tempfile::TempDir;

/// The same book, twice, as files: the plainest duplicate there is, and the only
/// shape `P3` emission covers this release (Archive of a folder loser waits on a
/// cross-volume round-trip being proven).
fn library_with_two_copies() -> TempDir {
    let lib = TempDir::new().expect("library tempdir");
    for folder in ["A", "B"] {
        let dir = lib.path().join(folder);
        std::fs::create_dir_all(&dir).expect("create copy folder");
        std::fs::write(dir.join("Dune.m4b"), b"the same bytes in both copies").expect("write copy");
    }
    lib
}

/// Every `quarantine` op in a freshly built plan. `quarantine` is the internal
/// op kind for what a user reads as "Archive" (`FD-42` renamed the word, not the
/// schema).
async fn archive_ops(pool: &SqlitePool, scan_id: i64, ruleset_id: i64) -> Vec<String> {
    let built = build_and_persist_plan(pool, scan_id, ruleset_id)
        .await
        .expect("build a plan");
    built
        .plan
        .ops
        .iter()
        .filter(|op| op.kind == "quarantine")
        .map(|op| op.source_path.clone())
        .collect()
}

#[tokio::test]
async fn a_confirmed_resolution_becomes_an_archive_op_and_does_not_outlive_its_scan() {
    let lib = library_with_two_copies();
    let db_dir = TempDir::new().expect("db tempdir");
    let (pool, _) = open_db(db_dir.path()).await.expect("open_db");
    let ruleset_id = seed_default_ruleset(&pool, "Default", &now_iso8601_utc())
        .await
        .expect("seed ruleset");

    // 1. Scan, and detect the duplicate the way the app does.
    let first = run_scan(&pool, lib.path()).await.expect("first scan");
    let persisted = ensure_duplicate_groups(&pool, first.scan_id, &now_iso8601_utc())
        .await
        .expect("detect duplicates");
    assert_eq!(
        persisted.groups.len(),
        1,
        "one duplicated book in the fixture"
    );
    let (_, group) = &persisted.groups[0];
    assert_eq!(group.members.len(), 2, "two copies of it");

    // 2. Before any confirmation the plan is flag-only: AC-26 is satisfied by
    //    there being nothing to emit from, not by a branch that suppresses it.
    assert!(
        archive_ops(&pool, first.scan_id, ruleset_id)
            .await
            .is_empty(),
        "an unconfirmed duplicate must never produce an Archive operation"
    );

    // 3. Confirm: keep the first copy, archive the second.
    let keeper = group.members[0].entry_id;
    let loser = group.members[1].entry_id;
    let loser_path = group.members[1].path.clone();
    confirm_resolution(
        &pool,
        first.scan_id,
        group.method,
        &group.group_key,
        &ConfirmedResolution {
            keeper,
            losers: vec![loser],
        },
        // Recorded as an override: this fixture confirms without hashing, which
        // is exactly the case AC-13 exists for, and the flag says so honestly.
        true,
        &now_iso8601_utc(),
    )
    .await
    .expect("record the confirmation");

    // 4. Now the plan carries it, and carries exactly it.
    let ops = archive_ops(&pool, first.scan_id, ruleset_id).await;
    assert_eq!(
        ops,
        vec![loser_path],
        "the confirmed loser, and only the confirmed loser, is archived"
    );

    // 5. A SECOND scan of the same library. Same files on disk, new snapshot,
    //    new entry ids. The confirmation belongs to the first scan and must be
    //    invisible here.
    let second = run_scan(&pool, lib.path()).await.expect("second scan");
    assert_ne!(second.scan_id, first.scan_id, "a genuinely new snapshot");

    assert!(
        archive_ops(&pool, second.scan_id, ruleset_id)
            .await
            .is_empty(),
        "a re-scan must start undecided rather than inheriting the last scan's \
         answers: FD-39 re-plans from a fresh scan, and a plan that quietly \
         carried old decisions would archive files this scan never showed anyone"
    );

    // 6. And the original scan still holds its own decision, so the isolation is
    //    a filter rather than a deletion.
    assert_eq!(
        archive_ops(&pool, first.scan_id, ruleset_id).await.len(),
        1,
        "the first scan's confirmation survived the second scan"
    );

    // 7. THE HAZARD THIS SCHEMA HAS TO SURVIVE LATER. `entries.id` is a plain
    //    rowid, so SQLite is free to reuse it once rows are deleted, and `FD-20`
    //    snapshot retention (configured, not yet implemented) will delete old
    //    scans. A confirmation left pointing at a reused id is the failure the
    //    scan_id filter exists to prevent. The database refuses to create that
    //    situation in the first place: the confirmation's foreign key means the
    //    entry it names cannot be deleted out from under it.
    let orphaning = sqlx::query("DELETE FROM entries WHERE id = ?")
        .bind(loser as i64)
        .execute(&pool)
        .await;
    assert!(
        orphaning.is_err(),
        "deleting an entry a confirmation names must be refused, or retention \
         could leave a decision pointing at a file nobody chose"
    );
}
