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
/// transaction (all groups commit, or none do). Returns the new group ids, in
/// the same order as `groups`. Each member's `entry_id` must reference a real
/// `entries` row from the same scan (the FK is enforced).
pub async fn insert_duplicate_groups(
    pool: &SqlitePool,
    scan_id: i64,
    groups: &[DuplicateGroup],
    now: &str,
) -> Result<Vec<i64>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut ids = Vec::with_capacity(groups.len());

    for group in groups {
        let group_result = sqlx::query(
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
        let group_id = group_result.last_insert_rowid();

        for member in &group.members {
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

/// Count duplicate GROUPS for a scan (the FD-08 canonical count: groups, never
/// pairs or copies).
pub async fn count_duplicate_groups(pool: &SqlitePool, scan_id: i64) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM duplicate_groups WHERE scan_id = ?")
        .bind(scan_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::dupes::detect::{DuplicateMember, METHOD_EXACT, METHOD_VERSION};
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
}
