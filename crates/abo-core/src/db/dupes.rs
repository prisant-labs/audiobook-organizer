//! F-701 database layer: sqlx CRUD for the `duplicate_groups` and
//! `duplicate_members` tables (migration 0002).
//!
//! Duplicate detection runs over an already-committed scan's entries and writes
//! its candidate groups here. The GROUP is the canonical unit (FD-08): one
//! `duplicate_groups` row per candidate group, one `duplicate_members` row per
//! copy. `method` (exact-basename-size | normalized-title) is the single source
//! of truth for the exact-vs-version label; a report derives the label from it
//! rather than storing a second column (migration 0002's note). This layer is
//! plumbing only: it persists the [`crate::dupes::detect`] output and reads it
//! back; it does not detect, and it emits no plan op (detection is candidate-only
//! this release).

use sqlx::{Row, SqlitePool};

use crate::dupes::detect::DuplicateGroup;

/// One persisted duplicate-group header row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroupRow {
    pub id: i64,
    pub scan_id: i64,
    pub method: String,
    pub group_key: String,
    pub total_bytes: i64,
    pub created_at: String,
}

/// One persisted duplicate-member row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMemberRow {
    pub id: i64,
    pub group_id: i64,
    pub entry_id: i64,
    pub path: String,
    pub size: i64,
    /// BLAKE3 hex of this member's content (F-702, migration 0007), or `None`
    /// when it has never been hashed.
    ///
    /// Read together with [`hash_error`](Self::hash_error): the pair encodes
    /// three states with no separate enum that could disagree with them. Both
    /// `None` means never hashed; a hash means verified; an error means tried
    /// and failed. See [`Self::verification`].
    pub content_hash: Option<String>,
    /// Why hashing this member failed, when it did.
    pub hash_error: Option<String>,
}

/// What is known about one member's content, derived from the stored pair.
///
/// Exists so callers ask a question rather than interpret two `Option`s at every
/// call site. AC-12's gate turns on the difference between `Failed` and
/// `Unhashed`, and a site that got that backwards would let an unreadable file
/// count toward an automatic set-aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberVerification {
    /// Nobody has hashed this member yet. Work not done, not a problem.
    Unhashed,
    /// Read end to end. Carries the hash.
    Verified(String),
    /// Read was attempted and failed. Carries why.
    Failed(String),
}

impl DuplicateMemberRow {
    /// The member's verification state.
    ///
    /// A stored hash wins over a stored error: if both are somehow present, the
    /// file WAS read successfully at some point, and the hash is the more
    /// specific fact. That ordering is stated here rather than left to whichever
    /// branch a reader writes first.
    pub fn verification(&self) -> MemberVerification {
        match (&self.content_hash, &self.hash_error) {
            (Some(h), _) => MemberVerification::Verified(h.clone()),
            (None, Some(e)) => MemberVerification::Failed(e.clone()),
            (None, None) => MemberVerification::Unhashed,
        }
    }
}

/// Persist every detected duplicate group and its members for `scan_id` in one
/// transaction (all groups commit, or none do). Returns the group ids, in the
/// same order as `groups`. Each member's `entry_id` must reference a real
/// `entries` row from the same scan (the FK is enforced).
///
/// # Insert or reuse, never insert twice
///
/// A group this scan already holds is REUSED and its id returned; only genuinely
/// new groups are inserted. `P5` calls this every time the duplicates surface
/// opens, so a blind insert (what this did until migration 0009) would write a
/// second copy of every group and every member on the second open.
///
/// Member rows are ADDED where missing and never deleted or rewritten. That
/// asymmetry is the point: the content hashes live on those rows (migration
/// 0007), so re-inserting them would throw away work that cost a read of the
/// disk, which `AC-15` exists to avoid. A member the detector no longer returns
/// keeps its row and simply stops being asked about, because the read path
/// re-detects groups fresh and looks hashes up by `entries.id`.
pub async fn insert_duplicate_groups(
    pool: &SqlitePool,
    scan_id: i64,
    groups: &[DuplicateGroup],
    now: &str,
) -> Result<Vec<i64>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut ids = Vec::with_capacity(groups.len());

    for group in groups {
        // The unique index on (scan_id, method, group_key) is what makes this
        // lookup total: at most one row can match, so there is no "which one"
        // question to answer here or anywhere downstream.
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM duplicate_groups WHERE scan_id = ? AND method = ? AND group_key = ?",
        )
        .bind(scan_id)
        .bind(group.method)
        .bind(&group.group_key)
        .fetch_optional(&mut *tx)
        .await?;

        let group_id = match existing {
            Some(id) => id,
            None => {
                let inserted = sqlx::query(
                    "INSERT INTO duplicate_groups (scan_id, method, group_key, total_bytes, created_at) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(scan_id)
                .bind(group.method)
                .bind(&group.group_key)
                .bind(group.total_bytes as i64)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                inserted.last_insert_rowid()
            }
        };

        let recorded: Vec<i64> =
            sqlx::query_scalar("SELECT entry_id FROM duplicate_members WHERE group_id = ?")
                .bind(group_id)
                .fetch_all(&mut *tx)
                .await?;

        for member in &group.members {
            if recorded.contains(&(member.entry_id as i64)) {
                continue;
            }
            sqlx::query(
                "INSERT INTO duplicate_members (group_id, entry_id, path, size) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(group_id)
            .bind(member.entry_id as i64)
            .bind(&member.path)
            .bind(member.size as i64)
            .execute(&mut *tx)
            .await?;
        }
        ids.push(group_id);
    }

    tx.commit().await?;
    Ok(ids)
}

/// Fetch every duplicate group for a scan, ordered by id (insertion order).
pub async fn get_duplicate_groups(
    pool: &SqlitePool,
    scan_id: i64,
) -> Result<Vec<DuplicateGroupRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, scan_id, method, group_key, total_bytes, created_at \
         FROM duplicate_groups WHERE scan_id = ? ORDER BY id",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DuplicateGroupRow {
            id: r.get("id"),
            scan_id: r.get("scan_id"),
            method: r.get("method"),
            group_key: r.get("group_key"),
            total_bytes: r.get("total_bytes"),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// Fetch every member of a duplicate group, ordered by id (insertion order,
/// which detection made path-sorted).
pub async fn get_duplicate_members(
    pool: &SqlitePool,
    group_id: i64,
) -> Result<Vec<DuplicateMemberRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, group_id, entry_id, path, size, content_hash, hash_error \
         FROM duplicate_members WHERE group_id = ? ORDER BY id",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| DuplicateMemberRow {
            id: r.get("id"),
            group_id: r.get("group_id"),
            entry_id: r.get("entry_id"),
            path: r.get("path"),
            size: r.get("size"),
            content_hash: r.get("content_hash"),
            hash_error: r.get("hash_error"),
        })
        .collect())
}

/// Every persisted hash state in one scan, keyed by `entries.id` (`F-703`).
///
/// # Why keyed by entry, not by group
///
/// A hash is a fact about a FILE, not about a group, so this deliberately
/// forgets which group each member sat in. That is what makes it safe to lay
/// over FRESH detection: `plan::query` re-detects duplicate groups from the
/// stored snapshot on every read rather than trusting persisted group rows, and
/// group identity can legitimately change between a persist and a read (the
/// `F-1110` subsumption rule did exactly that). Reattaching hashes by file means
/// the current detector's grouping always wins and no group-id reconciliation is
/// needed on the read path at all.
///
/// Scoped to one scan because `entries.id` is only unique within a snapshot.
///
/// Members with no hash and no error are omitted rather than returned as
/// `Unhashed`: absent and unhashed are the same fact, and a caller reading a
/// map treats a missing key as "not done yet" anyway.
pub async fn member_verifications_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
) -> Result<std::collections::HashMap<i64, MemberVerification>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT m.entry_id, m.content_hash, m.hash_error          FROM duplicate_members m          JOIN duplicate_groups g ON g.id = m.group_id          WHERE g.scan_id = ? AND (m.content_hash IS NOT NULL OR m.hash_error IS NOT NULL)          ORDER BY m.id",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await?;

    let mut out = std::collections::HashMap::new();
    for r in rows {
        let hash: Option<String> = r.get("content_hash");
        let error: Option<String> = r.get("hash_error");
        // Same precedence as `DuplicateMemberRow::verification`: a stored hash
        // wins over a stored error, because a hash means the file WAS read.
        let v = match (hash, error) {
            (Some(h), _) => MemberVerification::Verified(h),
            (None, Some(e)) => MemberVerification::Failed(e),
            (None, None) => continue,
        };
        out.insert(r.get::<i64, _>("entry_id"), v);
    }
    Ok(out)
}

/// Record the outcome of hashing ONE duplicate group member (F-702, AC-15).
///
/// Writes exactly one of the two columns and clears the other, so the pair can
/// never carry a stale error beside a fresh hash, or the reverse. A retry that
/// succeeds must leave no trace of the previous failure, because a surface
/// showing both would have to invent a rule for which one wins.
///
/// This is the ONLY statement that writes those columns. Keeping it single is
/// what lets the verification job wrap N of these in one transaction without a
/// second code path drifting from this one.
pub async fn set_member_hash(
    pool: &SqlitePool,
    member_id: i64,
    outcome: &crate::dupes::MemberHash,
) -> Result<(), sqlx::Error> {
    let (hash, error) = match outcome {
        crate::dupes::MemberHash::Hashed(h) => (Some(h.as_str()), None),
        crate::dupes::MemberHash::Failed(e) => (None, Some(e.as_str())),
    };
    sqlx::query("UPDATE duplicate_members SET content_hash = ?, hash_error = ? WHERE id = ?")
        .bind(hash)
        .bind(error)
        .bind(member_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The members of a group that still need hashing (AC-15): never hashed, and
/// not previously failed.
///
/// A previous FAILURE is deliberately not returned. Re-reading a file that just
/// failed, every time the surface opens, turns one permission error into an
/// endless retry loop the user cannot see the cause of. Retrying is a thing the
/// user asks for, not something the job does on its own.
pub async fn get_unhashed_members(
    pool: &SqlitePool,
    group_id: i64,
) -> Result<Vec<DuplicateMemberRow>, sqlx::Error> {
    Ok(get_duplicate_members(pool, group_id)
        .await?
        .into_iter()
        .filter(|m| matches!(m.verification(), MemberVerification::Unhashed))
        .collect())
}

/// One persisted per-file row beneath a FOLDER member of a book-level duplicate
/// group (`F-1110` `AC-54`, migration 0008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberFileRow {
    pub id: i64,
    pub member_id: i64,
    pub entry_id: i64,
    pub path: String,
    pub size: i64,
    pub content_hash: Option<String>,
    pub hash_error: Option<String>,
}

impl MemberFileRow {
    /// What is known about this file's content. Same three states, same encoding
    /// and same precedence as [`DuplicateMemberRow::verification`], so a reader
    /// never has to hold two rules in mind.
    pub fn verification(&self) -> MemberVerification {
        match (&self.content_hash, &self.hash_error) {
            (Some(h), _) => MemberVerification::Verified(h.clone()),
            (None, Some(e)) => MemberVerification::Failed(e.clone()),
            (None, None) => MemberVerification::Unhashed,
        }
    }
}

/// Register the audio files beneath one FOLDER member, so they can be hashed
/// (`AC-54`).
///
/// Idempotent: re-registering the same member leaves any hash already recorded
/// alone. A second verification pass over a group must find its previous work,
/// not start over, which is `AC-15`'s rule applied one level down.
pub async fn register_member_files(
    pool: &SqlitePool,
    member_id: i64,
    files: &[(i64, String, i64)],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for (entry_id, path, size) in files {
        sqlx::query(
            "INSERT OR IGNORE INTO duplicate_member_files (member_id, entry_id, path, size) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(member_id)
        .bind(entry_id)
        .bind(path)
        .bind(size)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Every registered file beneath a member, ordered by id.
pub async fn get_member_files(
    pool: &SqlitePool,
    member_id: i64,
) -> Result<Vec<MemberFileRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, member_id, entry_id, path, size, content_hash, hash_error \
         FROM duplicate_member_files WHERE member_id = ? ORDER BY id",
    )
    .bind(member_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| MemberFileRow {
            id: r.get("id"),
            member_id: r.get("member_id"),
            entry_id: r.get("entry_id"),
            path: r.get("path"),
            size: r.get("size"),
            content_hash: r.get("content_hash"),
            hash_error: r.get("hash_error"),
        })
        .collect())
}

/// Record the outcome of hashing ONE file beneath a folder member (`AC-54`).
///
/// Writes one column and clears the other, exactly as [`set_member_hash`] does
/// and for the same reason: a stale error must never sit beside a fresh hash.
/// Kept as the only statement that writes these two columns.
pub async fn set_member_file_hash(
    pool: &SqlitePool,
    file_id: i64,
    outcome: &crate::dupes::MemberHash,
) -> Result<(), sqlx::Error> {
    let (hash, error) = match outcome {
        crate::dupes::MemberHash::Hashed(h) => (Some(h.as_str()), None),
        crate::dupes::MemberHash::Failed(e) => (None, Some(e.as_str())),
    };
    sqlx::query("UPDATE duplicate_member_files SET content_hash = ?, hash_error = ? WHERE id = ?")
        .bind(hash)
        .bind(error)
        .bind(file_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resolve `(id, path, size)` for snapshot entries by id, for the files a book
/// folder holds.
///
/// Reads paths from `entries` rather than carrying them on the pure
/// [`BookFolder`](crate::dupes::BookFolder), which describes a book's SHAPE. A
/// path is something the database already stores and the pure layer has no use
/// for, so it is fetched at the point of the read rather than threaded through
/// detection.
pub async fn entry_paths(
    pool: &SqlitePool,
    ids: &[usize],
) -> Result<Vec<(i64, String, i64)>, sqlx::Error> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let row = sqlx::query("SELECT id, path, size FROM entries WHERE id = ?")
            .bind(*id as i64)
            .fetch_optional(pool)
            .await?;
        if let Some(r) = row {
            out.push((r.get("id"), r.get("path"), r.get("size")));
        }
    }
    Ok(out)
}

/// Count duplicate GROUPS for a scan (the FD-08 canonical count: groups, never
/// pairs or copies).
pub async fn count_duplicate_groups(pool: &SqlitePool, scan_id: i64) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM duplicate_groups WHERE scan_id = ?")
        .bind(scan_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// One confirmed resolution as stored, with the group identity the surface needs
/// to show which groups are settled.
///
/// The resolution itself is the core's [`ConfirmedResolution`] rather than a
/// second shape of the same idea, so what the plan builder consumes is exactly
/// what came out of the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConfirmation {
    pub method: String,
    pub group_key: String,
    pub confirmed_at: String,
    /// Whether this decision was made without the copies being proven identical
    /// (`AC-13`'s two-step override). A fact about the moment of decision, which
    /// is why it is stored rather than re-derived from the hashes later.
    pub unverified_override: bool,
    pub resolution: crate::dupes::ConfirmedResolution,
}

/// Record the user's confirmed resolution for ONE duplicate group (`AC-24`).
///
/// Replaces any previous confirmation for the same group in the same scan: the
/// unique index makes at most one possible, and two would need a rule for which
/// one wins, which is a coin toss about which files get archived. The delete
/// cascades to the loser rows, so there is no window where new losers sit beside
/// old ones.
///
/// `losers` is stored verbatim, never derived by removing the keeper from a
/// freshly detected member list. What gets archived must be exactly what the
/// user confirmed.
///
/// All in one transaction: a confirmation with no losers, or with half of them,
/// is not a smaller confirmation, it is a different and wrong one.
pub async fn confirm_resolution(
    pool: &SqlitePool,
    scan_id: i64,
    method: &str,
    group_key: &str,
    resolution: &crate::dupes::ConfirmedResolution,
    unverified_override: bool,
    now: &str,
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM duplicate_confirmations WHERE scan_id = ? AND method = ? AND group_key = ?",
    )
    .bind(scan_id)
    .bind(method)
    .bind(group_key)
    .execute(&mut *tx)
    .await?;

    let inserted = sqlx::query(
        "INSERT INTO duplicate_confirmations \
         (scan_id, method, group_key, keeper_entry_id, unverified_override, confirmed_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(scan_id)
    .bind(method)
    .bind(group_key)
    .bind(resolution.keeper as i64)
    .bind(unverified_override)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let confirmation_id = inserted.last_insert_rowid();

    for loser in &resolution.losers {
        sqlx::query(
            "INSERT INTO duplicate_confirmation_losers (confirmation_id, entry_id) VALUES (?, ?)",
        )
        .bind(confirmation_id)
        .bind(*loser as i64)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(confirmation_id)
}

/// Every confirmation recorded against `scan_id`, in confirmation order.
///
/// # The `scan_id` filter is a safety mechanism, not a convenience
///
/// `entries.id` is unique only within one snapshot, and `FD-39` re-plans from a
/// FRESH scan after an interruption. A confirmation read back against a
/// different scan would name whatever files happen to hold those ids now, and
/// the plan builder would archive them. Filtering here makes a stale
/// confirmation UNREACHABLE rather than merely unlikely: there is no code path
/// that returns one, so no caller can forget to check.
pub async fn confirmations_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
) -> Result<Vec<StoredConfirmation>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, method, group_key, keeper_entry_id, unverified_override, confirmed_at \
         FROM duplicate_confirmations WHERE scan_id = ? ORDER BY id",
    )
    .bind(scan_id)
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.get("id");
        let losers: Vec<i64> = sqlx::query_scalar(
            "SELECT entry_id FROM duplicate_confirmation_losers WHERE confirmation_id = ? ORDER BY id",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        out.push(StoredConfirmation {
            method: r.get("method"),
            group_key: r.get("group_key"),
            confirmed_at: r.get("confirmed_at"),
            unverified_override: r.get::<i64, _>("unverified_override") != 0,
            resolution: crate::dupes::ConfirmedResolution {
                keeper: r.get::<i64, _>("keeper_entry_id") as usize,
                losers: losers.into_iter().map(|l| l as usize).collect(),
            },
        });
    }
    Ok(out)
}

/// The persisted group id for one detected group, when it has one.
///
/// `None` means nothing has ever persisted this group, which happens whenever
/// the verification job has not run for this scan. That is a meaningful answer
/// rather than an error: a group with no row has certainly not been hashed, so
/// `AC-12`'s gate is closed for it.
///
/// Total by construction: the unique index on (scan_id, method, group_key) means
/// at most one row can match.
pub async fn duplicate_group_id(
    pool: &SqlitePool,
    scan_id: i64,
    method: &str,
    group_key: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM duplicate_groups WHERE scan_id = ? AND method = ? AND group_key = ?",
    )
    .bind(scan_id)
    .bind(method)
    .bind(group_key)
    .fetch_optional(pool)
    .await
}

/// Withdraw a confirmation. The loser rows go with it (the FK cascades), because
/// a confirmation without its losers is not a record of anything.
pub async fn clear_confirmation(
    pool: &SqlitePool,
    scan_id: i64,
    method: &str,
    group_key: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM duplicate_confirmations WHERE scan_id = ? AND method = ? AND group_key = ?",
    )
    .bind(scan_id)
    .bind(method)
    .bind(group_key)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::dupes::detect::{DuplicateMember, METHOD_EXACT, METHOD_VERSION};
    use crate::dupes::ConfirmedResolution;
    use tempfile::TempDir;

    /// Insert a scan plus two entries and return (scan_id, [entry_id, ...]) so
    /// member rows have real FKs to reference.
    async fn scan_with_entries(pool: &SqlitePool) -> (i64, Vec<i64>) {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:\\Books', '2026-07-04T00:00:00Z', 'completed')",
        )
        .execute(pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let mut entry_ids = Vec::new();
        for (path, name) in [
            ("E:\\Books\\a\\Book.m4b", "Book.m4b"),
            ("E:\\Books\\b\\Book.m4b", "Book.m4b"),
        ] {
            let id = sqlx::query(
                "INSERT INTO entries (scan_id, parent_id, path, name, kind, size, depth) \
                 VALUES (?, NULL, ?, ?, 'file', 100, 1)",
            )
            .bind(scan_id)
            .bind(path)
            .bind(name)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
            entry_ids.push(id);
        }
        (scan_id, entry_ids)
    }

    #[tokio::test]
    async fn insert_and_read_back_duplicate_group_with_members() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;

        let groups = vec![DuplicateGroup {
            method: METHOD_EXACT,
            group_key: "Book.m4b|100".to_string(),
            total_bytes: 200,
            members: vec![
                DuplicateMember {
                    entry_id: entry_ids[0] as usize,
                    path: "E:\\Books\\a\\Book.m4b".to_string(),
                    size: 100,
                },
                DuplicateMember {
                    entry_id: entry_ids[1] as usize,
                    path: "E:\\Books\\b\\Book.m4b".to_string(),
                    size: 100,
                },
            ],
            book_match: None,
            subsumed_by_book_group: false,
        }];

        let ids = insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-04T00:00:00Z")
            .await
            .unwrap();
        assert_eq!(ids.len(), 1);

        let stored = get_duplicate_groups(&pool, scan_id).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].method, METHOD_EXACT);
        assert_eq!(stored[0].group_key, "Book.m4b|100");
        assert_eq!(stored[0].total_bytes, 200);

        let members = get_duplicate_members(&pool, stored[0].id).await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].entry_id, entry_ids[0]);
        assert_eq!(members[1].path, "E:\\Books\\b\\Book.m4b");

        assert_eq!(count_duplicate_groups(&pool, scan_id).await.unwrap(), 1);
    }

    /// The `method` column round-trips both families, so a reader can derive the
    /// exact-vs-version label from it alone (migration 0002's design).
    #[tokio::test]
    async fn method_column_distinguishes_exact_from_version() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;

        let groups = vec![
            DuplicateGroup {
                method: METHOD_EXACT,
                group_key: "Book.m4b|100".to_string(),
                total_bytes: 200,
                members: vec![DuplicateMember {
                    entry_id: entry_ids[0] as usize,
                    path: "E:\\Books\\a\\Book.m4b".to_string(),
                    size: 100,
                }],
                book_match: None,
                subsumed_by_book_group: false,
            },
            DuplicateGroup {
                method: METHOD_VERSION,
                group_key: "book title".to_string(),
                total_bytes: 200,
                members: vec![DuplicateMember {
                    entry_id: entry_ids[1] as usize,
                    path: "E:\\Books\\b\\Book.m4b".to_string(),
                    size: 100,
                }],
                book_match: Some(crate::dupes::BookMatch::TitleOnly),
                subsumed_by_book_group: false,
            },
        ];
        insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-04T00:00:00Z")
            .await
            .unwrap();

        let stored = get_duplicate_groups(&pool, scan_id).await.unwrap();
        let methods: Vec<&str> = stored.iter().map(|g| g.method.as_str()).collect();
        assert!(methods.contains(&METHOD_EXACT));
        assert!(methods.contains(&METHOD_VERSION));
    }

    /// One group, built the way both idempotence tests need it.
    fn one_exact_group(entry_ids: &[i64]) -> Vec<DuplicateGroup> {
        vec![DuplicateGroup {
            method: METHOD_EXACT,
            group_key: "Book.m4b|100".to_string(),
            total_bytes: 200,
            members: entry_ids
                .iter()
                .enumerate()
                .map(|(i, id)| DuplicateMember {
                    entry_id: *id as usize,
                    path: format!("E:\\Books\\{}\\Book.m4b", if i == 0 { "a" } else { "b" }),
                    size: 100,
                })
                .collect(),
            book_match: None,
            subsumed_by_book_group: false,
        }]
    }

    /// P5's write path runs the insert every time the duplicates surface opens,
    /// so running it twice has to be a no-op rather than a second copy of
    /// everything. Before the insert-or-reuse fix this doubled the rows.
    #[tokio::test]
    async fn inserting_the_same_groups_twice_reuses_them_rather_than_duplicating() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;
        let groups = one_exact_group(&entry_ids);

        let first = insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-04T00:00:00Z")
            .await
            .unwrap();
        let second = insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-05T00:00:00Z")
            .await
            .unwrap();

        assert_eq!(
            first, second,
            "the second call must reuse the same group ids"
        );
        assert_eq!(get_duplicate_groups(&pool, scan_id).await.unwrap().len(), 1);
        assert_eq!(
            get_duplicate_members(&pool, first[0]).await.unwrap().len(),
            2
        );
    }

    /// The reason reuse must not delete and reinsert members: the hash state
    /// hangs off the member rows, and re-opening the surface must never throw
    /// away work that cost a read of the disk (`AC-15`).
    #[tokio::test]
    async fn reusing_a_group_preserves_hashes_already_recorded_against_its_members() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;
        let groups = one_exact_group(&entry_ids);

        let ids = insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-04T00:00:00Z")
            .await
            .unwrap();
        let members = get_duplicate_members(&pool, ids[0]).await.unwrap();
        set_member_hash(
            &pool,
            members[0].id,
            &crate::dupes::MemberHash::Hashed("abc123".to_string()),
        )
        .await
        .unwrap();

        insert_duplicate_groups(&pool, scan_id, &groups, "2026-07-05T00:00:00Z")
            .await
            .unwrap();

        let after = member_verifications_for_scan(&pool, scan_id).await.unwrap();
        assert_eq!(
            after.get(&entry_ids[0]),
            Some(&MemberVerification::Verified("abc123".to_string())),
            "a hash recorded before the second insert must survive it"
        );
    }

    /// Two scans can hold the same group key without colliding: the uniqueness is
    /// per snapshot, because that is the scope `entries.id` is meaningful in.
    #[tokio::test]
    async fn the_same_group_key_in_two_scans_is_two_groups() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_a, entries_a) = scan_with_entries(&pool).await;
        let (scan_b, entries_b) = scan_with_entries(&pool).await;

        let a = insert_duplicate_groups(
            &pool,
            scan_a,
            &one_exact_group(&entries_a),
            "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();
        let b = insert_duplicate_groups(
            &pool,
            scan_b,
            &one_exact_group(&entries_b),
            "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();

        assert_ne!(a, b);
        assert_eq!(get_duplicate_groups(&pool, scan_a).await.unwrap().len(), 1);
        assert_eq!(get_duplicate_groups(&pool, scan_b).await.unwrap().len(), 1);
    }

    // -- confirmations (AC-24) ------------------------------------------------

    #[tokio::test]
    async fn a_confirmation_round_trips_with_its_keeper_and_every_loser() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;

        let resolution = ConfirmedResolution {
            keeper: entry_ids[0] as usize,
            losers: vec![entry_ids[1] as usize],
        };
        confirm_resolution(
            &pool,
            scan_id,
            METHOD_EXACT,
            "Book.m4b|100",
            &resolution,
            false,
            "2026-08-19T00:00:00Z",
        )
        .await
        .unwrap();

        let stored = confirmations_for_scan(&pool, scan_id).await.unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].method, METHOD_EXACT);
        assert_eq!(stored[0].group_key, "Book.m4b|100");
        assert_eq!(stored[0].resolution, resolution);
    }

    /// Changing your mind must REPLACE the previous answer. Two confirmations for
    /// one group would need a rule for which one wins, and that rule decides
    /// which files get archived.
    #[tokio::test]
    async fn re_confirming_a_group_replaces_the_previous_answer_rather_than_adding_one() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;

        for (keeper, loser) in [(0, 1), (1, 0)] {
            confirm_resolution(
                &pool,
                scan_id,
                METHOD_EXACT,
                "Book.m4b|100",
                &ConfirmedResolution {
                    keeper: entry_ids[keeper] as usize,
                    losers: vec![entry_ids[loser] as usize],
                },
                false,
                "2026-08-19T00:00:00Z",
            )
            .await
            .unwrap();
        }

        let stored = confirmations_for_scan(&pool, scan_id).await.unwrap();
        assert_eq!(stored.len(), 1, "one group, one confirmation");
        assert_eq!(stored[0].resolution.keeper, entry_ids[1] as usize);
        assert_eq!(stored[0].resolution.losers, vec![entry_ids[0] as usize]);

        // The replaced answer's losers went with it rather than lingering.
        let orphans: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM duplicate_confirmation_losers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(orphans, 1);
    }

    /// THE INVARIANT THIS WHOLE SCHEME EXISTS FOR. `entries.id` is per snapshot
    /// and `FD-39` re-plans from a fresh scan, so a confirmation read back
    /// against a different scan would name whatever files hold those ids now.
    /// Archiving those is the worst failure this product can have.
    #[tokio::test]
    async fn a_confirmation_made_against_one_scan_is_invisible_to_another() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_a, entries_a) = scan_with_entries(&pool).await;
        let (scan_b, _entries_b) = scan_with_entries(&pool).await;

        confirm_resolution(
            &pool,
            scan_a,
            METHOD_EXACT,
            "Book.m4b|100",
            &ConfirmedResolution {
                keeper: entries_a[0] as usize,
                losers: vec![entries_a[1] as usize],
            },
            false,
            "2026-08-19T00:00:00Z",
        )
        .await
        .unwrap();

        assert_eq!(
            confirmations_for_scan(&pool, scan_a).await.unwrap().len(),
            1
        );
        assert!(
            confirmations_for_scan(&pool, scan_b)
                .await
                .unwrap()
                .is_empty(),
            "a re-scan must start with no confirmations, not inherit them"
        );
    }

    #[tokio::test]
    async fn clearing_a_confirmation_takes_its_losers_with_it() {
        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let (scan_id, entry_ids) = scan_with_entries(&pool).await;

        confirm_resolution(
            &pool,
            scan_id,
            METHOD_EXACT,
            "Book.m4b|100",
            &ConfirmedResolution {
                keeper: entry_ids[0] as usize,
                losers: vec![entry_ids[1] as usize],
            },
            false,
            "2026-08-19T00:00:00Z",
        )
        .await
        .unwrap();
        clear_confirmation(&pool, scan_id, METHOD_EXACT, "Book.m4b|100")
            .await
            .unwrap();

        assert!(confirmations_for_scan(&pool, scan_id)
            .await
            .unwrap()
            .is_empty());
        let losers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM duplicate_confirmation_losers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(losers, 0, "the cascade removed them");
    }
}
