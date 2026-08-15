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

/// Hash the audio files beneath every FOLDER member of a book-level duplicate
/// group (`F-1110` `AC-54`).
///
/// # On request only, and provably so
///
/// `AC-54` says content matching is never part of detection. That is true by
/// construction rather than by discipline: detection is
/// [`crate::dupes::detect`], which is pure, has no [`ContentSource`] in scope,
/// and could not read a byte if it wanted to. This function is the only way a
/// book group's contents are ever read, and something has to call it.
///
/// # Why folder members are hashed one file at a time
///
/// A folder has no hash. Inventing a folder digest would mean choosing a set and
/// an order and defending both forever; instead each audio file is hashed on its
/// own and two folders are compared as multisets of hashes by
/// [`book_group_content_matches`]. That also makes the work resumable at file
/// granularity, which matters when one book is fifty files and several
/// gigabytes.
///
/// Every semantic here is inherited from [`verify_groups`] rather than
/// re-decided: cancel is polled BETWEEN files and never inside a read, results
/// persist per file so a killed job keeps what it did, and a file that already
/// failed is skipped rather than retried on its own.
pub async fn verify_book_group<S: ContentSource>(
    pool: &SqlitePool,
    source: &S,
    group_id: i64,
    books: &[crate::dupes::BookFolder],
    ctx: &JobContext,
) -> Result<VerifyOutcome, AppError> {
    use crate::db::dupes::{
        entry_paths, get_member_files, register_member_files, set_member_file_hash,
    };

    let fail = |e: sqlx::Error| AppError::DuplicateVerifyFailed {
        detail: e.to_string(),
    };

    let mut out = VerifyOutcome::default();
    let members = get_duplicate_members(pool, group_id).await.map_err(fail)?;

    // Register each member's files first, so `total` is known before the first
    // read and the progress bar is honest from the start.
    for m in &members {
        let Some(book) = books.iter().find(|b| b.id as i64 == m.entry_id) else {
            // The member is not a book folder in this snapshot. Nothing to hash
            // and nothing to guess: the group simply cannot reach the content
            // tier, which `book_group_content_matches` will report.
            continue;
        };
        let files = entry_paths(pool, &book.audio_entry_ids)
            .await
            .map_err(fail)?;
        register_member_files(pool, m.id, &files)
            .await
            .map_err(fail)?;
    }

    let mut pending: Vec<(i64, String)> = Vec::new();
    for m in &members {
        for f in get_member_files(pool, m.id).await.map_err(fail)? {
            match f.verification() {
                MemberVerification::Unhashed => pending.push((f.id, f.path)),
                MemberVerification::Verified(_) | MemberVerification::Failed(_) => out.skipped += 1,
            }
        }
    }
    let total = pending.len() as u64;

    for (file_id, path) in pending {
        if ctx.is_cancelled() {
            out.cancelled = true;
            break;
        }
        let outcome = hash_member(source, &path)?;
        set_member_file_hash(pool, file_id, &outcome)
            .await
            .map_err(fail)?;
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

/// Whether every member of a book-level group holds the same audio, by content
/// (`AC-54`).
///
/// Two folders match when their multisets of file hashes are equal. Sorted and
/// compared canonically for the same reason `AC-53` sorts sizes: the order files
/// come back in is not a property of the book.
///
/// False unless there are at least two members, every registered file carries a
/// hash, and no member is empty. A single unreadable file leaves the answer
/// unknown, and unknown is not a match.
///
/// This is deliberately NOT a variant of
/// [`BookMatch`](crate::dupes::BookMatch). That enum is pure and computed at
/// detection time; this answer needs the database and only exists after someone
/// asked for it. Keeping them apart is what stops a pure detector from appearing
/// to know something it cannot.
pub async fn book_group_content_matches(
    pool: &SqlitePool,
    group_id: i64,
) -> Result<bool, AppError> {
    use crate::db::dupes::get_member_files;

    let fail = |e: sqlx::Error| AppError::DuplicateVerifyFailed {
        detail: e.to_string(),
    };

    let members = get_duplicate_members(pool, group_id).await.map_err(fail)?;
    if members.len() < 2 {
        return Ok(false);
    }

    let mut canonical: Option<Vec<String>> = None;
    for m in members {
        let files = get_member_files(pool, m.id).await.map_err(fail)?;
        if files.is_empty() {
            return Ok(false);
        }
        let mut hashes = Vec::with_capacity(files.len());
        for f in files {
            let MemberVerification::Verified(h) = f.verification() else {
                return Ok(false);
            };
            hashes.push(h);
        }
        hashes.sort();
        match &canonical {
            None => canonical = Some(hashes),
            Some(first) if *first == hashes => {}
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

    // ---- F-1110 AC-54: a book-level group whose members are FOLDERS ----

    const DIR_A: &str = "E:\\Books\\a\\Dune";
    const DIR_B: &str = "E:\\Books\\b\\Dune";

    /// A scan holding two copies of a two-part book as FOLDERS, one duplicate
    /// group over the two folders, and the [`BookFolder`] shapes detection would
    /// have produced for them.
    ///
    /// Returns `(group_id, books)`. The parts are deliberately named the same in
    /// both copies, which is the ordinary case and the one that made the
    /// subsumption rule necessary.
    async fn seed_book_group(pool: &SqlitePool) -> (i64, Vec<crate::dupes::BookFolder>) {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:\\Books', '2026-08-14T00:00:00Z', 'completed')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let mut books = Vec::new();
        let mut members = Vec::new();
        for dir in [DIR_A, DIR_B] {
            let dir_id = sqlx::query(
                "INSERT INTO entries (scan_id, parent_id, path, name, kind, size, depth) \
                 VALUES (?, NULL, ?, 'Dune', 'dir', 0, 1)",
            )
            .bind(scan_id)
            .bind(dir)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();

            let mut file_ids = Vec::new();
            for (name, size) in [("Part 01.mp3", 100u64), ("Part 02.mp3", 200u64)] {
                let id = sqlx::query(
                    "INSERT INTO entries (scan_id, parent_id, path, name, kind, file_class, size, depth) \
                     VALUES (?, ?, ?, ?, 'file', 'audio', ?, 2)",
                )
                .bind(scan_id)
                .bind(dir_id)
                .bind(format!("{dir}\\{name}"))
                .bind(name)
                .bind(size as i64)
                .execute(pool)
                .await
                .unwrap()
                .last_insert_rowid();
                file_ids.push(id as usize);
            }

            books.push(crate::dupes::BookFolder {
                id: dir_id as usize,
                path: dir.to_string(),
                title_norm: "dune".to_string(),
                audio_count: 2,
                audio_bytes: 300,
                audio_entry_ids: file_ids,
                audio_sizes: vec![100, 200],
            });
            members.push(DuplicateMember {
                entry_id: dir_id as usize,
                path: dir.to_string(),
                size: 300,
            });
        }

        let groups = vec![DuplicateGroup {
            method: crate::dupes::METHOD_VERSION,
            group_key: "dune".to_string(),
            total_bytes: 600,
            members,
            book_match: Some(crate::dupes::BookMatch::Structural),
            subsumed_by_book_group: false,
        }];
        let gid = insert_duplicate_groups(pool, scan_id, &groups, "2026-08-14T00:00:00Z")
            .await
            .unwrap()[0];
        (gid, books)
    }

    fn two_part_source(a1: &[u8], a2: &[u8], b1: &[u8], b2: &[u8]) -> MemSource {
        MemSource::new()
            .with(&format!("{DIR_A}\\Part 01.mp3"), a1)
            .with(&format!("{DIR_A}\\Part 02.mp3"), a2)
            .with(&format!("{DIR_B}\\Part 01.mp3"), b1)
            .with(&format!("{DIR_B}\\Part 02.mp3"), b2)
    }

    /// AC-54: two copies of a two-part book whose files agree, file for file,
    /// match by content.
    #[tokio::test]
    async fn two_book_copies_with_identical_audio_match_by_content_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(
            b"chapter one",
            b"chapter two",
            b"chapter one",
            b"chapter two",
        );

        assert!(
            !book_group_content_matches(&pool, gid).await.unwrap(),
            "nothing is hashed yet, so nothing is known"
        );

        let out = verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(out.hashed, 4, "two files in each of two copies");
        assert!(book_group_content_matches(&pool, gid).await.unwrap());
    }

    /// The sizes agreed and the bytes do not. Same shape, different recording:
    /// the structural tier said maybe and the content tier says no.
    #[tokio::test]
    async fn same_shape_different_recording_does_not_match_by_content_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(
            b"read by Simon Vance",
            b"chapter two",
            b"read by Scott Brick",
            b"chapter two",
        );

        verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert!(!book_group_content_matches(&pool, gid).await.unwrap());
    }

    /// The files arrive in whatever order the snapshot holds them, so the
    /// comparison is over a SORTED multiset of hashes. Two copies that hold the
    /// same audio under swapped part numbers still match.
    #[tokio::test]
    async fn content_matching_is_canonical_not_positional_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(b"alpha", b"beta", b"beta", b"alpha");

        verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert!(
            book_group_content_matches(&pool, gid).await.unwrap(),
            "the same two files, whichever part number they carry"
        );
    }

    /// One unreadable file leaves the answer unknown, and unknown is never a
    /// match. The failure is recorded with its reason rather than discarded.
    #[tokio::test]
    async fn an_unreadable_file_blocks_a_content_match_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(
            b"chapter one",
            b"chapter two",
            b"chapter one",
            b"chapter two",
        )
        .broken(&format!("{DIR_B}\\Part 02.mp3"), "access denied");

        let out = verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(out.hashed, 3);
        assert_eq!(out.failed, 1, "the job carried on past the bad file");
        assert!(!book_group_content_matches(&pool, gid).await.unwrap());
    }

    /// AC-15's rule one level down: hashes persist per file, so a second pass
    /// over the same group does no work at all.
    #[tokio::test]
    async fn a_second_book_verification_pass_does_no_work_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(b"one", b"two", b"one", b"two");

        assert_eq!(
            verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
                .await
                .unwrap()
                .hashed,
            4
        );
        let again = verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(again.hashed, 0, "nothing re-read");
        assert_eq!(again.skipped, 4, "all four already known");
    }

    /// AC-11's rule one level down: cancelling stops BETWEEN files, keeps what
    /// was finished, and leaves the rest to do.
    #[tokio::test]
    async fn cancelling_a_book_verification_stops_between_files_ac54() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(b"one", b"two", b"one", b"two");

        let flag = CancelFlag::new();
        flag.cancel();
        let out = verify_book_group(&pool, &src, gid, &books, &JobContext::with_cancel(flag))
            .await
            .unwrap();
        assert!(out.cancelled);
        assert_eq!(out.hashed, 0);
        assert!(
            !book_group_content_matches(&pool, gid).await.unwrap(),
            "a cancelled pass leaves nothing that looks like an answer"
        );

        let resumed = verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(resumed.hashed, 4, "the work is still there to do");
    }

    /// AC-52, the guarantee that matters most here: a book-level group stays
    /// candidate-only even when its contents are PROVEN identical.
    ///
    /// It holds by construction rather than by a rule someone remembers. The
    /// AC-12 gate reads `duplicate_members.content_hash`, folder members never
    /// get one, so the gate finds nothing and refuses. Asserted anyway, because
    /// "by construction" is a claim about code that can change.
    #[tokio::test]
    async fn a_content_verified_book_group_still_never_auto_resolves_ac52() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, books) = seed_book_group(&pool).await;
        let src = two_part_source(b"one", b"two", b"one", b"two");

        verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert!(
            book_group_content_matches(&pool, gid).await.unwrap(),
            "the contents really are identical"
        );
        assert!(
            !group_may_auto_resolve(&pool, gid).await.unwrap(),
            "and it STILL may not resolve itself: resolution opens at P3"
        );
    }

    /// A member that is not a book folder in this snapshot contributes no files,
    /// so the group cannot reach the content tier. Nothing is guessed about the
    /// missing side, and the verification job does not fail over it.
    #[tokio::test]
    async fn a_member_with_no_book_shape_cannot_reach_the_content_tier() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (gid, mut books) = seed_book_group(&pool).await;
        books.pop();
        let src = two_part_source(b"one", b"two", b"one", b"two");

        let out = verify_book_group(&pool, &src, gid, &books, &JobContext::inert())
            .await
            .unwrap();
        assert_eq!(out.hashed, 2, "only the side that has a shape");
        assert!(!book_group_content_matches(&pool, gid).await.unwrap());
    }

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
            book_match: None,
            subsumed_by_book_group: false,
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
