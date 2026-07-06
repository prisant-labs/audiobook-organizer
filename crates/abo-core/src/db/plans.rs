//! F-403/F-404/F-405/F-507 database layer: sqlx CRUD for the `plans` and
//! `plan_ops` tables (migration 0002).
//!
//! v0.3.0 Phase 1 (this module) is plumbing only: it persists a plan header
//! plus its ordered operation list, and reads them back. It does not build
//! a plan (F-403, Phase 4), validate one (F-404, Phase 5), or capture
//! provenance (F-507, Phase 6) - those phases call [`insert_plan`] with
//! already-decided values. See migration 0002's column-freeze comment for
//! which `plan_ops` columns are write-once versus the one mutable
//! (`approval`, `approval_updated_at`) pair.
//!
//! Immutability contract (F-405 AC-16): [`insert_plan`] always INSERTs a
//! fresh `plans` row and a fresh set of `plan_ops` rows in one transaction;
//! nothing in this module UPDATEs or DELETEs a `plans` row or a `plan_ops`
//! row's descriptive columns. Regenerating a plan after a ruleset change is
//! therefore just another [`insert_plan`] call, never a mutation of the
//! prior plan. [`set_approval`] is the one exception, by design: it targets
//! the mutable `approval`/`approval_updated_at` pair only (see migration
//! 0002's freeze note) and never touches any other column.

use sqlx::{Row, SqlitePool};

/// One persisted plan header row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRow {
    pub id: i64,
    pub scan_id: i64,
    pub ruleset_id: i64,
    pub created_at: String,
    pub status: String,
    pub stats_json: Option<String>,
}

/// A plan header to insert. Fields the caller decides; `id`/`created_at` are
/// assigned by [`insert_plan`] (id via autoincrement, `created_at` from the
/// caller-supplied `now`, kept as a parameter for the same clock-free reason
/// as `db::rulesets`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlan<'a> {
    pub scan_id: i64,
    pub ruleset_id: i64,
    pub status: &'a str,
    pub stats_json: Option<&'a str>,
}

/// One persisted plan-operation row, in the frozen `plan_ops` shape
/// (migration 0002). `seq` is the explicit, caller-assigned ordering within
/// the plan (dependency ordering, F-403 AC-10 depends on this surviving the
/// round trip byte-for-byte, not merely "some" order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanOpRow {
    pub id: i64,
    pub plan_id: i64,
    pub seq: i64,
    pub op_group: String,
    pub kind: String,
    pub kind_reason: Option<String>,
    pub source_path: String,
    pub target_path: String,
    pub rationale: String,
    pub rule_id: String,
    pub confidence: String,
    pub byte_size: i64,
    pub validation_state: String,
    pub validation_reason: Option<String>,
    pub provenance_json: Option<String>,
    pub approval: String,
    pub approval_updated_at: Option<String>,
}

/// One operation to insert as part of a new plan. Every descriptive
/// `plan_ops` column (everything but `id`/`plan_id`/`seq`, which
/// [`insert_plan`] assigns) lives here; `approval` always starts `pending`
/// with no `approval_updated_at` (set later via [`set_approval`]), so it is
/// deliberately not a field of this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPlanOp<'a> {
    pub op_group: &'a str,
    pub kind: &'a str,
    pub kind_reason: Option<&'a str>,
    pub source_path: &'a str,
    pub target_path: &'a str,
    pub rationale: &'a str,
    pub rule_id: &'a str,
    pub confidence: &'a str,
    pub byte_size: i64,
    pub validation_state: &'a str,
    pub validation_reason: Option<&'a str>,
    pub provenance_json: Option<&'a str>,
}

/// The literal starting value every new `plan_ops` row's `approval` column
/// takes (matches the column's own `DEFAULT 'pending'` in migration 0002;
/// restated here so callers/tests reading this module never have to go back
/// to the SQL to know it).
pub const APPROVAL_PENDING: &str = "pending";

/// Insert a new plan header plus its ordered operations in one transaction:
/// either the whole plan (header and every op) commits, or none of it does,
/// so a plan is never partially persisted. `seq` is assigned as each op's
/// position in `ops` (0-based), which is also insertion order. Returns the
/// new plan's id.
pub async fn insert_plan(
    pool: &SqlitePool,
    new_plan: &NewPlan<'_>,
    ops: &[NewPlanOp<'_>],
    now: &str,
) -> Result<i64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let plan_result = sqlx::query(
        "INSERT INTO plans (scan_id, ruleset_id, created_at, status, stats_json) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(new_plan.scan_id)
    .bind(new_plan.ruleset_id)
    .bind(now)
    .bind(new_plan.status)
    .bind(new_plan.stats_json)
    .execute(&mut *tx)
    .await?;
    let plan_id = plan_result.last_insert_rowid();

    for (seq, op) in ops.iter().enumerate() {
        sqlx::query(
            "INSERT INTO plan_ops ( \
                plan_id, seq, op_group, kind, kind_reason, source_path, target_path, \
                rationale, rule_id, confidence, byte_size, validation_state, \
                validation_reason, provenance_json, approval \
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(plan_id)
        .bind(seq as i64)
        .bind(op.op_group)
        .bind(op.kind)
        .bind(op.kind_reason)
        .bind(op.source_path)
        .bind(op.target_path)
        .bind(op.rationale)
        .bind(op.rule_id)
        .bind(op.confidence)
        .bind(op.byte_size)
        .bind(op.validation_state)
        .bind(op.validation_reason)
        .bind(op.provenance_json)
        .bind(APPROVAL_PENDING)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(plan_id)
}

/// Fetch one plan header by id, or `None` if it does not exist.
pub async fn get_plan(pool: &SqlitePool, plan_id: i64) -> Result<Option<PlanRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, scan_id, ruleset_id, created_at, status, stats_json \
         FROM plans WHERE id = ?",
    )
    .bind(plan_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| PlanRow {
        id: r.get("id"),
        scan_id: r.get("scan_id"),
        ruleset_id: r.get("ruleset_id"),
        created_at: r.get("created_at"),
        status: r.get("status"),
        stats_json: r.get("stats_json"),
    }))
}

/// Fetch every operation belonging to `plan_id`, ordered by `seq` (the plan's
/// canonical order, dependency-ordering-preserving per F-403 AC-10).
pub async fn get_plan_ops(pool: &SqlitePool, plan_id: i64) -> Result<Vec<PlanOpRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, plan_id, seq, op_group, kind, kind_reason, source_path, target_path, \
                rationale, rule_id, confidence, byte_size, validation_state, \
                validation_reason, provenance_json, approval, approval_updated_at \
         FROM plan_ops WHERE plan_id = ? ORDER BY seq",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PlanOpRow {
            id: r.get("id"),
            plan_id: r.get("plan_id"),
            seq: r.get("seq"),
            op_group: r.get("op_group"),
            kind: r.get("kind"),
            kind_reason: r.get("kind_reason"),
            source_path: r.get("source_path"),
            target_path: r.get("target_path"),
            rationale: r.get("rationale"),
            rule_id: r.get("rule_id"),
            confidence: r.get("confidence"),
            byte_size: r.get("byte_size"),
            validation_state: r.get("validation_state"),
            validation_reason: r.get("validation_reason"),
            provenance_json: r.get("provenance_json"),
            approval: r.get("approval"),
            approval_updated_at: r.get("approval_updated_at"),
        })
        .collect())
}

/// Fetch one plan-operation row by its own id, or `None` if it does not exist
/// (v0.4.0 Phase 5: the review surface's per-op exclude reads the row back
/// after [`crate::plan::validate::set_op_approval`] to return the caller an
/// up-to-date view without needing the owning `plan_id`).
pub async fn get_plan_op(
    pool: &SqlitePool,
    plan_op_id: i64,
) -> Result<Option<PlanOpRow>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, plan_id, seq, op_group, kind, kind_reason, source_path, target_path, \
                rationale, rule_id, confidence, byte_size, validation_state, \
                validation_reason, provenance_json, approval, approval_updated_at \
         FROM plan_ops WHERE id = ?",
    )
    .bind(plan_op_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| PlanOpRow {
        id: r.get("id"),
        plan_id: r.get("plan_id"),
        seq: r.get("seq"),
        op_group: r.get("op_group"),
        kind: r.get("kind"),
        kind_reason: r.get("kind_reason"),
        source_path: r.get("source_path"),
        target_path: r.get("target_path"),
        rationale: r.get("rationale"),
        rule_id: r.get("rule_id"),
        confidence: r.get("confidence"),
        byte_size: r.get("byte_size"),
        validation_state: r.get("validation_state"),
        validation_reason: r.get("validation_reason"),
        provenance_json: r.get("provenance_json"),
        approval: r.get("approval"),
        approval_updated_at: r.get("approval_updated_at"),
    }))
}

/// List every plan header, oldest first (insertion order via id).
pub async fn list_plans(pool: &SqlitePool) -> Result<Vec<PlanRow>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, scan_id, ruleset_id, created_at, status, stats_json FROM plans ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PlanRow {
            id: r.get("id"),
            scan_id: r.get("scan_id"),
            ruleset_id: r.get("ruleset_id"),
            created_at: r.get("created_at"),
            status: r.get("status"),
            stats_json: r.get("stats_json"),
        })
        .collect())
}

/// Update ONLY the mutable `approval`/`approval_updated_at` pair on one
/// `plan_ops` row (F-405 AC-17's approve/reject/exclude state machine).
/// Every other column on the row is untouched, preserving the write-once
/// contract for the descriptive columns (see migration 0002's freeze note
/// and this module's doc comment).
///
/// This function does not enforce the "a blocked op cannot be approved"
/// rule; that check belongs to the caller (Phase 5's state machine), which
/// reads `validation_state` before deciding whether to call this at all.
pub async fn set_approval(
    pool: &SqlitePool,
    plan_op_id: i64,
    approval: &str,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE plan_ops SET approval = ?, approval_updated_at = ? WHERE id = ?")
        .bind(approval)
        .bind(now)
        .bind(plan_op_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::db::rulesets::{insert_ruleset, NewRuleset};
    use tempfile::TempDir;

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    /// A scan row and a ruleset row, the two foreign keys `plans` needs, so
    /// tests in this module can insert a plan without reaching into
    /// `crate::scan`.
    async fn fresh_scan_and_ruleset(pool: &SqlitePool) -> (i64, i64) {
        let scan_result = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:\\Books', '2026-07-04T00:00:00Z', 'completed')",
        )
        .execute(pool)
        .await
        .expect("insert scans row");
        let scan_id = scan_result.last_insert_rowid();

        let ruleset_id = insert_ruleset(
            pool,
            &NewRuleset {
                name: "Default",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_ruleset");

        (scan_id, ruleset_id)
    }

    fn sample_ops() -> Vec<NewPlanOp<'static>> {
        vec![
            NewPlanOp {
                op_group: "loose-root-books",
                kind: "mkdir",
                kind_reason: None,
                source_path: "E:\\Books\\Some Book",
                target_path: "E:\\Books\\N.K. Jemisin",
                rationale: "Create the author folder before moving the book into it.",
                rule_id: "loose-root-books-mkdir",
                confidence: "high",
                byte_size: 0,
                validation_state: "valid",
                validation_reason: None,
                provenance_json: None,
            },
            NewPlanOp {
                op_group: "flatten-packs",
                kind: "move",
                kind_reason: None,
                source_path: "E:\\Books\\Hugo Pack\\2016^ - N.K. Jemisin - The Fifth Season",
                target_path:
                    "E:\\Books\\N.K. Jemisin\\The Broken Earth\\Book 01 - The Fifth Season",
                rationale:
                    "Matched pattern 4 (year-author-title-award); extracted from a Hugo pack.",
                rule_id: "flatten-packs-pattern-4",
                confidence: "high",
                byte_size: 446_693_376,
                validation_state: "warning",
                validation_reason: Some("path-length-near-260"),
                provenance_json: Some(
                    r#"{"pack_id":"hugo-winners","pack_title":"Hugo Award Winners","award_marker":"^"}"#,
                ),
            },
        ]
    }

    /// A plan plus its ops, inserted then read back, is field-for-field
    /// equal to what was inserted, including `seq` order, nullable columns,
    /// and the `approval` default (F-403/F-405 Phase 1 verification: "a
    /// db-layer test inserts a plan + ops and reads them back equal").
    #[tokio::test]
    async fn insert_and_read_back_plan_with_ops_is_equal() {
        let (_db, pool) = fresh_pool().await;
        let (scan_id, ruleset_id) = fresh_scan_and_ruleset(&pool).await;

        let ops = sample_ops();
        let plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: Some(r#"{"total_ops":2}"#),
            },
            &ops,
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_plan");

        let plan = get_plan(&pool, plan_id)
            .await
            .expect("get_plan")
            .expect("plan must exist");
        assert_eq!(
            plan,
            PlanRow {
                id: plan_id,
                scan_id,
                ruleset_id,
                created_at: "2026-07-04T00:00:00Z".to_string(),
                status: "draft".to_string(),
                stats_json: Some(r#"{"total_ops":2}"#.to_string()),
            }
        );

        let fetched_ops = get_plan_ops(&pool, plan_id).await.expect("get_plan_ops");
        assert_eq!(fetched_ops.len(), ops.len());
        for (seq, (fetched, original)) in fetched_ops.iter().zip(ops.iter()).enumerate() {
            assert_eq!(fetched.plan_id, plan_id);
            assert_eq!(fetched.seq, seq as i64, "seq preserves insertion order");
            assert_eq!(fetched.op_group, original.op_group);
            assert_eq!(fetched.kind, original.kind);
            assert_eq!(fetched.kind_reason.as_deref(), original.kind_reason);
            assert_eq!(fetched.source_path, original.source_path);
            assert_eq!(fetched.target_path, original.target_path);
            assert_eq!(fetched.rationale, original.rationale);
            assert_eq!(fetched.rule_id, original.rule_id);
            assert_eq!(fetched.confidence, original.confidence);
            assert_eq!(fetched.byte_size, original.byte_size);
            assert_eq!(fetched.validation_state, original.validation_state);
            assert_eq!(
                fetched.validation_reason.as_deref(),
                original.validation_reason
            );
            assert_eq!(fetched.provenance_json.as_deref(), original.provenance_json);
            assert_eq!(fetched.approval, APPROVAL_PENDING);
            assert_eq!(fetched.approval_updated_at, None);
        }
    }

    /// Regenerating a plan (a second `insert_plan` call, e.g. after a
    /// ruleset tweak) creates a distinct plan id and leaves the first
    /// plan's ops completely unchanged (F-405 AC-16).
    #[tokio::test]
    async fn regenerating_a_plan_never_mutates_the_prior_plan() {
        let (_db, pool) = fresh_pool().await;
        let (scan_id, ruleset_id) = fresh_scan_and_ruleset(&pool).await;

        let first_ops = sample_ops();
        let first_plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &first_ops,
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert first plan");
        let first_ops_before = get_plan_ops(&pool, first_plan_id)
            .await
            .expect("ops before");

        // Regenerate: a second, unrelated plan (simulating a ruleset change).
        let second_ops = vec![NewPlanOp {
            op_group: "strip-noise",
            kind: "rename",
            kind_reason: None,
            source_path: "E:\\Books\\Weird [64k] Name",
            target_path: "E:\\Books\\Weird Name",
            rationale: "Stripped a bitrate tag.",
            rule_id: "strip-noise-bitrate",
            confidence: "high",
            byte_size: 0,
            validation_state: "valid",
            validation_reason: None,
            provenance_json: None,
        }];
        let second_plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &second_ops,
            "2026-07-04T00:05:00Z",
        )
        .await
        .expect("insert second plan");

        assert_ne!(
            first_plan_id, second_plan_id,
            "regeneration must produce a distinct plan id"
        );

        let first_ops_after = get_plan_ops(&pool, first_plan_id).await.expect("ops after");
        assert_eq!(
            first_ops_before, first_ops_after,
            "the prior plan's ops must be byte-for-byte unchanged after regeneration"
        );

        let second_fetched = get_plan_ops(&pool, second_plan_id)
            .await
            .expect("second ops");
        assert_eq!(second_fetched.len(), 1);
        assert_eq!(second_fetched[0].op_group, "strip-noise");

        let all_plans = list_plans(&pool).await.expect("list_plans");
        assert_eq!(all_plans.len(), 2, "both plans persist independently");
    }

    /// [`set_approval`] updates only the mutable pair on the targeted row,
    /// leaving every descriptive column (and every other row) untouched.
    #[tokio::test]
    async fn set_approval_updates_only_the_mutable_pair() {
        let (_db, pool) = fresh_pool().await;
        let (scan_id, ruleset_id) = fresh_scan_and_ruleset(&pool).await;

        let ops = sample_ops();
        let plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &ops,
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_plan");

        let before = get_plan_ops(&pool, plan_id).await.expect("ops before");
        let target_id = before[0].id;

        set_approval(&pool, target_id, "approved", "2026-07-04T02:00:00Z")
            .await
            .expect("set_approval");

        let after = get_plan_ops(&pool, plan_id).await.expect("ops after");
        let updated = after.iter().find(|op| op.id == target_id).unwrap();
        assert_eq!(updated.approval, "approved");
        assert_eq!(
            updated.approval_updated_at.as_deref(),
            Some("2026-07-04T02:00:00Z")
        );
        // Every descriptive column is untouched.
        assert_eq!(updated.source_path, before[0].source_path);
        assert_eq!(updated.target_path, before[0].target_path);
        assert_eq!(updated.validation_state, before[0].validation_state);

        // The other row in the same plan is completely unaffected.
        let other = after.iter().find(|op| op.id != target_id).unwrap();
        assert_eq!(other.approval, APPROVAL_PENDING);
        assert_eq!(other.approval_updated_at, None);
    }

    /// An empty ops slice is a legal plan (e.g. an already-tidy library,
    /// F-505 AC-19's empty-plan case starts here at the storage layer): the
    /// header persists and `get_plan_ops` returns an empty vec, not an
    /// error.
    #[tokio::test]
    async fn a_plan_with_zero_ops_persists_and_reads_back_empty() {
        let (_db, pool) = fresh_pool().await;
        let (scan_id, ruleset_id) = fresh_scan_and_ruleset(&pool).await;

        let plan_id = insert_plan(
            &pool,
            &NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: Some(r#"{"total_ops":0}"#),
            },
            &[],
            "2026-07-04T00:00:00Z",
        )
        .await
        .expect("insert_plan with zero ops");

        let ops = get_plan_ops(&pool, plan_id).await.expect("get_plan_ops");
        assert!(ops.is_empty());
    }
}
