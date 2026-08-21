//! The duplicates job (`F-905`, v0.6.0 `P5`): what runs when the duplicates
//! surface opens, and what it hands back to render.
//!
//! # Why this module exists
//!
//! Everything under `dupes` before this was complete and reachable from nothing.
//! Detection was pure, the review model was pure, and the verification job took
//! a list of persisted group ids that no production code path had ever written.
//! `P2` shipped a hash engine the app could not run, which is the same shape as
//! the defect that created `P0`. This module is the missing middle: it turns a
//! `scan_id` into detected groups, persisted rows, hashes, and a review.
//!
//! # The three orderings that matter
//!
//! **Detection is fresh, persistence is a side effect.** Groups are re-detected
//! from the snapshot on every call and the persisted rows exist only so hash
//! state has somewhere to hang (`P4`'s note, 2026-08-16: a hash is a fact about a
//! file, group identity is not). Nothing here reads groups back out of the
//! database to decide what a group is.
//!
//! **Persisting happens before hashing, not at scan time.** `AC-10` forbids a
//! hash-everything path, so rows appear the first time someone actually asks to
//! verify a scan's duplicates. That is also why `insert_duplicate_groups` had to
//! become idempotent first (migration 0009): this runs again on every open.
//!
//! **Files and book folders are hashed by different machinery.** An exact group's
//! members are files and hash directly; a book group's members are folders, which
//! have no bytes of their own, so their audio files are hashed individually and
//! compared as multisets (`AC-54`). Sending a folder path to the file hasher
//! would fail on every book group, which is the bug this split exists to avoid.

use sqlx::SqlitePool;

use crate::db::dupes::{insert_duplicate_groups, member_verifications_for_scan};
use crate::dupes::detect::DuplicateGroup;
use crate::dupes::hash::ContentSource;
use crate::dupes::review::{build_review_with_policy, DuplicatesReview};
use crate::dupes::verify::{verify_book_group, verify_groups, VerifyOutcome};
use crate::dupes::BookFolder;
use crate::error::AppError;
use crate::job::JobContext;

/// One scan's duplicate candidates, detected fresh, with the persisted group id
/// each one was given.
///
/// The id is what the verification job needs (hash state hangs off group rows);
/// the group is what the review needs. Carrying them together is what keeps the
/// two from being looked up separately and drifting apart.
#[derive(Debug, Clone)]
pub struct PersistedDuplicates {
    /// Candidate groups in detection order, paired with their persisted row id.
    pub groups: Vec<(i64, DuplicateGroup)>,
    /// The book-folder view of the same snapshot, needed to hash folder groups.
    pub books: Vec<BookFolder>,
}

fn db_error(e: sqlx::Error) -> AppError {
    AppError::DuplicateVerifyFailed {
        detail: e.to_string(),
    }
}

/// Detect this scan's duplicate candidates and make sure each one has a row.
///
/// Only `AC-52` CANDIDATES are persisted, which is the same set the review shows
/// and the Copies card counts. An exact group that a book group already subsumes
/// is deliberately not persisted and so never hashed: it is the same duplicated
/// book seen one part at a time, and hashing it would spend real disk reads
/// proving something the book group already covers.
///
/// Safe to call repeatedly. The second call reuses every row the first wrote and
/// keeps every hash recorded against it.
pub async fn ensure_duplicate_groups(
    pool: &SqlitePool,
    scan_id: i64,
    now: &str,
) -> Result<PersistedDuplicates, AppError> {
    let (detected, books) = crate::plan::query::detected_duplicates_for_scan(pool, scan_id).await?;
    let candidates: Vec<DuplicateGroup> = detected
        .into_iter()
        .filter(|g| g.is_duplicate_candidate())
        .collect();

    let ids = insert_duplicate_groups(pool, scan_id, &candidates, now)
        .await
        .map_err(db_error)?;

    Ok(PersistedDuplicates {
        groups: ids.into_iter().zip(candidates).collect(),
        books,
    })
}

/// Hash every not-yet-hashed copy in this scan's duplicate candidates
/// (`F-702`, `AC-10`, `AC-11`, `AC-15`).
///
/// Returns one [`VerifyOutcome`] summed across every group, so a caller reports
/// the pass rather than a per-group tally nobody asked for.
///
/// Cancellation is inherited rather than re-implemented: each underlying pass
/// polls between files and persists per file, so a cancelled job keeps the work
/// it did and the next call resumes by simply finding fewer unhashed members.
/// The loop below stops handing out new groups as soon as a pass reports it was
/// cancelled, so a Stop does not quietly roll on into the next book.
pub async fn verify_scan_duplicates<S: ContentSource>(
    pool: &SqlitePool,
    source: &S,
    scan_id: i64,
    now: &str,
    ctx: &JobContext,
) -> Result<VerifyOutcome, AppError> {
    let persisted = ensure_duplicate_groups(pool, scan_id, now).await?;

    // File groups go through the file hasher in ONE call, so its progress total
    // is the whole file workload rather than a per-group number that restarts.
    let file_group_ids: Vec<i64> = persisted
        .groups
        .iter()
        .filter(|(_, g)| g.is_exact())
        .map(|(id, _)| *id)
        .collect();

    let mut out = verify_groups(pool, source, &file_group_ids, ctx).await?;

    let book_group_ids: Vec<i64> = persisted
        .groups
        .iter()
        .filter(|(_, g)| !g.is_exact())
        .map(|(id, _)| *id)
        .collect();

    for group_id in book_group_ids {
        if out.cancelled || ctx.is_cancelled() {
            out.cancelled = true;
            break;
        }
        let book_outcome = verify_book_group(pool, source, group_id, &persisted.books, ctx).await?;
        out.hashed += book_outcome.hashed;
        out.failed += book_outcome.failed;
        out.skipped += book_outcome.skipped;
        out.cancelled |= book_outcome.cancelled;
    }

    Ok(out)
}

/// The review to render: this scan's duplicate groups, each copy carrying
/// whatever is known about its content (`AC-17` to `AC-19`).
///
/// Read-only and filesystem-free. It re-detects rather than reading groups back,
/// and lays the persisted hash state over the result by `entries.id`, so a group
/// whose shape changed since the last hash job still shows the hashes it earned.
/// Nothing here persists, so opening the surface without ever verifying is a
/// cheap, honest "not checked yet" for every copy.
pub async fn review_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
    sep: char,
    policy: crate::dupes::ResolutionPolicy,
) -> Result<DuplicatesReview, AppError> {
    let (groups, books) = crate::plan::query::detected_duplicates_for_scan(pool, scan_id).await?;
    let verifications = member_verifications_for_scan(pool, scan_id)
        .await
        .map_err(db_error)?;
    Ok(build_review_with_policy(
        &groups,
        &books,
        &verifications,
        sep,
        policy,
    ))
}

/// Record a confirmed resolution, refusing it unless `AC-12` is satisfied.
///
/// # The gate lives here, not in the caller
///
/// `AC-12` permits archiving a duplicate group only when every copy carries a
/// matching hash, or when the user supplies an explicit override. Enforcing that
/// in whichever screen happens to call the command would make it a CONVENTION:
/// true for exactly as long as every present and future caller remembers it. The
/// same shape was found in `apply_start`, which takes the run mode as a parameter
/// from whoever calls it, and reclassified as a decision rather than code for
/// exactly this reason. This is the thing standing between the app and a file
/// nobody can get back, so the refusal is its own.
///
/// A group with no persisted row has certainly never been hashed, so the gate is
/// closed for it. That is why a missing row is a plain `false` rather than an
/// error: "not verified" is the honest answer, and it is the same answer the
/// user needs either way.
pub async fn confirm_resolution_gated(
    pool: &SqlitePool,
    scan_id: i64,
    method: &str,
    group_key: &str,
    resolution: &crate::dupes::ConfirmedResolution,
    unverified_override: bool,
    now: &str,
) -> Result<(), AppError> {
    use crate::db::dupes::{confirm_resolution, duplicate_group_id};
    use crate::dupes::verify::group_may_auto_resolve;

    if !unverified_override {
        let verified = match duplicate_group_id(pool, scan_id, method, group_key)
            .await
            .map_err(|e| AppError::DuplicateConfirmFailed {
                detail: format!("could not look up the group: {e}"),
            })? {
            Some(group_id) => group_may_auto_resolve(pool, group_id).await?,
            None => false,
        };
        if !verified {
            return Err(AppError::DuplicateNotVerified {
                group_key: group_key.to_string(),
            });
        }
    }

    confirm_resolution(
        pool,
        scan_id,
        method,
        group_key,
        resolution,
        unverified_override,
        now,
    )
    .await
    .map(|_| ())
    .map_err(|e| AppError::DuplicateConfirmFailed {
        detail: format!("could not record the decision: {e}"),
    })
}

/// The same review, in the shape the surface renders (`F-905`).
///
/// The mapping lives here rather than in TypeScript so the plain-language labels
/// stay inside `dupes::review`, which is the producer the vocabulary sweep
/// governs. A label re-invented on the frontend would be an eighth producer of
/// user-facing text, and three of the existing seven were only added after a
/// retired word had already shipped through them.
pub async fn review_view_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
    sep: char,
    policy: crate::dupes::ResolutionPolicy,
) -> Result<crate::ipc::DuplicatesReviewView, AppError> {
    use crate::db::dupes::confirmations_for_scan;
    use crate::dupes::review::{keeper_reason_label, CopyCheck};
    use crate::ipc::{CopyCheckState, DuplicateCopyView, DuplicateGroupCard, DuplicatesReviewView};

    let review = review_for_scan(pool, scan_id, sep, policy).await?;

    // Confirmations for THIS scan only, which the query guarantees. Keyed by
    // (method, group_key) to match a group's identity rather than by a persisted
    // group id, because a group can be confirmed before anything has persisted a
    // row for it: `AC-12`'s override exists precisely so a group nobody hashed
    // can still be resolved.
    let confirmed: std::collections::HashMap<(String, String), i64> =
        confirmations_for_scan(pool, scan_id)
            .await
            .map_err(db_error)?
            .into_iter()
            .map(|c| ((c.method, c.group_key), c.resolution.keeper as i64))
            .collect();

    Ok(DuplicatesReviewView {
        scan_id,
        group_count: review.group_count() as i64,
        copy_count: review.copy_count() as i64,
        candidate_bytes_estimate: review.candidate_bytes_estimate() as i64,
        groups: review
            .groups
            .iter()
            .map(|g| DuplicateGroupCard {
                book: g.book.clone(),
                group_key: g.group_key.clone(),
                method: g.method.to_string(),
                found_by: g.found_by.to_string(),
                copy_count: g.copy_count() as i64,
                candidate_bytes_estimate: g.candidate_bytes_estimate as i64,
                keeper_reason: g
                    .keeper_reason
                    .map(|r| keeper_reason_label(Some(r)).to_string()),
                content_verified: g.content_verified,
                confirmed_keeper: confirmed
                    .get(&(g.method.to_string(), g.group_key.clone()))
                    .copied(),
                copies: g
                    .copies
                    .iter()
                    .map(|c| DuplicateCopyView {
                        entry_id: c.entry_id as i64,
                        path: c.path.clone(),
                        size_bytes: c.size_bytes as i64,
                        check: match c.check {
                            CopyCheck::NotChecked => CopyCheckState::NotChecked,
                            CopyCheck::Checked => CopyCheckState::Checked,
                            CopyCheck::CouldNotRead(_) => CopyCheckState::CouldNotRead,
                        },
                        check_label: c.check.label().to_string(),
                        check_reason: match &c.check {
                            CopyCheck::CouldNotRead(why) => Some(why.clone()),
                            _ => None,
                        },
                        suggested_keeper: c.suggested_keeper,
                    })
                    .collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::dupes::review::CopyCheck;
    use crate::dupes::{ConfirmedResolution, ResolutionPolicy};
    use std::collections::HashMap;
    use tempfile::TempDir;

    const A: &str = "E:\\Books\\a\\Book.m4b";
    const B: &str = "E:\\Books\\b\\Book.m4b";

    /// Bytes in a map. A local double rather than a shared one, for the same
    /// reason `verify.rs` keeps its own: a change to one must not silently
    /// retune another module's tests.
    struct MemSource(HashMap<String, Vec<u8>>);

    impl ContentSource for MemSource {
        fn read_chunks(
            &self,
            path: &str,
            sink: &mut dyn FnMut(&[u8]),
        ) -> Result<(), std::io::Error> {
            let Some(bytes) = self.0.get(path) else {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, path));
            };
            sink(bytes);
            Ok(())
        }
    }

    /// A snapshot holding the same book twice, as files, so DETECTION (not a
    /// hand-built group) is what these tests run against. That matters: the
    /// point of this module is the path from a scan_id to persisted rows, and a
    /// hand-built group would skip the half most likely to be wrong.
    async fn seed_two_copies(pool: &SqlitePool) -> i64 {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:\\Books', '2026-08-19T00:00:00Z', 'completed')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();

        for (dir, path) in [("a", A), ("b", B)] {
            let dir_id = sqlx::query(
                "INSERT INTO entries (scan_id, parent_id, path, name, kind, size, depth) \
                 VALUES (?, NULL, ?, ?, 'dir', 0, 1)",
            )
            .bind(scan_id)
            .bind(format!("E:\\Books\\{dir}"))
            .bind(dir)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();

            sqlx::query(
                "INSERT INTO entries (scan_id, parent_id, path, name, kind, file_class, size, depth) \
                 VALUES (?, ?, ?, 'Book.m4b', 'file', 'audio', 100, 2)",
            )
            .bind(scan_id)
            .bind(dir_id)
            .bind(path)
            .execute(pool)
            .await
            .unwrap();
        }
        scan_id
    }

    fn source() -> MemSource {
        let mut files = HashMap::new();
        files.insert(A.to_string(), b"identical bytes".to_vec());
        files.insert(B.to_string(), b"identical bytes".to_vec());
        MemSource(files)
    }

    #[tokio::test]
    async fn detection_reaches_persisted_rows_which_is_what_p2_never_had() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;

        let persisted = ensure_duplicate_groups(&pool, scan_id, "2026-08-19T00:00:00Z")
            .await
            .unwrap();

        assert_eq!(persisted.groups.len(), 1, "one duplicated book, one group");
        let (group_id, group) = &persisted.groups[0];
        assert!(*group_id > 0);
        assert_eq!(group.members.len(), 2);
    }

    /// The surface opens more than once. The second open must not write a second
    /// copy of everything, which is what migration 0009 and the insert-or-reuse
    /// fix are for; this is that fix seen from the caller that motivated it.
    #[tokio::test]
    async fn opening_the_surface_twice_persists_one_set_of_groups() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;

        let first = ensure_duplicate_groups(&pool, scan_id, "2026-08-19T00:00:00Z")
            .await
            .unwrap();
        let second = ensure_duplicate_groups(&pool, scan_id, "2026-08-19T01:00:00Z")
            .await
            .unwrap();

        let first_ids: Vec<i64> = first.groups.iter().map(|(id, _)| *id).collect();
        let second_ids: Vec<i64> = second.groups.iter().map(|(id, _)| *id).collect();
        assert_eq!(first_ids, second_ids);
        assert_eq!(
            crate::db::dupes::count_duplicate_groups(&pool, scan_id)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn a_scan_nobody_verified_reports_every_copy_as_not_checked() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;

        let review = review_for_scan(&pool, scan_id, '\\', ResolutionPolicy::FlagOnly)
            .await
            .unwrap();
        assert_eq!(review.group_count(), 1);
        assert!(review.groups[0]
            .copies
            .iter()
            .all(|c| c.check == CopyCheck::NotChecked));
    }

    /// The whole chain in one test, because every link in it was present and
    /// unconnected before this module: detect, persist, hash, read back.
    #[tokio::test]
    async fn verifying_a_scan_puts_real_hashes_into_the_review() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;

        let outcome = verify_scan_duplicates(
            &pool,
            &source(),
            scan_id,
            "2026-08-19T00:00:00Z",
            &JobContext::inert(),
        )
        .await
        .unwrap();

        assert_eq!(outcome.hashed, 2);
        assert_eq!(outcome.failed, 0);
        assert!(!outcome.cancelled);

        let review = review_for_scan(&pool, scan_id, '\\', ResolutionPolicy::FlagOnly)
            .await
            .unwrap();
        assert!(review.groups[0]
            .copies
            .iter()
            .all(|c| c.check == CopyCheck::Checked));
    }

    /// `AC-15`: a re-open must not re-read the disk. The second pass finds the
    /// work already done and reports it as skipped rather than hashing again.
    #[tokio::test]
    async fn verifying_twice_re_reads_nothing() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;

        verify_scan_duplicates(
            &pool,
            &source(),
            scan_id,
            "2026-08-19T00:00:00Z",
            &JobContext::inert(),
        )
        .await
        .unwrap();
        let second = verify_scan_duplicates(
            &pool,
            &source(),
            scan_id,
            "2026-08-19T01:00:00Z",
            &JobContext::inert(),
        )
        .await
        .unwrap();

        assert_eq!(second.hashed, 0, "nothing left to hash");
        assert_eq!(second.skipped, 2, "both copies already carried a hash");
    }

    // -- AC-12's gate ---------------------------------------------------------

    /// The resolution the fixture's two copies support: keep the first.
    async fn keep_the_first(
        pool: &SqlitePool,
        scan_id: i64,
    ) -> (String, String, ConfirmedResolution) {
        let persisted = ensure_duplicate_groups(pool, scan_id, "2026-08-19T00:00:00Z")
            .await
            .unwrap();
        let (_, group) = &persisted.groups[0];
        (
            group.method.to_string(),
            group.group_key.clone(),
            ConfirmedResolution {
                keeper: group.members[0].entry_id,
                losers: vec![group.members[1].entry_id],
            },
        )
    }

    /// The refusal, on the ordinary path: nobody has checked these copies, and no
    /// override was given. Without this the gate would exist only in whichever
    /// screen remembered to apply it.
    #[tokio::test]
    async fn confirming_unchecked_copies_is_refused() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;
        let (method, key, resolution) = keep_the_first(&pool, scan_id).await;

        let refused = confirm_resolution_gated(
            &pool,
            scan_id,
            &method,
            &key,
            &resolution,
            false,
            "2026-08-19T00:00:00Z",
        )
        .await;

        assert!(matches!(
            refused,
            Err(AppError::DuplicateNotVerified { .. })
        ));
        assert!(
            crate::db::dupes::confirmations_for_scan(&pool, scan_id)
                .await
                .unwrap()
                .is_empty(),
            "a refused confirmation must not be half-recorded"
        );
    }

    /// Verified copies need no override: the automatic path is open exactly when
    /// the app can prove the copies are the same book.
    #[tokio::test]
    async fn confirming_checked_copies_is_allowed_without_an_override() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;
        verify_scan_duplicates(
            &pool,
            &source(),
            scan_id,
            "2026-08-19T00:00:00Z",
            &JobContext::inert(),
        )
        .await
        .unwrap();
        let (method, key, resolution) = keep_the_first(&pool, scan_id).await;

        confirm_resolution_gated(
            &pool,
            scan_id,
            &method,
            &key,
            &resolution,
            false,
            "2026-08-19T00:00:00Z",
        )
        .await
        .expect("verified copies need no override");

        let stored = crate::db::dupes::confirmations_for_scan(&pool, scan_id)
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert!(
            !stored[0].unverified_override,
            "this decision was made on evidence, and the record says so"
        );
    }

    /// `AC-13`: the override is what gets past the gate, and the confirmation
    /// REMEMBERS that it was used. Inferring it later from the hashes would be
    /// wrong, because hashes can arrive after the decision.
    #[tokio::test]
    async fn the_override_gets_past_the_gate_and_is_recorded_as_having_been_used() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = seed_two_copies(&pool).await;
        let (method, key, resolution) = keep_the_first(&pool, scan_id).await;

        confirm_resolution_gated(
            &pool,
            scan_id,
            &method,
            &key,
            &resolution,
            true,
            "2026-08-19T00:00:00Z",
        )
        .await
        .expect("the override is the sanctioned way past the gate");

        let stored = crate::db::dupes::confirmations_for_scan(&pool, scan_id)
            .await
            .unwrap();
        assert!(
            stored[0].unverified_override,
            "the record must say this was decided without checking"
        );

        // And hashing afterwards does not rewrite history: the decision was still
        // made without evidence, whatever is known now.
        verify_scan_duplicates(
            &pool,
            &source(),
            scan_id,
            "2026-08-19T01:00:00Z",
            &JobContext::inert(),
        )
        .await
        .unwrap();
        let after = crate::db::dupes::confirmations_for_scan(&pool, scan_id)
            .await
            .unwrap();
        assert!(after[0].unverified_override);
    }
}
