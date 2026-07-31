//! The History surface's read model (v0.6.0): what happened, and what can be put
//! back.
//!
//! # Why this exists
//!
//! v0.5.0 built a great deal of undo machinery - a self-contained undo file after
//! a completed walk, an inverse-plan builder that runs through the same validator
//! and executor as a forward plan (D-09), and a partial rollback that reconstructs
//! the inverse from a halted run's journal tail. None of it was reachable: the
//! History route was a placeholder, and no surface called either rollback
//! preparation command. "Undo exists in the engine" is not "undo is available",
//! and the product's promise ("always give me a comprehensible way back") is only
//! true from the user's screen, not from the engine's API.
//!
//! This module supplies the one read the History screen needs, and - crucially -
//! computes the UNDO OFFER server-side rather than leaving the frontend to infer
//! it. Which undo path applies (a full undo file, a journal tail, or none at all)
//! depends on invariants the engine owns: whether a manifest was exported, whether
//! the recorded operations are reversible, whether anything landed at all, and
//! whether reconciliation left something ambiguous. Re-deriving that in TypeScript
//! would put a safety decision in the layer with the least context, which is the
//! same mistake as letting the frontend decide it may apply for real.
//!
//! # Rehearsals appear, and say so
//!
//! Practice runs are listed rather than hidden. Hiding them would make the screen
//! lie by omission: the user did run something, and if a rehearsal is missing they
//! will reasonably wonder whether a real change went unrecorded. They are labelled
//! as practice and never offered an undo, because nothing moved.

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::AppError;

use super::ApplyMode;

/// What the user can do about one past tidy-up.
///
/// Computed here rather than in the shell because each arm encodes an engine
/// invariant (see the module docs). The `kind` tag is what the frontend switches
/// on; the payload carries exactly what the matching command needs, so the shell
/// never has to assemble arguments for a rollback itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UndoOffer {
    /// A complete undo file was exported for this run: every change it made can be
    /// put back. Carries the `manifests.id` for `rollback_prepare`.
    PutEverythingBack { manifest_id: i64 },
    /// No undo file (the run halted or was stopped before exporting one), but the
    /// journal records exactly which operations landed. Carries those op ids, in
    /// walk order, for `rollback_prepare_partial` - which re-checks contiguity
    /// itself and refuses a selection with a gap (AC-16).
    PutRecentChangesBack { op_ids: Vec<i64> },
    /// The run made no changes at all, so there is nothing to reverse.
    NothingToPutBack,
    /// A practice run: it only ever touched memory, so nothing moved and there is
    /// nothing to put back.
    PracticeRun,
    /// Something about this run is unresolved and a person should look before any
    /// undo is attempted. Never offers an automatic reversal.
    NeedsALook,
}

/// One past tidy-up, as the History screen shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// The apply job this row describes.
    pub job_id: i64,
    /// Whether this was a real tidy-up or a practice run. `None` for a row whose
    /// `jobs.mode` is unreadable (the column is nullable - see
    /// [`ApplyMode::from_db_tag`]); such a row is never offered an undo.
    pub mode: Option<ApplyMode>,
    /// The job lifecycle state as recorded: `done`, `failed`, `stopped`, or
    /// `running` (a run stranded by a kill that startup reconciliation has not yet
    /// closed out).
    pub state: String,
    /// When the run began (ISO 8601 UTC), if recorded.
    pub started_at: Option<String>,
    /// When the run finished (ISO 8601 UTC), if it finished.
    pub finished_at: Option<String>,
    /// How many operations actually landed, counted from committed `done` journal
    /// rows - the durable record, not the plan's intent. A stopped run reports what
    /// it managed before stopping.
    pub changes_made: i64,
    /// What can be done about this run now.
    pub undo: UndoOffer,
}

/// The most recent apply jobs, newest first, with each run's undo offer resolved.
///
/// `limit` bounds the read so a long-lived library cannot make the screen
/// unbounded; the History surface pages by calling again with a larger limit.
/// Rollback jobs are included - undoing an undo is a legitimate thing to see in
/// the record - but scan jobs are not, since they change nothing.
pub async fn list_history(pool: &SqlitePool, limit: i64) -> Result<Vec<HistoryEntry>, AppError> {
    let rows = sqlx::query(
        "SELECT id, mode, state, started_at, finished_at \
         FROM jobs WHERE kind = 'apply' \
         ORDER BY COALESCE(started_at, '') DESC, id DESC \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::HistoryUnavailable {
        detail: e.to_string(),
    })?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let job_id: i64 = row.get("id");
        let mode = row
            .get::<Option<String>, _>("mode")
            .as_deref()
            .and_then(ApplyMode::from_db_tag);

        let done_op_ids = done_op_ids(pool, job_id).await?;
        let undo = resolve_undo_offer(pool, job_id, mode, &done_op_ids).await?;

        out.push(HistoryEntry {
            job_id,
            mode,
            state: row.get("state"),
            started_at: row.get("started_at"),
            finished_at: row.get("finished_at"),
            changes_made: done_op_ids.len() as i64,
            undo,
        });
    }
    Ok(out)
}

/// Decide which undo path (if any) this run supports.
///
/// The order of these checks is the safety order, not a convenience order:
/// unresolved ambiguity is considered BEFORE any offer is made, and a rehearsal is
/// excluded before a manifest is even looked for, so neither can fall through into
/// an offer to move real files.
async fn resolve_undo_offer(
    pool: &SqlitePool,
    job_id: i64,
    mode: Option<ApplyMode>,
    done_op_ids: &[i64],
) -> Result<UndoOffer, AppError> {
    // An unreadable mode means we cannot say whether this run touched real files.
    // Never offer to reverse it.
    let Some(mode) = mode else {
        return Ok(UndoOffer::NeedsALook);
    };

    // A rehearsal moved nothing, whatever its journal says: the walk mutated a
    // MemFs that no longer exists.
    if mode == ApplyMode::DryRun {
        return Ok(UndoOffer::PracticeRun);
    }

    // Reconciliation records an ambiguous in-doubt op as a `failed` terminal row
    // carrying a reconcile reason. An ambiguous op means the on-disk state does not
    // decisively say what happened, and a generic inverse could double-move or
    // clobber - so this run needs a person, not an automatic reversal.
    if has_unresolved_ambiguity(pool, job_id).await? {
        return Ok(UndoOffer::NeedsALook);
    }

    if done_op_ids.is_empty() {
        return Ok(UndoOffer::NothingToPutBack);
    }

    // A completed walk exports a self-contained undo file; its index row points at
    // it. `reversible = 0` records honestly that some operation in the run cannot
    // be reversed, so the whole-run offer is withheld.
    if let Some(manifest_id) = reversible_manifest_id(pool, job_id).await? {
        return Ok(UndoOffer::PutEverythingBack { manifest_id });
    }

    // No undo file (halted, stopped, or failed before export), but the journal
    // records what landed.
    Ok(UndoOffer::PutRecentChangesBack {
        op_ids: done_op_ids.to_vec(),
    })
}

/// The op ids this job actually applied, in walk order, from committed `done`
/// journal rows.
async fn done_op_ids(pool: &SqlitePool, job_id: i64) -> Result<Vec<i64>, AppError> {
    let rows = sqlx::query(
        "SELECT op_id FROM journal WHERE job_id = ? AND phase = 'done' ORDER BY seq, id",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::HistoryUnavailable {
        detail: e.to_string(),
    })?;
    Ok(rows.into_iter().map(|r| r.get::<i64, _>("op_id")).collect())
}

/// Whether startup reconciliation closed an op on this job as ambiguous.
///
/// Matches the marker [`super::reconcile`] writes into the `failed` row's detail,
/// rather than any free-text failure message, so an ordinary walk-time failure
/// (permission denied, say) does not get mistaken for an unresolved state.
async fn has_unresolved_ambiguity(pool: &SqlitePool, job_id: i64) -> Result<bool, AppError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM journal \
         WHERE job_id = ? AND phase = 'failed' \
           AND detail_json LIKE '%\"reconcile\":\"ambiguous on-disk state\"%'",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::HistoryUnavailable {
        detail: e.to_string(),
    })?;
    Ok(row.get::<i64, _>("n") > 0)
}

/// The id of this job's exported undo file, if it has one AND every operation in
/// it is reversible.
async fn reversible_manifest_id(pool: &SqlitePool, job_id: i64) -> Result<Option<i64>, AppError> {
    let row = sqlx::query(
        "SELECT id FROM manifests \
         WHERE job_id = ? AND reversible = 1 AND mode = 'real' \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::HistoryUnavailable {
        detail: e.to_string(),
    })?;
    Ok(row.map(|r| r.get::<i64, _>("id")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::exec::{Journal, JournalEntry, JournalPhase, SqliteJournal};
    use tempfile::TempDir;

    const NOW: &str = "2026-07-30T00:00:00Z";

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    async fn add_job(pool: &SqlitePool, mode: Option<&str>, state: &str) -> i64 {
        sqlx::query("INSERT INTO jobs (kind, state, started_at, mode) VALUES ('apply', ?, ?, ?)")
            .bind(state)
            .bind(NOW)
            .bind(mode)
            .execute(pool)
            .await
            .expect("insert job")
            .last_insert_rowid()
    }

    /// A plan + one op, so `journal.op_id` has something real to point at.
    /// `plans.scan_id` and `plans.ruleset_id` are NOT NULL FKs, so both are seeded.
    async fn add_plan_op(pool: &SqlitePool) -> (i64, i64) {
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', ?, ?, 'completed')",
        )
        .bind(r"E:\lib")
        .bind(NOW)
        .execute(pool)
        .await
        .expect("scan")
        .last_insert_rowid();
        let ruleset_id = crate::db::rulesets::insert_ruleset(
            pool,
            &crate::db::rulesets::NewRuleset {
                name: "history-fixture",
                body_json: "{}",
                schema_version: 1,
            },
            NOW,
        )
        .await
        .expect("ruleset");
        let plan_id = crate::db::plans::insert_plan(
            pool,
            &crate::db::plans::NewPlan {
                scan_id,
                ruleset_id,
                status: "draft",
                stats_json: None,
            },
            &[crate::db::plans::NewPlanOp {
                op_group: "loose",
                kind: "move",
                kind_reason: None,
                source_path: r"E:\lib\Old\B.m4b",
                target_path: r"E:\lib\New\B.m4b",
                rationale: "op.",
                rule_id: "test-rule",
                confidence: "high",
                byte_size: 1,
                validation_state: "valid",
                validation_reason: None,
                provenance_json: None,
            }],
            NOW,
        )
        .await
        .expect("insert plan");
        let op_id = crate::db::plans::get_plan_ops(pool, plan_id)
            .await
            .expect("ops")[0]
            .id;
        (plan_id, op_id)
    }

    async fn journal_done(pool: &SqlitePool, job_id: i64, op_id: i64, seq: i64) {
        let j = SqliteJournal::new(pool.clone());
        let e = JournalEntry {
            job_id,
            seq,
            op_id,
            phase: JournalPhase::Intent,
            at: NOW.to_string(),
            detail_json: None,
        };
        j.write_intent(&e).await.unwrap();
        j.write_done(&JournalEntry {
            phase: JournalPhase::Done,
            ..e
        })
        .await
        .unwrap();
    }

    /// A practice run is listed (not hidden) and is never offered an undo, even
    /// when its journal shows completed operations - the MemFs it walked is gone.
    #[tokio::test]
    async fn a_rehearsal_is_listed_as_a_practice_run_and_offers_no_undo() {
        let (_d, pool) = fresh_pool().await;
        let job = add_job(&pool, Some("dry-run"), "done").await;
        let (_plan, op) = add_plan_op(&pool).await;
        journal_done(&pool, job, op, 0).await;

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(history.len(), 1, "the practice run is listed, not hidden");
        assert_eq!(history[0].mode, Some(ApplyMode::DryRun));
        assert_eq!(history[0].undo, UndoOffer::PracticeRun);
        pool.close().await;
    }

    /// A completed real run with a reversible undo file offers the whole-run undo.
    #[tokio::test]
    async fn a_completed_real_run_offers_the_whole_run_undo() {
        let (_d, pool) = fresh_pool().await;
        let job = add_job(&pool, Some("real"), "done").await;
        let (plan, op) = add_plan_op(&pool).await;
        journal_done(&pool, job, op, 0).await;
        let manifest_id = sqlx::query(
            "INSERT INTO manifests (job_id, plan_id, json_path, reversible, mode) \
             VALUES (?, ?, 'E:\\Reports\\undo.json', 1, 'real')",
        )
        .bind(job)
        .bind(plan)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(history[0].changes_made, 1);
        assert_eq!(
            history[0].undo,
            UndoOffer::PutEverythingBack { manifest_id }
        );
        pool.close().await;
    }

    /// A halted real run with no undo file falls back to the journal tail, carrying
    /// the op ids the partial rollback needs.
    #[tokio::test]
    async fn a_halted_real_run_offers_the_journal_tail() {
        let (_d, pool) = fresh_pool().await;
        let job = add_job(&pool, Some("real"), "stopped").await;
        let (_plan, op) = add_plan_op(&pool).await;
        journal_done(&pool, job, op, 0).await;

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(
            history[0].undo,
            UndoOffer::PutRecentChangesBack { op_ids: vec![op] }
        );
        pool.close().await;
    }

    /// A real run that landed nothing says so, rather than offering an empty undo.
    #[tokio::test]
    async fn a_real_run_that_changed_nothing_offers_nothing_to_put_back() {
        let (_d, pool) = fresh_pool().await;
        add_job(&pool, Some("real"), "failed").await;

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(history[0].changes_made, 0);
        assert_eq!(history[0].undo, UndoOffer::NothingToPutBack);
        pool.close().await;
    }

    /// An op reconciliation left ambiguous suppresses every automatic undo offer,
    /// even though the run has completed operations that would otherwise qualify.
    #[tokio::test]
    async fn an_ambiguous_reconciliation_needs_a_look_instead_of_an_undo() {
        let (_d, pool) = fresh_pool().await;
        let job = add_job(&pool, Some("real"), "failed").await;
        let (_plan, op) = add_plan_op(&pool).await;
        journal_done(&pool, job, op, 0).await;

        // A second op the reconciler closed as ambiguous.
        let (_p2, op2) = add_plan_op(&pool).await;
        let j = SqliteJournal::new(pool.clone());
        let e = JournalEntry {
            job_id: job,
            seq: 1,
            op_id: op2,
            phase: JournalPhase::Intent,
            at: NOW.to_string(),
            detail_json: None,
        };
        j.write_intent(&e).await.unwrap();
        j.write_failed(&JournalEntry {
            phase: JournalPhase::Failed,
            detail_json: Some(r#"{"reconcile":"ambiguous on-disk state"}"#.to_string()),
            ..e
        })
        .await
        .unwrap();

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(
            history[0].undo,
            UndoOffer::NeedsALook,
            "an ambiguous op is never auto-reversed"
        );
        pool.close().await;
    }

    /// An ordinary walk-time failure is NOT mistaken for an unresolved ambiguity.
    #[tokio::test]
    async fn an_ordinary_failure_still_offers_the_journal_tail() {
        let (_d, pool) = fresh_pool().await;
        let job = add_job(&pool, Some("real"), "failed").await;
        let (_plan, op) = add_plan_op(&pool).await;
        journal_done(&pool, job, op, 0).await;

        let (_p2, op2) = add_plan_op(&pool).await;
        let j = SqliteJournal::new(pool.clone());
        let e = JournalEntry {
            job_id: job,
            seq: 1,
            op_id: op2,
            phase: JournalPhase::Intent,
            at: NOW.to_string(),
            detail_json: None,
        };
        j.write_intent(&e).await.unwrap();
        j.write_failed(&JournalEntry {
            phase: JournalPhase::Failed,
            detail_json: Some(r#"{"code":"access-denied"}"#.to_string()),
            ..e
        })
        .await
        .unwrap();

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(
            history[0].undo,
            UndoOffer::PutRecentChangesBack { op_ids: vec![op] }
        );
        pool.close().await;
    }

    /// A row whose mode is unreadable is never offered an undo.
    #[tokio::test]
    async fn an_unknown_mode_needs_a_look() {
        let (_d, pool) = fresh_pool().await;
        add_job(&pool, None, "done").await;

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(history[0].mode, None);
        assert_eq!(history[0].undo, UndoOffer::NeedsALook);
        pool.close().await;
    }

    /// Newest first, and scans never appear.
    #[tokio::test]
    async fn history_is_newest_first_and_excludes_scans() {
        let (_d, pool) = fresh_pool().await;
        sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('scan', 'done', ?)")
            .bind("2026-07-30T05:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        let older = add_job(&pool, Some("real"), "done").await;
        sqlx::query("UPDATE jobs SET started_at = '2026-07-29T00:00:00Z' WHERE id = ?")
            .bind(older)
            .execute(&pool)
            .await
            .unwrap();
        let newer = add_job(&pool, Some("real"), "done").await;
        sqlx::query("UPDATE jobs SET started_at = '2026-07-30T00:00:00Z' WHERE id = ?")
            .bind(newer)
            .execute(&pool)
            .await
            .unwrap();

        let history = list_history(&pool, 20).await.unwrap();
        assert_eq!(history.len(), 2, "the scan job is excluded");
        assert_eq!(history[0].job_id, newer, "newest first");
        assert_eq!(history[1].job_id, older);
        pool.close().await;
    }

    /// `limit` bounds the read.
    #[tokio::test]
    async fn limit_bounds_the_read() {
        let (_d, pool) = fresh_pool().await;
        for _ in 0..5 {
            add_job(&pool, Some("real"), "done").await;
        }
        assert_eq!(list_history(&pool, 3).await.unwrap().len(), 3);
        pool.close().await;
    }

    /// `UndoOffer` is an IPC type: kebab-case tag, payload intact.
    #[test]
    fn undo_offer_round_trips_through_serde() {
        let o = UndoOffer::PutRecentChangesBack {
            op_ids: vec![4, 5, 6],
        };
        let json = serde_json::to_string(&o).expect("serialize");
        assert!(json.contains("\"kind\":\"put-recent-changes-back\""));
        let back: UndoOffer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, o);
    }
}
