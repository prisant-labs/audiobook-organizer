//! F-801 (ruleset model and persistence) database layer: sqlx CRUD for the
//! `rulesets` table (migration 0002; `is_active` added by migration 0004,
//! v0.4.0 Phase 6, F-906).
//!
//! v0.3.0 Phase 1 (this module) is plumbing only: it stores and reads
//! whatever `body_json` it is given, verbatim. Schema validation of that
//! JSON body against a versioned shape (AC-29: reject a body that fails
//! validation, on save, with a machine code) is `ruleset.rs`'s job (v0.3.0
//! Phase 3, not yet landed) - this layer never parses or validates
//! `body_json`, so a later phase can layer real validation on top without
//! touching the storage contract. The default ruleset (AC-30:
//! `abs-author-first` plus pack-shell-to-quarantine) is also Phase 3's job to
//! seed; this module supplies the plain INSERT it will call.
//!
//! Like `db::activity`, every query uses the sqlx RUNTIME API
//! (`sqlx::query`, binding at runtime): no compile-time `query!` macros, so
//! the build never needs `DATABASE_URL`.
//!
//! # The active ruleset (v0.4.0 Phase 6, F-906)
//!
//! `plan_generate` always builds against whichever row carries `is_active =
//! 1` (there is at most one at a time, see [`set_active_ruleset`]); saving a
//! ruleset in the editor makes it the active one ("Apply/save semantics per
//! the spec: a saved change persists the active ruleset"). [`delete_ruleset`]
//! itself has no opinion on "in use" - the command layer (`ruleset_delete`)
//! is where that guard belongs, since it needs to compare against what is
//! currently active, a policy decision rather than plain CRUD.

use sqlx::{Row, SqlitePool};

/// One persisted ruleset row, read back verbatim from `rulesets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesetRow {
    pub id: i64,
    pub name: String,
    pub body_json: String,
    pub schema_version: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Whether this is the one ruleset `plan_generate` currently builds
    /// against (F-906). At most one row carries `true` at a time.
    pub is_active: bool,
}

/// A ruleset to insert: everything but the id and timestamps, which this
/// module assigns (id via autoincrement, timestamps via the caller-supplied
/// `now`, kept as a parameter rather than read from the clock here so the
/// core stays free of a wall-clock/chrono dependency, matching the rest of
/// the crate's timestamp convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRuleset<'a> {
    pub name: &'a str,
    pub body_json: &'a str,
    pub schema_version: i64,
}

/// Insert a new ruleset row and return its assigned id.
///
/// Does not validate `body_json` (see module doc); a caller wanting AC-29
/// validate-before-save behavior must validate before calling this.
pub async fn insert_ruleset(
    pool: &SqlitePool,
    new: &NewRuleset<'_>,
    now: &str,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO rulesets (name, body_json, schema_version, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(new.name)
    .bind(new.body_json)
    .bind(new.schema_version)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Fetch one ruleset by id, or `None` if it does not exist.
pub async fn get_ruleset(pool: &SqlitePool, id: i64) -> Result<Option<RulesetRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, body_json, schema_version, created_at, updated_at, is_active \
         FROM rulesets WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_ruleset))
}

/// List every ruleset, oldest first (insertion order via id).
pub async fn list_rulesets(pool: &SqlitePool) -> Result<Vec<RulesetRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, body_json, schema_version, created_at, updated_at, is_active \
         FROM rulesets ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_ruleset).collect())
}

/// How many rulesets exist. Used by the command layer to decide whether a
/// delete would remove the user's only ruleset (F-906).
pub async fn count_rulesets(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM rulesets")
        .fetch_one(pool)
        .await
}

/// Atomically seed a single active `Default` ruleset IF the table is empty,
/// returning how many rows were inserted (1 when this call seeded, 0 when a
/// row already existed - including one a concurrent caller just seeded).
///
/// The `INSERT ... SELECT ... WHERE NOT EXISTS` is ONE statement, so SQLite
/// evaluates the emptiness check and performs the insert under the same write
/// lock: two concurrent first callers can never both insert, so a brand-new
/// database always ends with exactly one seeded row. This closes the F-906
/// bootstrap race the earlier check-then-insert left open (P6 review): a
/// plain "is it empty? no -> insert" pair could interleave so both callers
/// saw an empty table and both inserted a `Default` row. The seeded row is
/// marked active in the same statement (there is no prior active row to clear,
/// precisely because the table was empty), preserving the at-most-one-active
/// invariant without a second UPDATE.
pub async fn seed_default_active_ruleset(
    pool: &SqlitePool,
    name: &str,
    body_json: &str,
    schema_version: i64,
    now: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO rulesets (name, body_json, schema_version, created_at, updated_at, is_active) \
         SELECT ?, ?, ?, ?, ?, 1 WHERE NOT EXISTS (SELECT 1 FROM rulesets)",
    )
    .bind(name)
    .bind(body_json)
    .bind(schema_version)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Fetch the one ACTIVE ruleset (`is_active = 1`), or `None` if no ruleset has
/// ever been activated yet (a brand-new database before the first seed/save).
/// At most one row is ever active (see [`set_active_ruleset`]), so `LIMIT 1`
/// is a defensive cap, not a real ambiguity.
pub async fn get_active_ruleset(pool: &SqlitePool) -> Result<Option<RulesetRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, name, body_json, schema_version, created_at, updated_at, is_active \
         FROM rulesets WHERE is_active = 1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_ruleset))
}

/// Make `id` the one active ruleset (F-906 "Apply/save semantics": a saved
/// change persists the active ruleset), clearing any previously-active row
/// first. Runs inside a transaction so the "at most one active row"
/// invariant never observes two active rows (or zero, once any ruleset has
/// ever been activated) between the two statements.
pub async fn set_active_ruleset(pool: &SqlitePool, id: i64, now: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE rulesets SET is_active = 0 WHERE is_active = 1")
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE rulesets SET is_active = 1, updated_at = ? WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Overwrite an existing ruleset's name and body in place (a named ruleset
/// being edited, as opposed to plan regeneration which always creates a new
/// `plans` row). Does not validate `body_json`; see module doc. Bumps
/// `updated_at` to `now`; `created_at` is untouched.
pub async fn update_ruleset_body(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    body_json: &str,
    schema_version: i64,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE rulesets SET name = ?, body_json = ?, schema_version = ?, updated_at = ? \
         WHERE id = ?",
    )
    .bind(name)
    .bind(body_json)
    .bind(schema_version)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a ruleset by id. Plain plumbing: whether `id` is currently the
/// active ruleset (or the user's only one) is a policy question the command
/// layer answers BEFORE calling this (see the module doc); deleting an
/// unknown id is a silent no-op, matching SQL `DELETE`'s own semantics.
pub async fn delete_ruleset(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM rulesets WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn row_to_ruleset(row: sqlx::sqlite::SqliteRow) -> RulesetRow {
    RulesetRow {
        id: row.get("id"),
        name: row.get("name"),
        body_json: row.get("body_json"),
        schema_version: row.get("schema_version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        is_active: row.get::<i64, _>("is_active") != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use tempfile::TempDir;

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    /// A ruleset inserted and read back by id round-trips every field
    /// exactly, including `body_json` verbatim (no reformatting).
    #[tokio::test]
    async fn insert_and_get_round_trips() {
        let (_db, pool) = fresh_pool().await;

        let body = r#"{"schema_version":1,"template":"abs-author-first"}"#;
        let id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Default",
                body_json: body,
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_ruleset");

        let fetched = get_ruleset(&pool, id)
            .await
            .expect("get_ruleset")
            .expect("ruleset must exist");
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.name, "Default");
        assert_eq!(fetched.body_json, body);
        assert_eq!(fetched.schema_version, 1);
        assert_eq!(fetched.created_at, "2026-07-04T00:00:00Z");
        assert_eq!(fetched.updated_at, "2026-07-04T00:00:00Z");
        assert!(
            !fetched.is_active,
            "a freshly inserted ruleset starts inactive"
        );
    }

    /// A missing id returns `None`, not an error.
    #[tokio::test]
    async fn get_missing_ruleset_returns_none() {
        let (_db, pool) = fresh_pool().await;
        let fetched = get_ruleset(&pool, 999).await.expect("get_ruleset");
        assert_eq!(fetched, None);
    }

    /// Two rulesets list back in insertion order.
    #[tokio::test]
    async fn list_rulesets_returns_insertion_order() {
        let (_db, pool) = fresh_pool().await;

        let first = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "First",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert first");
        let second = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Second",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:01Z",
        )
        .await
        .expect("insert second");

        let all = list_rulesets(&pool).await.expect("list_rulesets");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, first);
        assert_eq!(all[1].id, second);
    }

    /// Updating a ruleset's name and body changes `name`/`body_json`/
    /// `schema_version`/`updated_at` but leaves `created_at` untouched, and
    /// never inserts a second row (distinct from plan regeneration's
    /// always-insert contract: a ruleset is an editable named row, not an
    /// immutable plan header).
    #[tokio::test]
    async fn update_ruleset_body_overwrites_in_place() {
        let (_db, pool) = fresh_pool().await;

        let id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Default",
                body_json: r#"{"v":1}"#,
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_ruleset");

        update_ruleset_body(
            &pool,
            id,
            "Renamed",
            r#"{"v":2}"#,
            2,
            "2026-07-04T01:00:00Z",
        )
        .await
        .expect("update_ruleset_body");

        let all = list_rulesets(&pool).await.expect("list_rulesets");
        assert_eq!(all.len(), 1, "update must not insert a second row");

        let fetched = get_ruleset(&pool, id)
            .await
            .expect("get_ruleset")
            .expect("ruleset must exist");
        assert_eq!(fetched.name, "Renamed");
        assert_eq!(fetched.body_json, r#"{"v":2}"#);
        assert_eq!(fetched.schema_version, 2);
        assert_eq!(fetched.created_at, "2026-07-04T00:00:00Z");
        assert_eq!(fetched.updated_at, "2026-07-04T01:00:00Z");
    }

    // ---- F-906 (v0.4.0 Phase 6): active ruleset + delete + count. --------

    /// A brand-new database has no active ruleset yet.
    #[tokio::test]
    async fn fresh_db_has_no_active_ruleset() {
        let (_db, pool) = fresh_pool().await;
        assert_eq!(get_active_ruleset(&pool).await.expect("query"), None);
    }

    /// Activating a ruleset marks it active and clears any previously-active
    /// row, so at most one row is ever active (AC-32/AC-33 "the active
    /// ruleset" the editor saves against).
    #[tokio::test]
    async fn set_active_ruleset_clears_the_previous_one() {
        let (_db, pool) = fresh_pool().await;
        let first = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "First",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert first");
        let second = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Second",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:01Z",
        )
        .await
        .expect("insert second");

        set_active_ruleset(&pool, first, "2026-07-04T00:00:02Z")
            .await
            .expect("activate first");
        assert_eq!(
            get_active_ruleset(&pool)
                .await
                .expect("query")
                .map(|r| r.id),
            Some(first)
        );

        set_active_ruleset(&pool, second, "2026-07-04T00:00:03Z")
            .await
            .expect("activate second");
        let active = get_active_ruleset(&pool)
            .await
            .expect("query")
            .expect("one active row");
        assert_eq!(
            active.id, second,
            "activating the second must clear the first"
        );

        let all = list_rulesets(&pool).await.expect("list_rulesets");
        let active_count = all.iter().filter(|r| r.is_active).count();
        assert_eq!(active_count, 1, "exactly one row is ever active");
    }

    /// `delete_ruleset` removes the row; deleting an unknown id is a no-op,
    /// not an error (matching SQL DELETE's own semantics; the command layer
    /// is responsible for refusing to delete the active/only ruleset).
    #[tokio::test]
    async fn delete_ruleset_removes_the_row_and_tolerates_unknown_ids() {
        let (_db, pool) = fresh_pool().await;
        let id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Temp",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert");

        delete_ruleset(&pool, id).await.expect("delete");
        assert_eq!(get_ruleset(&pool, id).await.expect("query"), None);
        assert_eq!(count_rulesets(&pool).await.expect("count"), 0);

        // Deleting again (already gone) is a tolerated no-op.
        delete_ruleset(&pool, id).await.expect("delete again");
        // Deleting an id that never existed is likewise a tolerated no-op.
        delete_ruleset(&pool, 999).await.expect("delete unknown");
    }

    /// The atomic seed inserts exactly one active `Default` row on an empty
    /// table, and is a no-op once any row exists.
    #[tokio::test]
    async fn seed_default_active_ruleset_seeds_once_then_no_ops() {
        let (_db, pool) = fresh_pool().await;

        let inserted = seed_default_active_ruleset(&pool, "Default", "{}", 1, "2026-07-06T00:00:00Z")
            .await
            .expect("seed");
        assert_eq!(inserted, 1, "the first seed inserts one row");
        let active = get_active_ruleset(&pool)
            .await
            .expect("query")
            .expect("one active row");
        assert_eq!(active.name, "Default");
        assert!(active.is_active);

        let again = seed_default_active_ruleset(&pool, "Default", "{}", 1, "2026-07-06T00:00:01Z")
            .await
            .expect("seed again");
        assert_eq!(again, 0, "a second seed on a non-empty table inserts nothing");
        assert_eq!(count_rulesets(&pool).await.expect("count"), 1);
    }

    /// The F-906 bootstrap race (P6 review): two concurrent first callers seed
    /// exactly ONE `Default` active row, never two. The atomic
    /// `INSERT ... WHERE NOT EXISTS` serializes under SQLite's write lock, so
    /// the second caller's emptiness check is false and it inserts nothing.
    #[tokio::test]
    async fn concurrent_seed_produces_exactly_one_row() {
        let (_db, pool) = fresh_pool().await;
        let p1 = pool.clone();
        let p2 = pool.clone();
        let now = "2026-07-06T00:00:00Z";
        let (a, b) = tokio::join!(
            async move { seed_default_active_ruleset(&p1, "Default", "{}", 1, now).await },
            async move { seed_default_active_ruleset(&p2, "Default", "{}", 1, now).await },
        );
        let inserted_a = a.expect("seed a");
        let inserted_b = b.expect("seed b");
        assert_eq!(
            inserted_a + inserted_b,
            1,
            "exactly one of the two concurrent seeds inserts a row"
        );
        assert_eq!(
            count_rulesets(&pool).await.expect("count"),
            1,
            "the table holds exactly one row after concurrent bootstrap"
        );
        let all = list_rulesets(&pool).await.expect("list");
        assert_eq!(
            all.iter().filter(|r| r.is_active).count(),
            1,
            "exactly one row is active"
        );
        assert_eq!(all[0].name, "Default");
    }

    /// `count_rulesets` tracks inserts and deletes.
    #[tokio::test]
    async fn count_rulesets_tracks_inserts() {
        let (_db, pool) = fresh_pool().await;
        assert_eq!(count_rulesets(&pool).await.expect("count"), 0);
        insert_ruleset(
            &pool,
            &NewRuleset {
                name: "A",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert");
        assert_eq!(count_rulesets(&pool).await.expect("count"), 1);
    }
}
