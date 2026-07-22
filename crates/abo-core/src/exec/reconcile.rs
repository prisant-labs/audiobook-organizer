//! F-606 (interruption safety + resume): reconcile the single in-doubt operation
//! after a process kill.
//!
//! The safety spine's journal-before-act invariant (F-602) means that after a
//! process is killed mid-apply, AT MOST ONE operation can be in doubt: one whose
//! `intent` row was flushed but whose `done`/`failed` terminal row was not. The
//! executor walk is serial (one op at a time, awaiting each [`Vfs`] call), so a
//! kill interrupts exactly one op, and the single-writer lock guarantees no second
//! writer produced a competing row. This module supplies the two primitives a
//! startup reconciliation pass is built from:
//!
//! - [`query_in_doubt`] finds the in-doubt op(s) in the journal (AC-1). It returns
//!   a `Vec` rather than an `Option` on purpose: the invariant says at most one,
//!   so the reconciler treats an empty result as "nothing to recover" and MORE
//!   than one as a safety abort (a corrupt or hand-edited journal), never silently
//!   picking one.
//! - [`verify_outcome`] re-reads the filesystem to determine what actually
//!   happened to that op, classifying it as [`OpOutcome::Completed`],
//!   [`OpOutcome::NotStarted`], or [`OpOutcome::Ambiguous`] (AC-2 rename, AC-3
//!   cross-volume copy).
//!
//! The orchestration that ties these together (look the op up in the plan, write
//! the correct terminal journal row, and offer the user resume-or-rollback) plus
//! the startup hook and the IPC surface land in the following slices; these two
//! primitives are pure and independently testable, which is where the safety
//! reasoning lives.
//!
//! # The FD-33 lost-tail boundary
//!
//! `open_db` runs WAL with `synchronous = NORMAL`. A committed `intent` survives a
//! process kill (its frames are already handed to the OS), which is the threat
//! F-606 recovers from. It does NOT survive a power loss between commit and the
//! next checkpoint. If a filesystem call was in flight when power was lost and the
//! `intent` frame never reached disk, [`query_in_doubt`] returns EMPTY even though
//! a real operation was mid-flight. That is not corruption: the reconciler treats
//! an empty result as "no recoverable in-doubt op" and falls through to marking the
//! stranded job interrupted, rather than inventing a recovery it cannot ground in a
//! journal row.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::db::plans::PlanOpRow;
use crate::error::AppError;

use super::vfs::Vfs;
use super::{Journal, JournalEntry, JournalPhase, SqliteJournal};

/// The verified on-disk outcome of the single in-doubt operation, determined by
/// re-reading the filesystem after an interruption.
///
/// The classification drives what the reconciler does next:
/// - [`Completed`](OpOutcome::Completed): the op provably took effect. Record a
///   `done` terminal row and resume from the NEXT op (AC-5).
/// - [`NotStarted`](OpOutcome::NotStarted): the op provably never took effect.
///   Record a `failed` terminal row and resume from THIS op (AC-4).
/// - [`Ambiguous`](OpOutcome::Ambiguous): the on-disk state does not decisively
///   say either way. Record a `failed` terminal row and offer rollback only - an
///   ambiguous op is NEVER auto-resumed, because resuming a half-applied op risks a
///   double move or a clobber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum OpOutcome {
    /// The operation provably completed on disk.
    Completed,
    /// The operation provably never started.
    NotStarted,
    /// The on-disk state is ambiguous; do not auto-resume.
    Ambiguous,
}

/// Find every `intent` row for `job_id` that has no matching `done`/`failed`
/// terminal row for the same `op_id` - the operation(s) left in doubt by an
/// interruption (AC-1). Ordered most-recent-first (`seq` then insertion order).
///
/// The single-writer plus journal-before-act invariant guarantees AT MOST ONE such
/// row after a real kill. This returns a `Vec` so the caller can enforce that
/// invariant rather than trusting it: an empty result means nothing is recoverable
/// (a clean finish, or the FD-33 lost-WAL-tail case), and more than one means a
/// corrupt or hand-edited journal the caller must treat as a safety abort.
pub async fn query_in_doubt(pool: &SqlitePool, job_id: i64) -> Result<Vec<JournalEntry>, AppError> {
    let rows = sqlx::query(
        "SELECT j.job_id AS job_id, j.seq AS seq, j.op_id AS op_id, \
                j.at AS at, j.detail_json AS detail_json \
         FROM journal AS j \
         WHERE j.job_id = ? AND j.phase = 'intent' \
           AND NOT EXISTS ( \
             SELECT 1 FROM journal AS t \
             WHERE t.job_id = j.job_id AND t.op_id = j.op_id \
               AND t.phase IN ('done', 'failed') \
           ) \
         ORDER BY j.seq DESC, j.id DESC",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::ReconcileFailed {
        detail: e.to_string(),
    })?;

    Ok(rows
        .into_iter()
        .map(|r| JournalEntry {
            job_id: r.get("job_id"),
            seq: r.get("seq"),
            op_id: r.get("op_id"),
            // Every row is an `intent` (the query filters on it), so the phase is
            // fixed rather than parsed back from the string column.
            phase: JournalPhase::Intent,
            at: r.get("at"),
            detail_json: r.get("detail_json"),
        })
        .collect())
}

/// Classify the real on-disk outcome of an in-doubt op by probing the filesystem
/// through the [`Vfs`] seam.
///
/// `expected_size` distinguishes the two move mechanisms:
/// - `None` for a same-volume `rename`/`move`/`quarantine` (AC-2): existence alone
///   is decisive, because [`RealFs`](super::RealFs) renames are atomic no-clobber -
///   after a kill the item is at the source XOR the target, never both from a
///   genuine rename.
/// - `Some(size)` for a cross-volume `copy + verify + delete` (AC-3): the target
///   must be present AND its size must match the source snapshot to count as
///   completed; a present-but-short target is a partial copy, which is ambiguous.
///
/// The `(source, target)` presence table:
/// - source gone, target present -> `Completed` (rename), or size-checked (copy);
/// - source present, target absent -> `NotStarted`;
/// - both present (a copy whose source-delete had not run yet, or an unexplained
///   duplicate) or neither present (the item vanished) -> `Ambiguous`.
pub fn verify_outcome<V: Vfs>(
    vfs: &V,
    source: &Path,
    target: &Path,
    expected_size: Option<u64>,
) -> OpOutcome {
    let source_present = vfs.exists(source);
    let target_present = vfs.exists(target);
    match (source_present, target_present) {
        (false, true) => match expected_size {
            // Same-volume rename/move: existence is decisive (atomic no-clobber).
            None => OpOutcome::Completed,
            // Cross-volume copy: the target must be the full snapshot size, or it
            // is a partial copy with the source already gone - ambiguous, offer
            // rollback rather than trusting a short target.
            Some(size) => match vfs.metadata(target) {
                Ok(m) if m.size == size => OpOutcome::Completed,
                _ => OpOutcome::Ambiguous,
            },
        },
        // Source still there, target not: the op never landed. Resume from it.
        (true, false) => OpOutcome::NotStarted,
        // Both present (copy done, source-delete pending) or neither present (the
        // item vanished): never auto-resume - record failed and offer rollback.
        _ => OpOutcome::Ambiguous,
    }
}

/// Classify the on-disk outcome of an in-doubt op of ANY kind, dispatching on
/// `op.kind` exactly as the executor's own dispatch does, so the reconciler asks
/// precisely the question the executor would have answered next:
/// - `no-op` changes nothing on disk, so it is trivially
///   [`Completed`](OpOutcome::Completed);
/// - `mkdir` creates `op.target_path` (idempotent `create_dir_all`): present is
///   `Completed`, absent is `NotStarted` - never ambiguous, because a directory
///   either exists or does not and re-running the create is safe either way;
/// - `rmdir-empty` removes `op.source_path`: absent is `Completed`, present is
///   `NotStarted`;
/// - `move`/`rename`/`quarantine` is a source-to-target move, delegated to
///   [`verify_outcome`] with `expected_size = None` for a same-volume rename or
///   `Some(op.byte_size)` for a cross-volume copy - the same split
///   [`crate::paths::same_volume`] drives inside the executor;
/// - any other kind cannot be reconciled and is
///   [`Ambiguous`](OpOutcome::Ambiguous): offer rollback, never auto-resume a kind
///   the reconciler does not model.
pub fn classify_op_outcome<V: Vfs>(vfs: &V, op: &PlanOpRow) -> OpOutcome {
    match op.kind.as_str() {
        "no-op" => OpOutcome::Completed,
        "mkdir" => {
            if vfs.exists(Path::new(&op.target_path)) {
                OpOutcome::Completed
            } else {
                OpOutcome::NotStarted
            }
        }
        "rmdir-empty" => {
            if vfs.exists(Path::new(&op.source_path)) {
                OpOutcome::NotStarted
            } else {
                OpOutcome::Completed
            }
        }
        "move" | "rename" | "quarantine" => {
            let source = Path::new(&op.source_path);
            let target = Path::new(&op.target_path);
            let expected_size = if crate::paths::same_volume(source, target) {
                None
            } else {
                Some(op.byte_size as u64)
            };
            verify_outcome(vfs, source, target, expected_size)
        }
        _ => OpOutcome::Ambiguous,
    }
}

/// The result of the startup reconciliation pass over one interrupted apply job
/// (F-606). The caller turns this into the resume-or-rollback choice the shell
/// shows, or - when nothing was in doubt - marks the stranded job interrupted the
/// ordinary way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ReconcileResult {
    /// The apply job this pass inspected.
    pub job_id: i64,
    /// Whether an in-doubt op was found and its terminal row repaired. `false`
    /// means the job left nothing to recover: a clean finish, or the FD-33
    /// lost-WAL-tail case.
    pub interrupted: bool,
    /// The verified outcome of the single in-doubt op, when one was found. `None`
    /// when nothing was in doubt, or when more than one in-doubt row was present (a
    /// safety abort that never auto-repairs).
    pub outcome: Option<OpOutcome>,
    /// The in-doubt op's id, for the shell's "Show file details" disclosure.
    pub in_doubt_op_id: Option<i64>,
    /// Whether resume is a SAFE option to offer alongside rollback: `true` for a
    /// decisive `Completed`/`NotStarted`, `false` when the state is ambiguous or the
    /// journal held more than one in-doubt row (rollback only).
    pub resume_offered: bool,
    /// Count of ops with a committed `done` terminal row after reconciliation - the
    /// floor a resume continues from.
    pub done_count: i64,
}

/// Reconcile one apply job found stranded (`state = 'running'`) at startup (F-606):
/// find its single in-doubt op, verify the real on-disk outcome through `vfs`,
/// repair the journal by writing the terminal row the kill prevented, and report
/// what a human can do next.
///
/// Writes AT MOST ONE journal row (the terminal row for the in-doubt op), preserving
/// the append-only, every-intent-has-a-terminal-row invariant the rest of the system
/// relies on. Never touches the filesystem - it only reads it, through `vfs`, to
/// classify the outcome.
pub async fn reconcile_interrupted_job<V: Vfs>(
    pool: &SqlitePool,
    vfs: &V,
    job_id: i64,
    now: &str,
) -> Result<ReconcileResult, AppError> {
    let in_doubt = query_in_doubt(pool, job_id).await?;
    let done_count = count_done(pool, job_id).await?;

    // More than one in-doubt row violates the single-writer invariant (a corrupt or
    // hand-edited journal). Never auto-repair or auto-resume: surface as interrupted,
    // rollback only.
    if in_doubt.len() > 1 {
        return Ok(ReconcileResult {
            job_id,
            interrupted: true,
            outcome: None,
            in_doubt_op_id: None,
            resume_offered: false,
            done_count,
        });
    }

    // Nothing in doubt: a clean finish, or the FD-33 lost-WAL-tail case. Nothing to
    // repair; the caller marks the stranded job interrupted the ordinary way.
    let Some(entry) = in_doubt.into_iter().next() else {
        return Ok(ReconcileResult {
            job_id,
            interrupted: false,
            outcome: None,
            in_doubt_op_id: None,
            resume_offered: false,
            done_count,
        });
    };

    // Look the op up in the plan to learn its kind, paths, and snapshot size. A
    // missing op (should be impossible) cannot be verified, so it is ambiguous.
    let op = crate::db::plans::get_plan_op(pool, entry.op_id)
        .await
        .map_err(|e| AppError::ReconcileFailed {
            detail: e.to_string(),
        })?;
    let outcome = match &op {
        Some(op) => classify_op_outcome(vfs, op),
        None => OpOutcome::Ambiguous,
    };

    // Repair the journal: write the terminal row the kill prevented. Completed ->
    // `done` (resume from the NEXT op, AC-5); NotStarted or Ambiguous -> `failed`
    // (resume from THIS op for NotStarted per AC-4; rollback only for Ambiguous).
    let (phase, detail) = match outcome {
        OpOutcome::Completed => (JournalPhase::Done, None),
        OpOutcome::NotStarted => (
            JournalPhase::Failed,
            Some(reconcile_detail("op not started")),
        ),
        OpOutcome::Ambiguous => (
            JournalPhase::Failed,
            Some(reconcile_detail("ambiguous on-disk state")),
        ),
    };
    let terminal = JournalEntry {
        job_id: entry.job_id,
        seq: entry.seq,
        op_id: entry.op_id,
        phase,
        at: now.to_string(),
        detail_json: detail,
    };
    let journal = SqliteJournal::new(pool.clone());
    match phase {
        JournalPhase::Done => journal.write_done(&terminal).await?,
        _ => journal.write_failed(&terminal).await?,
    };

    let done_count = if outcome == OpOutcome::Completed {
        done_count + 1
    } else {
        done_count
    };
    let resume_offered = matches!(outcome, OpOutcome::Completed | OpOutcome::NotStarted);

    Ok(ReconcileResult {
        job_id,
        interrupted: true,
        outcome: Some(outcome),
        in_doubt_op_id: Some(entry.op_id),
        resume_offered,
        done_count,
    })
}

/// Count the ops with a committed `done` terminal row for `job_id` - the resume
/// floor a `Completed` reconciliation adds one to.
async fn count_done(pool: &SqlitePool, job_id: i64) -> Result<i64, AppError> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM journal WHERE job_id = ? AND phase = 'done'")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::ReconcileFailed {
            detail: e.to_string(),
        })?;
    Ok(row.get::<i64, _>("n"))
}

/// A compact JSON detail for a reconcile-written `failed` row, recording WHY the
/// reconciler closed the intent (distinct from a walk-time failure code). `reason`
/// is always a fixed, quote-free literal from this module.
fn reconcile_detail(reason: &str) -> String {
    format!(r#"{{"reconcile":"{reason}"}}"#)
}

/// Reconcile every stranded apply job at startup (F-606): a `running` apply
/// `jobs` row a prior session left behind (killed mid-apply) has its single
/// in-doubt op's real on-disk outcome verified and its journal repaired, and the
/// first interruption worth surfacing is returned for the shell to offer
/// resume-or-rollback. At most one apply is ever in flight (single-writer), so in
/// practice this reconciles zero or one job. Reads the real filesystem through
/// `vfs`; never mutates it.
pub async fn reconcile_stranded_apply_jobs<V: Vfs>(
    pool: &SqlitePool,
    vfs: &V,
    now: &str,
) -> Result<Option<ReconcileResult>, AppError> {
    let mut surfaced = None;
    for job_id in stranded_apply_job_ids(pool).await? {
        let result = reconcile_interrupted_job(pool, vfs, job_id, now).await?;
        if result.interrupted && surfaced.is_none() {
            surfaced = Some(result);
        }
    }
    Ok(surfaced)
}

/// The ids of apply jobs still marked `running` - stranded by a prior session's
/// kill (the single-writer invariant means there is normally zero or one).
async fn stranded_apply_job_ids(pool: &SqlitePool) -> Result<Vec<i64>, AppError> {
    let rows =
        sqlx::query("SELECT id FROM jobs WHERE kind = 'apply' AND state = 'running' ORDER BY id")
            .fetch_all(pool)
            .await
            .map_err(|e| AppError::ReconcileFailed {
                detail: e.to_string(),
            })?;
    Ok(rows.into_iter().map(|r| r.get::<i64, _>("id")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::db::plans::PlanOpRow;
    use crate::exec::{Journal, JournalEntry, JournalPhase, MemFs, SeedEntry, SqliteJournal};
    use std::path::Path;
    use tempfile::TempDir;

    /// A fresh migrated database with the one `jobs` row the journal's `job_id`
    /// foreign key needs, returning the pool and that job id.
    async fn fresh_pool_and_job() -> (TempDir, SqlitePool, i64) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        let result = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply', 'running', ?)",
        )
        .bind("2026-07-22T00:00:00Z")
        .execute(&pool)
        .await
        .expect("insert jobs row");
        (dir, pool, result.last_insert_rowid())
    }

    fn entry(job_id: i64, seq: i64, op_id: i64, phase: JournalPhase) -> JournalEntry {
        JournalEntry {
            job_id,
            seq,
            op_id,
            phase,
            at: "2026-07-22T00:00:00Z".to_string(),
            detail_json: None,
        }
    }

    fn mem(files: &[(&str, u64, bool)]) -> MemFs {
        MemFs::from_seed(
            &files
                .iter()
                .map(|(p, s, d)| SeedEntry {
                    path: p.to_string(),
                    size: *s,
                    is_dir: *d,
                })
                .collect::<Vec<_>>(),
        )
    }

    /// A minimal in-memory `PlanOpRow` for classifier tests: only the fields
    /// `classify_op_outcome` reads (`kind`, `source_path`, `target_path`,
    /// `byte_size`) vary; the rest take valid placeholder values.
    fn plan_op(kind: &str, source: &str, target: &str, byte_size: i64) -> PlanOpRow {
        PlanOpRow {
            id: 1,
            plan_id: 1,
            seq: 0,
            op_group: "loose".to_string(),
            kind: kind.to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: String::new(),
            rule_id: String::new(),
            confidence: "high".to_string(),
            byte_size,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    // ---- classify_op_outcome, per op kind ----

    #[test]
    fn classify_no_op_is_always_completed() {
        let fs = MemFs::new();
        assert_eq!(
            classify_op_outcome(&fs, &plan_op("no-op", "", "", 0)),
            OpOutcome::Completed
        );
    }

    #[test]
    fn classify_mkdir_present_is_completed_absent_is_not_started() {
        let op = plan_op("mkdir", "", r"E:\lib\Author\Title", 0);
        let present = mem(&[(r"E:\lib\Author\Title", 0, true)]);
        assert_eq!(classify_op_outcome(&present, &op), OpOutcome::Completed);
        assert_eq!(
            classify_op_outcome(&MemFs::new(), &op),
            OpOutcome::NotStarted
        );
    }

    #[test]
    fn classify_rmdir_empty_gone_is_completed_present_is_not_started() {
        let op = plan_op("rmdir-empty", r"E:\lib\Empty", "", 0);
        assert_eq!(
            classify_op_outcome(&MemFs::new(), &op),
            OpOutcome::Completed
        );
        let present = mem(&[(r"E:\lib\Empty", 0, true)]);
        assert_eq!(classify_op_outcome(&present, &op), OpOutcome::NotStarted);
    }

    #[test]
    fn classify_same_volume_move_uses_existence() {
        // Same drive letter routes to rename semantics (existence is decisive).
        let op = plan_op("move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100);
        let landed = mem(&[(r"E:\lib\New\B.m4b", 100, false)]);
        assert_eq!(classify_op_outcome(&landed, &op), OpOutcome::Completed);
        let not_yet = mem(&[(r"E:\lib\Old\B.m4b", 100, false)]);
        assert_eq!(classify_op_outcome(&not_yet, &op), OpOutcome::NotStarted);
    }

    #[test]
    fn classify_cross_volume_move_uses_size() {
        // Different drive letters route to copy semantics (target must be full size).
        let op = plan_op("quarantine", r"E:\lib\B.m4b", r"F:\aside\B.m4b", 500);
        let full = mem(&[(r"F:\aside\B.m4b", 500, false)]);
        assert_eq!(classify_op_outcome(&full, &op), OpOutcome::Completed);
        let short = mem(&[(r"F:\aside\B.m4b", 200, false)]);
        assert_eq!(classify_op_outcome(&short, &op), OpOutcome::Ambiguous);
    }

    #[test]
    fn classify_unknown_kind_is_ambiguous() {
        let op = plan_op("teleport", r"E:\a", r"E:\b", 0);
        assert_eq!(
            classify_op_outcome(&MemFs::new(), &op),
            OpOutcome::Ambiguous
        );
    }

    // ---- query_in_doubt (AC-1) ----

    /// An op with an `intent` and a `done` is settled; an op with only an `intent`
    /// is the single in-doubt row (AC-1).
    #[tokio::test]
    async fn query_in_doubt_finds_the_intent_without_a_terminal_row() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let j = SqliteJournal::new(pool.clone());
        // op 1: intent + done (settled).
        j.write_intent(&entry(job, 1, 1, JournalPhase::Intent))
            .await
            .unwrap();
        j.write_done(&entry(job, 1, 1, JournalPhase::Done))
            .await
            .unwrap();
        // op 2: intent only (in doubt).
        j.write_intent(&entry(job, 2, 2, JournalPhase::Intent))
            .await
            .unwrap();

        let in_doubt = query_in_doubt(&pool, job).await.unwrap();
        assert_eq!(in_doubt.len(), 1, "exactly one op is in doubt");
        assert_eq!(in_doubt[0].op_id, 2);
        assert_eq!(in_doubt[0].seq, 2);
        pool.close().await;
    }

    /// A `failed` terminal row settles an op just as a `done` does - it is no
    /// longer in doubt.
    #[tokio::test]
    async fn a_failed_terminal_row_settles_the_op() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 1, 1, JournalPhase::Intent))
            .await
            .unwrap();
        j.write_failed(&entry(job, 1, 1, JournalPhase::Failed))
            .await
            .unwrap();
        assert!(query_in_doubt(&pool, job).await.unwrap().is_empty());
        pool.close().await;
    }

    /// A fully settled job (every intent has a terminal row) leaves nothing in
    /// doubt - the clean-finish and FD-33 lost-tail case both surface as empty.
    #[tokio::test]
    async fn a_fully_settled_job_has_nothing_in_doubt() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let j = SqliteJournal::new(pool.clone());
        for op in 1..=3 {
            j.write_intent(&entry(job, op, op, JournalPhase::Intent))
                .await
                .unwrap();
            j.write_done(&entry(job, op, op, JournalPhase::Done))
                .await
                .unwrap();
        }
        assert!(query_in_doubt(&pool, job).await.unwrap().is_empty());
        pool.close().await;
    }

    /// The single-writer invariant should prevent two in-doubt ops, but if the
    /// journal ever holds them the query must SURFACE both so the reconciler can
    /// safety-abort rather than silently pick one.
    #[tokio::test]
    async fn multiple_in_doubt_rows_are_all_surfaced_for_a_safety_abort() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 1, 1, JournalPhase::Intent))
            .await
            .unwrap();
        j.write_intent(&entry(job, 2, 2, JournalPhase::Intent))
            .await
            .unwrap();
        let in_doubt = query_in_doubt(&pool, job).await.unwrap();
        assert_eq!(in_doubt.len(), 2);
        // Ordered most-recent-first.
        assert_eq!(in_doubt[0].op_id, 2);
        assert_eq!(in_doubt[1].op_id, 1);
        pool.close().await;
    }

    /// The query is scoped to its `job_id`: another job's in-doubt op is invisible.
    #[tokio::test]
    async fn query_in_doubt_is_scoped_to_its_job() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let other = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply', 'running', ?)",
        )
        .bind("2026-07-22T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(other, 1, 1, JournalPhase::Intent))
            .await
            .unwrap();
        assert!(
            query_in_doubt(&pool, job).await.unwrap().is_empty(),
            "job under test has no in-doubt op of its own"
        );
        assert_eq!(query_in_doubt(&pool, other).await.unwrap().len(), 1);
        pool.close().await;
    }

    // ---- verify_outcome, same-volume rename (AC-2) ----

    #[test]
    fn rename_target_present_source_gone_is_completed() {
        let fs = mem(&[(r"E:\lib\New\Book.m4b", 100, false)]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Old\Book.m4b"),
                Path::new(r"E:\lib\New\Book.m4b"),
                None,
            ),
            OpOutcome::Completed
        );
    }

    #[test]
    fn rename_source_present_target_absent_is_not_started() {
        let fs = mem(&[(r"E:\lib\Old\Book.m4b", 100, false)]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Old\Book.m4b"),
                Path::new(r"E:\lib\New\Book.m4b"),
                None,
            ),
            OpOutcome::NotStarted
        );
    }

    #[test]
    fn rename_both_present_is_ambiguous() {
        let fs = mem(&[
            (r"E:\lib\Old\Book.m4b", 100, false),
            (r"E:\lib\New\Book.m4b", 100, false),
        ]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Old\Book.m4b"),
                Path::new(r"E:\lib\New\Book.m4b"),
                None,
            ),
            OpOutcome::Ambiguous
        );
    }

    #[test]
    fn rename_neither_present_is_ambiguous() {
        let fs = MemFs::new();
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Old\Book.m4b"),
                Path::new(r"E:\lib\New\Book.m4b"),
                None,
            ),
            OpOutcome::Ambiguous
        );
    }

    // ---- verify_outcome, cross-volume copy+verify+delete (AC-3) ----

    #[test]
    fn copy_target_full_size_source_gone_is_completed() {
        let fs = mem(&[(r"F:\lib\Book.m4b", 500, false)]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Book.m4b"),
                Path::new(r"F:\lib\Book.m4b"),
                Some(500),
            ),
            OpOutcome::Completed
        );
    }

    #[test]
    fn copy_target_short_size_is_ambiguous() {
        // A partial copy (200 of the expected 500) with the source already gone.
        let fs = mem(&[(r"F:\lib\Book.m4b", 200, false)]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Book.m4b"),
                Path::new(r"F:\lib\Book.m4b"),
                Some(500),
            ),
            OpOutcome::Ambiguous
        );
    }

    #[test]
    fn copy_both_present_is_ambiguous() {
        // Copy landed at full size but the source-delete had not run yet.
        let fs = mem(&[
            (r"E:\lib\Book.m4b", 500, false),
            (r"F:\lib\Book.m4b", 500, false),
        ]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Book.m4b"),
                Path::new(r"F:\lib\Book.m4b"),
                Some(500),
            ),
            OpOutcome::Ambiguous
        );
    }

    #[test]
    fn copy_source_present_target_absent_is_not_started() {
        let fs = mem(&[(r"E:\lib\Book.m4b", 500, false)]);
        assert_eq!(
            verify_outcome(
                &fs,
                Path::new(r"E:\lib\Book.m4b"),
                Path::new(r"F:\lib\Book.m4b"),
                Some(500),
            ),
            OpOutcome::NotStarted
        );
    }

    // ---- reconcile_interrupted_job, end to end (AC-4, AC-5) ----

    const NOW: &str = "2026-07-22T00:00:00Z";

    /// Seed the FK chain (scan + ruleset + plan) with a single op of `kind`, and
    /// return that op's id (which the journal's `op_id` references).
    async fn seed_plan_op(
        pool: &SqlitePool,
        kind: &str,
        source: &str,
        target: &str,
        byte_size: i64,
    ) -> i64 {
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
                name: "d",
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
                kind,
                kind_reason: None,
                source_path: source,
                target_path: target,
                rationale: "op.",
                rule_id: "test-rule",
                confidence: "high",
                byte_size,
                validation_state: "valid",
                validation_reason: None,
                provenance_json: None,
            }],
            NOW,
        )
        .await
        .expect("insert plan");
        crate::db::plans::get_plan_ops(pool, plan_id)
            .await
            .expect("ops")[0]
            .id
    }

    /// AC-5: the op provably completed on disk -> `done` written, resume from the
    /// next op, nothing left in doubt.
    #[tokio::test]
    async fn reconcile_completed_move_writes_done_and_offers_resume() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        // Target landed, source gone: the same-volume rename completed.
        let fs = mem(&[(r"E:\lib\New\B.m4b", 100, false)]);

        let result = reconcile_interrupted_job(&pool, &fs, job, NOW)
            .await
            .unwrap();
        assert!(result.interrupted);
        assert_eq!(result.outcome, Some(OpOutcome::Completed));
        assert!(result.resume_offered);
        assert_eq!(result.done_count, 1);
        assert!(
            query_in_doubt(&pool, job).await.unwrap().is_empty(),
            "the journal is repaired: the intent now has a terminal row"
        );
        pool.close().await;
    }

    /// AC-4: the op never started -> `failed` written, resume from this op offered.
    #[tokio::test]
    async fn reconcile_not_started_move_writes_failed_and_offers_resume() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        // Source still present, target absent: the op never landed.
        let fs = mem(&[(r"E:\lib\Old\B.m4b", 100, false)]);

        let result = reconcile_interrupted_job(&pool, &fs, job, NOW)
            .await
            .unwrap();
        assert!(result.interrupted);
        assert_eq!(result.outcome, Some(OpOutcome::NotStarted));
        assert!(result.resume_offered);
        assert_eq!(result.done_count, 0);
        assert!(query_in_doubt(&pool, job).await.unwrap().is_empty());
        pool.close().await;
    }

    /// An ambiguous on-disk state -> `failed` written, rollback ONLY (no resume).
    #[tokio::test]
    async fn reconcile_ambiguous_move_offers_rollback_only() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        // Both present: ambiguous.
        let fs = mem(&[
            (r"E:\lib\Old\B.m4b", 100, false),
            (r"E:\lib\New\B.m4b", 100, false),
        ]);

        let result = reconcile_interrupted_job(&pool, &fs, job, NOW)
            .await
            .unwrap();
        assert!(result.interrupted);
        assert_eq!(result.outcome, Some(OpOutcome::Ambiguous));
        assert!(!result.resume_offered, "ambiguous offers rollback only");
        assert!(query_in_doubt(&pool, job).await.unwrap().is_empty());
        pool.close().await;
    }

    /// A job whose ops are all settled leaves nothing in doubt: not interrupted, no
    /// row written, the done floor reflects the committed dones.
    #[tokio::test]
    async fn reconcile_clean_job_is_not_interrupted() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        j.write_done(&entry(job, 0, op_id, JournalPhase::Done))
            .await
            .unwrap();

        let result = reconcile_interrupted_job(&pool, &MemFs::new(), job, NOW)
            .await
            .unwrap();
        assert!(!result.interrupted);
        assert_eq!(result.outcome, None);
        assert!(!result.resume_offered);
        assert_eq!(result.done_count, 1);
        pool.close().await;
    }

    // ---- reconcile_stranded_apply_jobs (startup sweep) ----

    /// The startup sweep reconciles a stranded running apply job with an in-doubt
    /// op and surfaces its interruption.
    #[tokio::test]
    async fn stranded_sweep_surfaces_an_interrupted_job() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        let fs = mem(&[(r"E:\lib\New\B.m4b", 100, false)]);

        let surfaced = reconcile_stranded_apply_jobs(&pool, &fs, NOW)
            .await
            .unwrap();
        let surfaced = surfaced.expect("an interrupted job is surfaced");
        assert_eq!(surfaced.job_id, job);
        assert_eq!(surfaced.outcome, Some(OpOutcome::Completed));
        assert!(surfaced.resume_offered);
        pool.close().await;
    }

    /// With no in-doubt op, the sweep surfaces nothing (a clean finish, or the
    /// FD-33 lost tail left nothing to recover).
    #[tokio::test]
    async fn stranded_sweep_surfaces_nothing_when_no_op_is_in_doubt() {
        let (_dir, pool, job) = fresh_pool_and_job().await;
        let op_id =
            seed_plan_op(&pool, "move", r"E:\lib\Old\B.m4b", r"E:\lib\New\B.m4b", 100).await;
        let j = SqliteJournal::new(pool.clone());
        j.write_intent(&entry(job, 0, op_id, JournalPhase::Intent))
            .await
            .unwrap();
        j.write_done(&entry(job, 0, op_id, JournalPhase::Done))
            .await
            .unwrap();

        let surfaced = reconcile_stranded_apply_jobs(&pool, &MemFs::new(), NOW)
            .await
            .unwrap();
        assert!(surfaced.is_none());
        pool.close().await;
    }

    /// `ReconcileResult` is an IPC type: it round-trips through serde and its nested
    /// `OpOutcome` serializes kebab-case on the wire.
    #[test]
    fn reconcile_result_round_trips_through_serde() {
        let r = ReconcileResult {
            job_id: 7,
            interrupted: true,
            outcome: Some(OpOutcome::NotStarted),
            in_doubt_op_id: Some(42),
            resume_offered: true,
            done_count: 3,
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert!(
            json.contains("not-started"),
            "OpOutcome is kebab-case on the wire"
        );
        let back: ReconcileResult = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, r);
    }
}
