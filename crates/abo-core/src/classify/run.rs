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
