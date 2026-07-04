//! F-801 (ruleset model and persistence) database layer: sqlx CRUD for the
//! `rulesets` table (migration 0002).
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
        "SELECT id, name, body_json, schema_version, created_at, updated_at \
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
        "SELECT id, name, body_json, schema_version, created_at, updated_at \
         FROM rulesets ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_ruleset).collect())
}

/// Overwrite an existing ruleset's body in place (a named ruleset being
/// edited, as opposed to plan regeneration which always creates a new
/// `plans` row). Does not validate `body_json`; see module doc. Bumps
/// `updated_at` to `now`; `created_at` is untouched.
pub async fn update_ruleset_body(
    pool: &SqlitePool,
    id: i64,
    body_json: &str,
    schema_version: i64,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE rulesets SET body_json = ?, schema_version = ?, updated_at = ? WHERE id = ?",
    )
    .bind(body_json)
    .bind(schema_version)
    .bind(now)
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

    /// Updating a ruleset's body changes `body_json`/`schema_version`/
    /// `updated_at` but leaves `created_at` untouched, and never inserts a
    /// second row (distinct from plan regeneration's always-insert
    /// contract: a ruleset is an editable named row, not an immutable plan
    /// header).
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

        update_ruleset_body(&pool, id, r#"{"v":2}"#, 2, "2026-07-04T01:00:00Z")
            .await
            .expect("update_ruleset_body");

        let all = list_rulesets(&pool).await.expect("list_rulesets");
        assert_eq!(all.len(), 1, "update must not insert a second row");

        let fetched = get_ruleset(&pool, id)
            .await
            .expect("get_ruleset")
            .expect("ruleset must exist");
        assert_eq!(fetched.body_json, r#"{"v":2}"#);
        assert_eq!(fetched.schema_version, 2);
        assert_eq!(fetched.created_at, "2026-07-04T00:00:00Z");
        assert_eq!(fetched.updated_at, "2026-07-04T01:00:00Z");
    }
}
