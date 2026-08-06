//! F-702 duplicate verification job (v0.6.0 P2, AC-11 and AC-12): hash the
//! members of detected duplicate groups, report progress, stop when asked, and
//! persist what was learned.
//!
//! # Where this sits
//!
//! [`super::detect`] produces candidates. [`super::hash`] turns one candidate
//! member into a fact. This module is the loop between them: it decides what
//! still needs hashing, feeds each file to the hasher, records the outcome, and
//! answers whether a group may be resolved automatically.
//!
//! # Cancellation happens BETWEEN files, never inside one (AC-11)
//!
//! The same rule the executor and the scanner follow. A cancel checked mid-read
//! would abandon a partial hash, and a partial hash is worse than no hash: it is
//! a value that looks like an answer. So the flag is polled at the top of each
//! file and nowhere else, and a cancelled run leaves every member either fully
//! hashed or untouched.
//!
//! # Progress counts FILES, not bytes
//!
//! Bytes would give a smoother bar and a worse number. A user watching this
//! wants to know how many books are left to check, and the file count is the
//! thing that maps onto what they can see in the interface. `total` is known up
//! front here, unlike a first scan, because the candidate set is already
//! detected.

use crate::db::dupes::{get_duplicate_members, set_member_hash, MemberVerification};
use crate::dupes::hash::{hash_member, ContentSource, MemberHash};
use crate::error::AppError;
use crate::job::{JobContext, ProgressUpdate};
use sqlx::SqlitePool;

/// What one verification pass did.
///
/// `cancelled` is separate from the counts on purpose: a run that hashed nine of
/// ten files and was stopped is not the same as one that hashed nine and found
/// the tenth already done, and the surface has to say which.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VerifyOutcome {
    /// Members read and hashed successfully in this pass.
    pub hashed: u64,
    /// Members attempted and failed in this pass. Their reasons are persisted.
    pub failed: u64,
    /// Members skipped because they already carried a hash or a recorded error.
    pub skipped: u64,
    /// True when the pass stopped early because it was asked to.
    pub cancelled: bool,
}

/// Hash every not-yet-hashed member of `group_ids`, persisting each result as it
/// lands.
///
/// Results are written per member rather than batched at the end. A job that
/// dies halfway should keep the work it actually did: re-reading gigabytes
/// because the process was closed is the kind of avoidable cost that makes
/// people stop using a feature.
///
/// Members that previously FAILED are skipped, not retried. See
/// [`get_unhashed_members`](crate::db::dupes::get_unhashed_members) for why.
pub async fn verify_groups<S: ContentSource>(
    pool: &SqlitePool,
    source: &S,
    group_ids: &[i64],
    ctx: &JobContext,
) -> Result<VerifyOutcome, AppError> {
    // Total is known before the first read, so the progress bar is honest from
    // the start rather than growing as it goes.
    let mut out = VerifyOutcome::default();
    let mut pending: Vec<(i64, String)> = Vec::new();
    for &gid in group_ids {
        let members = get_duplicate_members(pool, gid).await.map_err(|e| {
            AppError::DuplicateVerifyFailed {
                detail: e.to_string(),
            }
        })?;
        for m in members {
            match m.verification() {
                MemberVerification::Unhashed => pending.push((m.id, m.path)),
                // Already known, either way. Counted rather than ignored, so a
                // caller can tell "there was nothing to do" from "there was
                // nothing here", which are different things to report.
                MemberVerification::Verified(_) | MemberVerification::Failed(_) => out.skipped += 1,
            }
        }
    }

    let total = pending.len() as u64;

    for (member_id, path) in pending {
        // Poll only here: between files, never inside a read (AC-11).
        if ctx.is_cancelled() {
            out.cancelled = true;
            break;
        }

        let outcome = hash_member(source, &path)?;
        set_member_hash(pool, member_id, &outcome)
            .await
            .map_err(|e| AppError::DuplicateVerifyFailed {
                detail: e.to_string(),
            })?;

        match outcome {
            MemberHash::Hashed(_) => out.hashed += 1,
            MemberHash::Failed(_) => out.failed += 1,
        }

        ctx.report(ProgressUpdate {
            done: out.hashed + out.failed,
            total_estimate: Some(total),
            current_label: path,
        });
    }

    Ok(out)
}

/// Whether a group may be resolved automatically (AC-12).
///
/// True only when the group has at least two members, every one carries a hash,
/// and all the hashes agree.
///
/// Each of those three conditions rejects a different real situation:
///
/// - **Fewer than two members** is not a duplicate group. Resolving it would set
///   aside the only copy of a book.
/// - **Any member unhashed or failed** means the tool does not know. AC-12's
///   whole point is that not-knowing blocks the automatic path, and an
///   unreadable file is the case where it matters most.
/// - **Hashes disagree** means the detector was wrong: same name, same size,
///   different book (AC-14). The answer is to stop, not to pick a winner.
///
/// The user can still override this, deliberately, through the two-step
/// warning-confirm in AC-13. This function is what that override overrides.
pub async fn group_may_auto_resolve(pool: &SqlitePool, group_id: i64) -> Result<bool, AppError> {
    let members = get_duplicate_members(pool, group_id).await.map_err(|e| {
        AppError::DuplicateVerifyFailed {
            detail: e.to_string(),
        }
    })?;

    if members.len() < 2 {
        return Ok(false);
    }

    let mut agreed: Option<String> = None;
    for m in members {
        let MemberVerification::Verified(h) = m.verification() else {
            return Ok(false);
        };
        match &agreed {
            None => agreed = Some(h),
            Some(first) if *first == h => {}
            Some(_) => return Ok(false),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::dupes::{get_duplicate_members, insert_duplicate_groups};
    use crate::db::open_db;
    use crate::dupes::detect::{DuplicateGroup, DuplicateMember, METHOD_EXACT};
    use crate::job::CancelFlag;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    /// Bytes in a map, with per-path failure. Kept separate from the hash
    /// module's double so a change to one cannot silently retune the other.
    struct MemSource {
        files: HashMap<String, Vec<u8>>,
        broken: HashMap<String, String>,
    }

    impl MemSource {
        fn new() -> Self {
            MemSource {
                files: HashMap::new(),
                broken: HashMap::new(),
            }
        }
        fn with(mut self, path: &str, bytes: &[u8]) -> Self {
            self.files.insert(path.to_string(), bytes.to_vec());
            self
        }
        fn broken(mut self, path: &str, why: &str) -> Self {
            self.broken.insert(path.to_string(), why.to_string());
            self
        }
    }

    impl ContentSource for MemSource {
        fn read_chunks(
            &self,
            path: &str,
            sink: &mut dyn FnMut(&[u8]),
        ) -> Result<(), std::io::Error> {
            if let Some(why) = self.broken.get(path) {
                return Err(std::io::Error::other(why.clone()));
            }
            let Some(b) = self.files.get(path) else {
                return Err(std::io::Error::new(std::io::ErrorKind::NotFound, path));
            };
            sink(b);
            Ok(())
        }
    }

    const A: &str = "E:\\Books\\a\\Book.m4b";
    const B: &str = "E:\\Books\\b\\Book.m4b";

    /// A scan, two entries, and one duplicate group over them.
    async fn seed_group(pool: &SqlitePool) -> i64 {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:\\Books', '2026-08-06T00:00:00Z', 'completed')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let mut entry_ids = Vec::new();
        for path in [A, B] {
            let id = sqlx::query(
                "INSERT INTO entries (scan_id, parent_id, path, name, kind, size, depth) \
                 VALUES (?, NULL, ?, 'Book.m4b', 'file', 100, 1)",
            )
            .bind(scan_id)
            .bind(path)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            entry_ids.push(id);
        }

        let groups = vec![DuplicateGroup {
            method: METHOD_EXACT,
            group_key: "Book.m4b|100".to_string(),
            total_bytes: 200,
            members: vec![
                DuplicateMember {
                    entry_id: entry_ids[0] as usize,
                    path: A.to_string(),
                    size: 100,
                },
                DuplicateMember {
                    entry_id: entry_ids[1] as usize,
                    path: B.to_string(),
                    size: 100,
                },
            ],
        }];
        insert_duplicate_groups(pool, scan_id, &groups, "2026-08-06T00:00:00Z")
            .await
            .unwrap()[0]
    }

    #[tokio::test]
    async fn hashes_persist_so_a_second_pass_does_no_work_ac15() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new().with(A, b"identical").with(B, b"identical");

        let first = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(first.hashed, 2);

        // AC-15: the second pass finds nothing to do. Re-reading gigabytes
        // because a screen was reopened is the cost this guards against.
        let second = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(second.hashed, 0, "nothing should be re-hashed");
        assert_eq!(second.skipped, 2, "both were already known");
        assert!(!second.cancelled);
    }

    #[tokio::test]
    async fn a_group_that_agrees_may_auto_resolve_ac12() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new().with(A, b"identical").with(B, b"identical");

        assert!(
            !group_may_auto_resolve(&pool, gid).await.unwrap(),
            "an unhashed group must not auto-resolve"
        );
        verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert!(group_may_auto_resolve(&pool, gid).await.unwrap());
    }

    /// AC-14 through the whole stack: same name, same recorded size, different
    /// bytes. The detector grouped them and the content says otherwise, so the
    /// automatic path must stay shut.
    #[tokio::test]
    async fn a_group_whose_content_disagrees_never_auto_resolves_ac14() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new()
            .with(A, b"read by Simon Vance")
            .with(B, b"read by Scott Brick");

        let out = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(out.hashed, 2, "both read fine; they simply differ");
        assert!(!group_may_auto_resolve(&pool, gid).await.unwrap());
    }

    /// An unreadable member does not fail the job, is recorded with its reason,
    /// and blocks the group (AC-12).
    #[tokio::test]
    async fn an_unreadable_member_is_recorded_and_blocks_the_group() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new()
            .with(A, b"identical")
            .broken(B, "access denied");

        let out = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(out.hashed, 1);
        assert_eq!(out.failed, 1, "the job carried on past the bad file");

        let members = get_duplicate_members(&pool, gid).await.unwrap();
        let failed = members.iter().find(|m| m.path == B).unwrap();
        assert!(
            failed
                .hash_error
                .as_deref()
                .unwrap()
                .contains("access denied"),
            "the reason is persisted, not discarded"
        );
        assert!(!group_may_auto_resolve(&pool, gid).await.unwrap());
    }

    /// A failure is not retried on the next pass. One permission error must not
    /// become an endless re-read every time the surface opens.
    #[tokio::test]
    async fn a_recorded_failure_is_not_retried_automatically() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new()
            .with(A, b"identical")
            .broken(B, "access denied");

        verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        let again = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(again.hashed + again.failed, 0, "nothing retried on its own");
    }

    /// AC-11: cancelling stops the pass BETWEEN files. Whatever was hashed
    /// before the stop is persisted; nothing is left half-hashed.
    #[tokio::test]
    async fn cancelling_stops_between_files_and_keeps_finished_work_ac11() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new().with(A, b"identical").with(B, b"identical");

        // Cancelled before the first file: nothing is read at all.
        let flag = CancelFlag::new();
        flag.cancel();
        let out = verify_groups(&pool, &src, &[gid], &JobContext::with_cancel(flag))
            .await
            .unwrap();
        assert!(out.cancelled);
        assert_eq!(out.hashed, 0);

        let members = get_duplicate_members(&pool, gid).await.unwrap();
        assert!(
            members.iter().all(|m| m.content_hash.is_none()),
            "a cancelled pass leaves no partial hashes"
        );

        // And the work is still there to do afterwards.
        let resumed = verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(resumed.hashed, 2);
    }

    /// Progress counts files, knows its total up front, and never goes
    /// backwards.
    #[tokio::test]
    async fn progress_is_monotonic_and_knows_its_total() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        let src = MemSource::new().with(A, b"identical").with(B, b"identical");

        let seen: Arc<Mutex<Vec<ProgressUpdate>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = {
            let seen = Arc::clone(&seen);
            Arc::new(move |u: ProgressUpdate| seen.lock().unwrap().push(u))
        };
        let ctx = JobContext::new(CancelFlag::new(), sink);

        verify_groups(&pool, &src, &[gid], &ctx).await.unwrap();

        let updates = seen.lock().unwrap();
        assert_eq!(updates.len(), 2);
        assert!(
            updates.iter().all(|u| u.total_estimate == Some(2)),
            "the candidate set is known before the first read, unlike a first scan"
        );
        assert!(
            updates.windows(2).all(|w| w[1].done >= w[0].done),
            "done must never go backwards"
        );
        assert_eq!(updates.last().unwrap().done, 2);
    }

    /// A group of one is not a duplicate group. Auto-resolving it would set
    /// aside the only copy of a book.
    #[tokio::test]
    async fn a_group_with_one_member_never_auto_resolves() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let gid = seed_group(&pool).await;
        sqlx::query("DELETE FROM duplicate_members WHERE path = ?")
            .bind(B)
            .execute(&pool)
            .await
            .unwrap();
        let src = MemSource::new().with(A, b"identical");
        verify_groups(&pool, &src, &[gid], &JobContext::inert())
            .await
            .unwrap();
        assert!(!group_may_auto_resolve(&pool, gid).await.unwrap());
    }
}
