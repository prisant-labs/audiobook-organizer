//! F-1001 (activity log): an append-only record of every scan, CSV import, and
//! classify run, with its parameters and outcome, in the `activity_records`
//! table (migration 0001).
//!
//! v0.2.0 Phase 6, the smallest phase of this release: one helper,
//! [`append_activity`], plus a hook at each of scan, CSV import, and classify's
//! entry points ([`crate::scan::run_scan_with_job`], [`crate::scan::run_csv_import`],
//! and [`crate::classify::run_classify`]). Every hooked entry point wraps its
//! existing (unchanged) body and appends exactly one row after it returns,
//! whatever the result - success, a cooperative cancel (FD-02, scan only), or
//! a failure. A failure row always carries the [`AppError::code`] machine code
//! (AC-1001.2), never a bare "failed" with no reason.
//!
//! **Parse has no standalone entry point in this release.** F-301..304 run as
//! a pure in-memory step INSIDE classify's merge
//! ([`crate::parse::extract::extract`], called from
//! [`crate::classify::engine::classify`]) - there is no separate DB-facing
//! "run parse" function to hook. The classify activity record therefore covers
//! both: logging classify already logs the parse work classify performs as
//! part of producing its output. See [`crate::classify::run_classify`]'s doc
//! for the exact call chain.
//!
//! **Append-only.** [`append_activity`] only ever INSERTs; nothing in this
//! module or its callers issues an UPDATE or DELETE against
//! `activity_records` in normal operation, so a prior row is never mutated
//! (the other half of AC-1001.2).
//!
//! **No new dependency.** `params_json` is built with [`json_object`], a tiny
//! hand-rolled string-pair JSON object, rather than pulling `serde_json` into
//! the production build: `serde_json` is a dev-dependency only in this crate
//! (see `Cargo.toml`), matching the crate's existing posture of hand-rolling
//! small, stable serializations (e.g. the ISO-8601 timestamps in
//! `crate::scan::walk`) instead of adding a dependency for them.

use sqlx::SqlitePool;

use crate::error::AppError;
use crate::scan::walk::now_iso8601_utc;

/// The recorded outcome of one logged run.
///
/// `Succeeded` and `Cancelled` are both designed, non-failure outcomes (a
/// cooperative cancel at a safe boundary, FD-02, is not an error); only
/// `Failed` carries a machine error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityOutcome {
    /// The run completed normally.
    Succeeded,
    /// The run was cancelled at a safe boundary (scan only, this release).
    Cancelled,
    /// The run failed; `error_code` is the [`AppError::code`] machine code.
    Failed { error_code: String },
}

impl ActivityOutcome {
    /// The literal `outcome` column text. A failure's shape is
    /// `"failed:<error-code>"`, so the machine code round-trips out of the
    /// plain-text column with a simple prefix strip, no JSON decode needed.
    fn as_column(&self) -> String {
        match self {
            ActivityOutcome::Succeeded => "succeeded".to_string(),
            ActivityOutcome::Cancelled => "cancelled".to_string(),
            ActivityOutcome::Failed { error_code } => format!("failed:{error_code}"),
        }
    }

    /// Derive the outcome to log from a fallible run's result: `Ok` always
    /// maps to [`Succeeded`](ActivityOutcome::Succeeded) here (callers with a
    /// distinct non-error outcome, like a scan cancel, build
    /// [`Cancelled`](ActivityOutcome::Cancelled) directly instead of calling
    /// this).
    pub fn from_result<T>(result: &Result<T, AppError>) -> Self {
        match result {
            Ok(_) => ActivityOutcome::Succeeded,
            Err(e) => ActivityOutcome::Failed {
                error_code: e.code().to_string(),
            },
        }
    }
}

/// Append one `activity_records` row: `action`, `params_json`, the outcome
/// column, and a fresh `created_at` timestamp.
///
/// Best-effort by design: a logging failure (a rare DB write error) is
/// reported via `tracing::error!` and swallowed rather than propagated, so a
/// broken audit trail never masks or blocks the primary operation's own
/// result - the same posture `crate::scan::persist`'s `mark_scan_failed` /
/// `mark_scan_cancelled` already use for their own best-effort status writes.
pub async fn append_activity(
    pool: &SqlitePool,
    action: &str,
    params_json: &str,
    outcome: &ActivityOutcome,
) {
    let created_at = now_iso8601_utc();
    let result = sqlx::query(
        "INSERT INTO activity_records (action, params_json, outcome, created_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(action)
    .bind(params_json)
    .bind(outcome.as_column())
    .bind(&created_at)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(
            action,
            error = %e,
            "activity log: failed to append record"
        );
    }
}

/// Hand-build a minimal JSON object (string keys and values only) from
/// `(key, value)` pairs, escaping quotes/backslashes/control characters in
/// both. See the module doc for why this avoids a `serde_json` dependency.
pub fn json_object(fields: &[(&str, &str)]) -> String {
    let mut out = String::from("{");
    for (i, (k, v)) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        json_escape_into(k, &mut out);
        out.push_str("\":\"");
        json_escape_into(v, &mut out);
        out.push('"');
    }
    out.push('}');
    out
}

/// Append `s` to `out` with the minimal JSON string escaping needed for a
/// well-formed double-quoted JSON string value: `"`, `\`, the common control
/// escapes, and a `\u00XX` fallback for any other control character.
fn json_escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use sqlx::Row;
    use tempfile::TempDir;

    /// A fresh, migrated pool in its own temp dir (kept alive by the returned
    /// `TempDir`), matching the convention already used across `db`/`scan`.
    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    async fn all_rows(pool: &SqlitePool) -> Vec<(String, String, String, String)> {
        sqlx::query(
            "SELECT action, params_json, outcome, created_at FROM activity_records ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .expect("fetch activity_records")
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("action"),
                row.get::<Option<String>, _>("params_json")
                    .unwrap_or_default(),
                row.get::<Option<String>, _>("outcome").unwrap_or_default(),
                row.get::<String, _>("created_at"),
            )
        })
        .collect()
    }

    /// One `append_activity` call appends exactly one row, carrying the given
    /// action, params, and outcome (AC-1001.1).
    #[tokio::test]
    async fn append_activity_writes_one_row() {
        let (_db, pool) = fresh_pool().await;

        append_activity(
            &pool,
            "scan",
            &json_object(&[("root", "E:\\Books - Audio")]),
            &ActivityOutcome::Succeeded,
        )
        .await;

        let rows = all_rows(&pool).await;
        assert_eq!(rows.len(), 1, "exactly one row per run");
        let (action, params, outcome, created_at) = &rows[0];
        assert_eq!(action, "scan");
        assert_eq!(params, r#"{"root":"E:\\Books - Audio"}"#);
        assert_eq!(outcome, "succeeded");
        assert!(!created_at.is_empty());
    }

    /// A failed run's outcome column carries the machine error code, not just
    /// a bare "failed" (AC-1001.2).
    #[tokio::test]
    async fn failed_run_records_error_code() {
        let (_db, pool) = fresh_pool().await;

        let outcome = ActivityOutcome::Failed {
            error_code: "root-not-found".to_string(),
        };
        append_activity(&pool, "scan", "{}", &outcome).await;

        let rows = all_rows(&pool).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, "failed:root-not-found");
    }

    /// `from_result` derives `Succeeded` from `Ok` and `Failed{error_code}`
    /// from `Err`, carrying the exact `AppError::code()`.
    #[test]
    fn from_result_derives_outcome() {
        let ok: Result<(), AppError> = Ok(());
        assert_eq!(
            ActivityOutcome::from_result(&ok),
            ActivityOutcome::Succeeded
        );

        let err: Result<(), AppError> = Err(AppError::CsvParse { row: 3 });
        assert_eq!(
            ActivityOutcome::from_result(&err),
            ActivityOutcome::Failed {
                error_code: "csv-parse".to_string()
            }
        );
    }

    /// Append-only: appending a second row never mutates the first (AC-1001.2).
    #[tokio::test]
    async fn prior_rows_are_never_mutated() {
        let (_db, pool) = fresh_pool().await;

        append_activity(
            &pool,
            "scan",
            &json_object(&[("root", "E:\\Lib")]),
            &ActivityOutcome::Succeeded,
        )
        .await;
        let first_snapshot = all_rows(&pool).await;

        append_activity(
            &pool,
            "csv-import",
            &json_object(&[("csv_path", "E:\\import.csv")]),
            &ActivityOutcome::Failed {
                error_code: "csv-parse".to_string(),
            },
        )
        .await;

        let rows = all_rows(&pool).await;
        assert_eq!(rows.len(), 2, "one row per append, nothing collapsed");
        assert_eq!(
            rows[0], first_snapshot[0],
            "the first row is byte-identical after a second append"
        );
        assert_eq!(rows[1].0, "csv-import");
        assert_eq!(rows[1].2, "failed:csv-parse");
    }

    #[test]
    fn json_object_escapes_quotes_and_backslashes() {
        let s = json_object(&[("path", "C:\\a \"weird\" name")]);
        assert_eq!(s, r#"{"path":"C:\\a \"weird\" name"}"#);
    }
}
