//! F-602 (journal-before-act): the append-only journal seam.
//!
//! The whole point of the safety spine is that nothing touches the filesystem
//! before an `intent` row is flushed and committed (R-5, AC-10). This module is
//! the seam that makes that structural: the executor walk (see [`super::Executor`])
//! reaches the filesystem only AFTER calling [`Journal::write_intent`], and a
//! failed intent flush is a hard stop ([`AppError::JournalWriteFailed`], AC-13).
//!
//! # A seam, exactly like [`Vfs`](super::Vfs)
//!
//! [`Journal`] is a trait with two implementations, mirroring the `Vfs` seam so
//! the same walk serves production and tests:
//! - [`SqliteJournal`] - the production writer. Each write is a single, immediately
//!   committed `INSERT` into the `journal` table (migration 0005). Under WAL a
//!   committed single-statement insert is durable, so "flush" and "the write
//!   returned `Ok`" are the same instant: when [`write_intent`](Journal::write_intent)
//!   returns, the intent row survives a process kill.
//! - [`MemJournal`] - the in-memory test writer. It collects entries in a `Mutex`
//!   so the executor's pure walk tests assert on the produced rows without a
//!   database, and it can inject an intent-flush failure to exercise the AC-13
//!   hard stop.
//!
//! # Append-only (AC-10)
//!
//! Every method here only ever `INSERT`s. There is deliberately no update or
//! delete path: an `intent` row with no terminal row is the exact evidence a
//! v0.6.0 reconciliation pass looks for, so it must never be mutated away.

use std::sync::Mutex;

use sqlx::SqlitePool;

use crate::error::AppError;

use super::JournalEntry;

/// The append-only journal seam (F-602). The executor calls [`write_intent`] and
/// flushes it BEFORE the filesystem call, then appends a terminal row after.
///
/// The methods are `async` because the production writer commits to SQLite. The
/// `async_fn_in_trait` lint (which warns that the returned futures' `Send`-ness is
/// unspecified for arbitrary implementors) is allowed here deliberately: the trait
/// is crate-internal, and the one place it is awaited across a thread boundary (the
/// `apply_start` command) monomorphizes it with the concrete, `Send` [`SqliteJournal`].
///
/// [`write_intent`]: Journal::write_intent
#[allow(async_fn_in_trait)]
pub trait Journal {
    /// Flush an `intent` row and do not return until it is committed (R-5,
    /// AC-10). A failure is a hard stop: the executor must not proceed to the
    /// filesystem call, so this returns [`AppError::JournalWriteFailed`] (AC-13).
    async fn write_intent(&self, entry: &JournalEntry) -> Result<(), AppError>;

    /// Append a `done` row after the operation succeeded.
    async fn write_done(&self, entry: &JournalEntry) -> Result<(), AppError>;

    /// Append a `failed` row after the operation failed (the `entry`'s
    /// `detail_json` carries the failure text). Defined and tested now; the
    /// executor's dispatch is an infallible skeleton this phase, so the walk
    /// itself only produces `intent` + `done` (the failed branch lands with the
    /// real operation logic in Phase 3).
    async fn write_failed(&self, entry: &JournalEntry) -> Result<(), AppError>;
}

/// The production [`Journal`]: append-only `INSERT`s into the `journal` table,
/// each its own committed transaction (a single `INSERT` on a pooled connection
/// in autocommit mode), so an `intent` row is durable the instant its write
/// returns.
///
/// # Durability boundary (v0.5.0)
///
/// `open_db` runs WAL with `synchronous = NORMAL`. Under that setting a committed
/// intent survives a PROCESS KILL (the app being killed mid-apply): the WAL frames
/// are already handed to the OS, which persists them regardless of the process
/// dying. Process-kill is exactly AC-10's threat, and the kill test proves the
/// intent row is still there after it. `synchronous = NORMAL` does NOT fsync the
/// WAL on every commit, so a POWER LOSS or OS crash between commit and the next
/// checkpoint could lose a just-committed intent. That harsher failure is out of
/// scope for v0.5.0 (whose AC-10 is process-kill); crash reconciliation is v0.6.0
/// (F-606), which is the right place to decide between a dedicated FULL-synchronous
/// journal connection and reconciliation that tolerates a lost tail. NORMAL is kept
/// here deliberately rather than forcing `synchronous = FULL` globally, which would
/// slow the unrelated scan write path. See the P2 report's durability note for the
/// v0.6.0 follow-up.
#[derive(Clone)]
pub struct SqliteJournal {
    pool: SqlitePool,
}

impl SqliteJournal {
    /// Build a journal writer over `pool`. The pool is cheap to clone (it is an
    /// `Arc` internally), so a caller can hand an owned clone into a spawned walk.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The one append path all three methods funnel through: a single, immediately
    /// committed `INSERT`. Any SQLite failure maps to
    /// [`AppError::JournalWriteFailed`] so the caller (the executor) treats an
    /// intent-flush failure as the AC-13 hard stop.
    async fn append(&self, entry: &JournalEntry) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO journal (job_id, seq, op_id, phase, at, detail_json) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(entry.job_id)
        .bind(entry.seq)
        .bind(entry.op_id)
        .bind(entry.phase.as_str())
        .bind(&entry.at)
        .bind(entry.detail_json.as_deref())
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::JournalWriteFailed {
            detail: e.to_string(),
        })?;
        Ok(())
    }
}

impl Journal for SqliteJournal {
    async fn write_intent(&self, entry: &JournalEntry) -> Result<(), AppError> {
        self.append(entry).await
    }

    async fn write_done(&self, entry: &JournalEntry) -> Result<(), AppError> {
        self.append(entry).await
    }

    async fn write_failed(&self, entry: &JournalEntry) -> Result<(), AppError> {
        self.append(entry).await
    }
}

/// The in-memory test [`Journal`]: collects every written entry in insertion
/// order, and can inject an intent-flush failure. Mirrors [`super::MemFs`] - it
/// lets the executor's pure walk tests run with no database.
#[derive(Default)]
pub struct MemJournal {
    entries: Mutex<Vec<JournalEntry>>,
    /// When `true`, [`write_intent`](Journal::write_intent) returns
    /// [`AppError::JournalWriteFailed`] without recording anything - the injected
    /// failure the AC-13 hard-stop test uses.
    fail_intent: bool,
}

impl MemJournal {
    /// A journal that records every write.
    pub fn new() -> Self {
        Self::default()
    }

    /// A journal whose intent flush always fails (AC-13 hard-stop injection).
    pub fn failing_intent() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            fail_intent: true,
        }
    }

    /// A snapshot copy of every entry written so far, in insertion order.
    pub fn entries(&self) -> Vec<JournalEntry> {
        self.entries
            .lock()
            .expect("journal mutex is never poisoned")
            .clone()
    }

    fn push(&self, entry: &JournalEntry) {
        self.entries
            .lock()
            .expect("journal mutex is never poisoned")
            .push(entry.clone());
    }
}

impl Journal for MemJournal {
    async fn write_intent(&self, entry: &JournalEntry) -> Result<(), AppError> {
        if self.fail_intent {
            return Err(AppError::JournalWriteFailed {
                detail: "injected intent-flush failure".to_string(),
            });
        }
        self.push(entry);
        Ok(())
    }

    async fn write_done(&self, entry: &JournalEntry) -> Result<(), AppError> {
        self.push(entry);
        Ok(())
    }

    async fn write_failed(&self, entry: &JournalEntry) -> Result<(), AppError> {
        self.push(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::exec::JournalPhase;
    use sqlx::Row;
    use tempfile::TempDir;

    /// Open a fresh migrated database and insert the one `jobs` row the journal's
    /// `job_id` foreign key needs, returning the pool and that job id.
    async fn fresh_pool_and_job() -> (TempDir, SqlitePool, i64) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        let result = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply', 'running', ?)",
        )
        .bind("2026-07-18T00:00:00Z")
        .execute(&pool)
        .await
        .expect("insert jobs row");
        (dir, pool, result.last_insert_rowid())
    }

    fn entry(job_id: i64, op_id: i64, phase: JournalPhase, detail: Option<&str>) -> JournalEntry {
        JournalEntry {
            job_id,
            seq: 0,
            op_id,
            phase,
            at: "2026-07-18T00:00:00Z".to_string(),
            detail_json: detail.map(|d| d.to_string()),
        }
    }

    /// SqliteJournal appends a row per write, storing every JournalEntry field
    /// one-to-one, and the phase is the kebab-case tag the CHECK constraint allows.
    #[tokio::test]
    async fn sqlite_journal_appends_each_phase_with_fields_one_to_one() {
        let (_dir, pool, job_id) = fresh_pool_and_job().await;
        let journal = SqliteJournal::new(pool.clone());

        journal
            .write_intent(&entry(
                job_id,
                1,
                JournalPhase::Intent,
                Some(r#"{"pack_name":"x"}"#),
            ))
            .await
            .expect("intent");
        journal
            .write_done(&entry(job_id, 1, JournalPhase::Done, None))
            .await
            .expect("done");
        journal
            .write_failed(&entry(
                job_id,
                2,
                JournalPhase::Failed,
                Some("disk fell over"),
            ))
            .await
            .expect("failed");

        let rows = sqlx::query(
            "SELECT job_id, seq, op_id, phase, at, detail_json FROM journal ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("select journal");
        assert_eq!(rows.len(), 3);

        assert_eq!(rows[0].get::<i64, _>("job_id"), job_id);
        assert_eq!(rows[0].get::<i64, _>("op_id"), 1);
        assert_eq!(rows[0].get::<String, _>("phase"), "intent");
        assert_eq!(rows[0].get::<String, _>("at"), "2026-07-18T00:00:00Z");
        assert_eq!(
            rows[0].get::<Option<String>, _>("detail_json").as_deref(),
            Some(r#"{"pack_name":"x"}"#)
        );

        assert_eq!(rows[1].get::<String, _>("phase"), "done");
        assert!(rows[1].get::<Option<String>, _>("detail_json").is_none());

        // The failed row carries the failure text in detail_json (Phase 3 fills it
        // from the real operation error; here it is asserted directly).
        assert_eq!(rows[2].get::<String, _>("phase"), "failed");
        assert_eq!(
            rows[2].get::<Option<String>, _>("detail_json").as_deref(),
            Some("disk fell over")
        );
    }

    /// A SQLite failure on the intent flush surfaces as journal-write-failed (the
    /// AC-13 code), proven by pointing the writer at a closed pool.
    #[tokio::test]
    async fn a_failed_flush_is_journal_write_failed() {
        let (_dir, pool, job_id) = fresh_pool_and_job().await;
        pool.close().await;
        let journal = SqliteJournal::new(pool);

        let err = journal
            .write_intent(&entry(job_id, 1, JournalPhase::Intent, None))
            .await
            .expect_err("a closed pool must fail the flush");
        assert_eq!(err.code(), "journal-write-failed");
    }

    /// MemJournal collects every write; the failing variant rejects the intent
    /// flush without recording anything (the hard-stop injection).
    #[tokio::test]
    async fn mem_journal_collects_and_can_inject_an_intent_failure() {
        let journal = MemJournal::new();
        journal
            .write_intent(&entry(7, 1, JournalPhase::Intent, None))
            .await
            .expect("intent");
        journal
            .write_done(&entry(7, 1, JournalPhase::Done, None))
            .await
            .expect("done");
        assert_eq!(journal.entries().len(), 2);

        let failing = MemJournal::failing_intent();
        let err = failing
            .write_intent(&entry(7, 1, JournalPhase::Intent, None))
            .await
            .expect_err("the failing variant rejects the intent flush");
        assert_eq!(err.code(), "journal-write-failed");
        assert!(
            failing.entries().is_empty(),
            "a rejected intent flush records nothing"
        );
    }
}
