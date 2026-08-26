//! F-1001 activity-log wrapper around the pure [`engine::classify`].
//!
//! This is the ONLY I/O-touching function in `crate::classify`: [`engine`],
//! [`crate::classify::metrics`], and [`crate::classify::multibook`] remain pure
//! per their own module docs, unchanged by this phase. [`run_classify`] exists
//! solely so a classify run appends one `activity_records` row via
//! [`crate::db::activity::append_activity`] - and, because [`engine::classify`]
//! calls [`crate::parse::extract::extract`] as an internal step (the F-303
//! merge it consumes while classifying), this same record is also the one
//! that covers a parse run in this release: parse (F-301..304) has no
//! standalone DB-facing entry point of its own (see
//! `crate::db::activity`'s module doc).
//!
//! [`engine::classify`] itself never fails - it returns a plain `Vec`, no
//! `Result` - so in practice every row this appends is
//! [`ActivityOutcome::Succeeded`](crate::db::activity::ActivityOutcome::Succeeded).
//! The `Result` return type is kept anyway so this wrapper is call-shape
//! symmetric with the scan/import ones, and so a future fallible extension to
//! classification (there is none planned this release) would not need a
//! signature change here.

use sqlx::SqlitePool;

use crate::classify::engine::{classify, ClassifyInput, FolderClassification};
use crate::classify::metrics::{health_metrics, HealthMetrics, DUPLICATE_CANDIDATE_GROUPS};
use crate::db::activity::{append_activity, json_object, ActivityOutcome};
use crate::error::AppError;

/// Run [`classify`] over `entries`, then append one F-1001 `activity_records`
/// row (action `"classify"`, params naming the entry count).
pub async fn run_classify(
    pool: &SqlitePool,
    entries: &[ClassifyInput],
) -> Result<Vec<FolderClassification>, AppError> {
    let result: Result<Vec<FolderClassification>, AppError> = Ok(classify(entries));

    let params = json_object(&[("entry_count", &entries.len().to_string())]);
    let outcome = ActivityOutcome::from_result(&result);
    append_activity(pool, "classify", &params, &outcome).await;

    result
}

/// The F-202 health metrics for one persisted scan, counting duplicates the way
/// the Duplicates screen counts them (`AC-18`, `AC-29`, `FD-08`).
///
/// # Why this exists rather than a book-aware `health_metrics`
///
/// [`health_metrics`] answers "how many duplicate groups" by bucketing files on
/// `(basename, size)`. `dupes::detect` answers the same question by ALSO
/// grouping duplicated book folders and then marking the per-track groups those
/// folders already cover as subsumed. On jp's library the first says 406 and the
/// second says 300, so the nav badge promised 406 things and the screen listed
/// 300 (the audit at
/// `docs/internal/audits/2026-08-21_nav-badge-count-divergence.md`).
///
/// The audit proposed teaching [`health_metrics`] the book extraction. That
/// would have produced a SECOND book-aware duplicate counter, which is the
/// disease rather than the cure: this repository has now been bitten three times
/// by one rule with two implementations checked against each other instead of
/// against the world (`AC-12`'s gate as a convention, `P2`'s engine reachable
/// from nothing, and this). `detected_duplicates_for_scan`'s own doc comment
/// already says a second copy of the pipeline "is the shape that has already
/// drifted twice in this repository."
///
/// So there is exactly one implementation of "a duplicate group is a book", it
/// lives in `dupes::detect`, and this function makes the health metrics quote it
/// instead of paraphrasing it. Everything user-facing that carries a duplicate
/// count comes through here: the Duplicates nav badge and the Library home (via
/// `classify_overview` and [`crate::classify::build_overview`], which reads the
/// spliced metric), and the after-the-fact check report's before/after delta
/// (via `exec::verify`).
///
/// # Cost
///
/// One extra snapshot read and one detection pass per call, on a surface whose
/// own contract is that it re-derives on every call and never caches. Measured
/// scale is 14,799 entries, and the same detection already runs on demand for
/// the screen. The freshness guarantee (`AC-7`: every count read at render time)
/// is worth more than the read.
pub async fn health_metrics_for_scan(
    pool: &SqlitePool,
    scan_id: i64,
    entries: &[ClassifyInput],
    classifications: &[FolderClassification],
) -> Result<HealthMetrics, AppError> {
    let mut metrics = health_metrics(entries, classifications);

    let (groups, _books) = crate::plan::query::detected_duplicates_for_scan(pool, scan_id).await?;
    let (count, byte_total) = groups
        .iter()
        .filter(|g| g.is_duplicate_candidate())
        .fold((0u64, 0u64), |(n, b), g| (n + 1, b + g.total_bytes));

    // `health_metrics` always emits this metric, and a test pins that, so a
    // miss here means the id was retyped on one side. Fail loudly in debug
    // rather than quietly serving the file-level count the splice was meant to
    // replace: a silent no-op would restore the exact defect being fixed.
    let replaced = metrics
        .problems
        .iter_mut()
        .filter(|p| p.problem == DUPLICATE_CANDIDATE_GROUPS)
        .map(|p| {
            p.count = count;
            p.byte_total = byte_total;
        })
        .count();
    debug_assert_eq!(
        replaced, 1,
        "expected exactly one {DUPLICATE_CANDIDATE_GROUPS} metric to replace, found {replaced}"
    );

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::engine::ClassifyInput;
    use crate::db::open_db;
    use crate::parse::extract::NodeKind;
    use sqlx::Row;
    use tempfile::TempDir;

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");
        (dir, pool)
    }

    /// AC-1001.1: a classify run appends exactly one `activity_records` row,
    /// action `"classify"`, outcome `"succeeded"` (classify never fails).
    #[tokio::test]
    async fn run_classify_appends_one_succeeded_row() {
        let (_db, pool) = fresh_pool().await;

        let entries = vec![ClassifyInput {
            id: 0,
            parent: None,
            name: "Empty Folder".to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }];

        let out = run_classify(&pool, &entries).await.expect("classify runs");
        assert_eq!(out.len(), 1, "one classification per folder entry");

        let rows = sqlx::query("SELECT action, outcome, params_json FROM activity_records")
            .fetch_all(&pool)
            .await
            .expect("fetch activity_records");
        assert_eq!(rows.len(), 1, "exactly one row for the one classify run");
        assert_eq!(rows[0].get::<String, _>("action"), "classify");
        assert_eq!(rows[0].get::<String, _>("outcome"), "succeeded");
        assert_eq!(
            rows[0].get::<String, _>("params_json"),
            r#"{"entry_count":"1"}"#
        );
    }
}
