//! F-803 (app settings) database layer: sqlx read/write for the singleton
//! `settings` row (migrations 0001/0002/0003).
//!
//! v0.4.0 (seeing) Phase 2. Two functions over the one enforced row
//! (`CHECK (id = 1)`): [`get_settings`] reads it into the wire
//! [`AppSettings`](crate::ipc::AppSettings), and [`set_settings`] writes the
//! whole row back and returns the persisted result. The shell's
//! `settings_get` / `settings_set` commands are thin adapters over these.
//!
//! Like the rest of `db`, every query uses the sqlx RUNTIME API
//! (`sqlx::query`, binding at runtime): no compile-time `query!` macros, so the
//! build never needs `DATABASE_URL`.
//!
//! ## Normalization at the boundary (one representation of "unset")
//!
//! The IPC payload and the storage disagree on shape in two documented, tested
//! ways, and this module is the single place they are reconciled:
//!
//! - Roots: a root that is unset is `None` in [`AppSettings`] and SQL `NULL` in
//!   the column. A caller that sends `Some("")` (an empty path) is treated as
//!   unset and stored as `NULL`; a stored empty string reads back as `None`. So
//!   "no library configured" has exactly one representation on each side, which
//!   is what the first-run detection (a `NULL` `library_root`) relies on.
//! - Theme: the column is `NOT NULL DEFAULT 'day'`, but a hand-edited or
//!   future-written value could be anything. [`get_settings`] clamps any value
//!   that is not `"evening"` back to `"day"`, so the frontend never receives a
//!   theme it cannot render (FD-09). Writes trust the caller (the UI only ever
//!   sends `day`/`evening`), but store the clamped value for the same reason.
//!
//! ## Field/column name divergence (`set_aside_root` vs `quarantine_root`)
//!
//! The [`AppSettings::set_aside_root`](crate::ipc::AppSettings::set_aside_root)
//! field maps to the `settings.quarantine_root` column. The column and every
//! internal type keep the engine term "quarantine" (FD-31); the crossing
//! payload uses the family-facing "set aside". This module is that seam.

use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::ipc::AppSettings;
use crate::scan::walk::now_iso8601_utc;

/// The two theme identifiers the UI can render (FD-09). Any other stored value
/// is clamped to [`DEFAULT_THEME`] on read.
const DEFAULT_THEME: &str = "day";
const EVENING_THEME: &str = "evening";

/// Normalize a stored/optional root to the wire `Option<String>`: an absent or
/// empty value becomes `None`, so "unset" has one representation.
fn normalize_root(value: Option<String>) -> Option<String> {
    match value {
        Some(s) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// Clamp any stored theme string to a value the UI can render (`day`/`evening`),
/// defaulting to `day` (FD-09). Applied on both read and write.
fn normalize_theme(value: &str) -> String {
    if value == EVENING_THEME {
        EVENING_THEME.to_string()
    } else {
        DEFAULT_THEME.to_string()
    }
}

/// Read the singleton `settings` row (F-803) into the wire [`AppSettings`].
///
/// The row is guaranteed to exist (seeded by migration 0001, `id = 1`), so a
/// missing row is an internal error mapped to [`AppError::SettingsFailed`], as
/// is any SQLite failure. Roots and theme are normalized (see the module doc).
pub async fn get_settings(pool: &SqlitePool) -> Result<AppSettings, AppError> {
    let row = sqlx::query(
        "SELECT library_root, quarantine_root, reports_root, theme, scan_retention_count \
         FROM settings WHERE id = 1",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::SettingsFailed {
        detail: format!("could not read settings: {e}"),
    })?;

    Ok(AppSettings {
        library_root: normalize_root(row.get::<Option<String>, _>("library_root")),
        set_aside_root: normalize_root(row.get::<Option<String>, _>("quarantine_root")),
        reports_root: normalize_root(row.get::<Option<String>, _>("reports_root")),
        theme: normalize_theme(&row.get::<String, _>("theme")),
        scan_retention_count: row.get::<i64, _>("scan_retention_count"),
    })
}

/// Write the whole singleton `settings` row (F-803) from `next`, then return the
/// persisted, normalized result (so the caller sees exactly what was stored,
/// including the empty-root-to-`None` and theme clamping).
///
/// This is a full-row UPDATE of `id = 1`, not a partial patch: the frontend
/// sends the complete [`AppSettings`] it wants, matching the singleton model.
/// Empty roots are stored as `NULL`; the theme is clamped to `day`/`evening`.
/// Any SQLite failure maps to [`AppError::SettingsFailed`].
pub async fn set_settings(pool: &SqlitePool, next: &AppSettings) -> Result<AppSettings, AppError> {
    let library_root = normalize_root(next.library_root.clone());
    let set_aside_root = normalize_root(next.set_aside_root.clone());
    let reports_root = normalize_root(next.reports_root.clone());
    let theme = normalize_theme(&next.theme);
    let updated_at = now_iso8601_utc();

    sqlx::query(
        "UPDATE settings SET \
           library_root = ?, \
           quarantine_root = ?, \
           reports_root = ?, \
           theme = ?, \
           scan_retention_count = ?, \
           updated_at = ? \
         WHERE id = 1",
    )
    .bind(&library_root)
    .bind(&set_aside_root)
    .bind(&reports_root)
    .bind(&theme)
    .bind(next.scan_retention_count)
    .bind(&updated_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::SettingsFailed {
        detail: format!("could not save settings: {e}"),
    })?;

    Ok(AppSettings {
        library_root,
        set_aside_root,
        reports_root,
        theme,
        scan_retention_count: next.scan_retention_count,
    })
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

    /// A brand-new database returns the F-803 defaults: no roots configured
    /// (library_root `None` is what drives first-run), Day theme (FD-09), and
    /// retention 10 (FD-20, AC-35).
    #[tokio::test]
    async fn fresh_db_returns_f803_defaults() {
        let (_dir, pool) = fresh_pool().await;
        let settings = get_settings(&pool).await.expect("get_settings");
        assert_eq!(settings.library_root, None, "no library configured yet");
        assert_eq!(settings.set_aside_root, None);
        assert_eq!(settings.reports_root, None);
        assert_eq!(
            settings.theme, "day",
            "Day is the first-run default (FD-09)"
        );
        assert_eq!(settings.scan_retention_count, 10, "FD-20 default retention");
    }

    /// set then get round-trips every field (AC-34), including the
    /// set_aside_root -> quarantine_root column mapping.
    #[tokio::test]
    async fn set_then_get_round_trips_all_fields() {
        let (_dir, pool) = fresh_pool().await;

        let desired = AppSettings {
            library_root: Some(r"E:\Books - Audio".to_string()),
            set_aside_root: Some(r"E:\Books - Audio\Set Aside".to_string()),
            reports_root: Some(r"E:\Reports".to_string()),
            theme: "evening".to_string(),
            scan_retention_count: 25,
        };
        let persisted = set_settings(&pool, &desired).await.expect("set_settings");
        assert_eq!(persisted, desired, "set returns exactly what it stored");

        let read_back = get_settings(&pool).await.expect("get_settings");
        assert_eq!(read_back, desired, "get returns what set persisted (AC-34)");
    }

    /// The set_aside_root field lands in the `quarantine_root` column (FD-31),
    /// proven by reading the column directly.
    #[tokio::test]
    async fn set_aside_root_maps_to_quarantine_column() {
        let (_dir, pool) = fresh_pool().await;
        let desired = AppSettings {
            library_root: None,
            set_aside_root: Some(r"E:\Aside".to_string()),
            reports_root: None,
            theme: "day".to_string(),
            scan_retention_count: 10,
        };
        set_settings(&pool, &desired).await.expect("set_settings");

        let column: Option<String> =
            sqlx::query_scalar("SELECT quarantine_root FROM settings WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("read quarantine_root");
        assert_eq!(column.as_deref(), Some(r"E:\Aside"));
    }

    /// An empty-string root is normalized to `None` (SQL NULL), so "unset" has
    /// one representation and first-run detection stays reliable.
    #[tokio::test]
    async fn empty_root_normalizes_to_none() {
        let (_dir, pool) = fresh_pool().await;
        let desired = AppSettings {
            library_root: Some(String::new()),
            set_aside_root: Some(String::new()),
            reports_root: None,
            theme: "day".to_string(),
            scan_retention_count: 10,
        };
        let persisted = set_settings(&pool, &desired).await.expect("set_settings");
        assert_eq!(persisted.library_root, None, "empty root stored as unset");
        assert_eq!(persisted.set_aside_root, None);

        let read_back = get_settings(&pool).await.expect("get_settings");
        assert_eq!(read_back.library_root, None);
    }

    /// An out-of-range/unknown theme string is clamped to Day on read (FD-09),
    /// so the frontend never receives a theme it cannot render.
    #[tokio::test]
    async fn unknown_theme_clamps_to_day() {
        let (_dir, pool) = fresh_pool().await;
        // Write a bad value directly, bypassing set_settings' own clamp, to
        // prove the READ path also defends.
        sqlx::query("UPDATE settings SET theme = 'midnight' WHERE id = 1")
            .execute(&pool)
            .await
            .expect("write bad theme");

        let settings = get_settings(&pool).await.expect("get_settings");
        assert_eq!(settings.theme, "day", "unknown theme clamps to Day");
    }

    /// A second set fully replaces the row (singleton semantics): the previous
    /// values do not linger.
    #[tokio::test]
    async fn second_set_replaces_the_row() {
        let (_dir, pool) = fresh_pool().await;
        set_settings(
            &pool,
            &AppSettings {
                library_root: Some(r"E:\First".to_string()),
                set_aside_root: None,
                reports_root: None,
                theme: "evening".to_string(),
                scan_retention_count: 5,
            },
        )
        .await
        .expect("first set");

        let second = AppSettings {
            library_root: Some(r"E:\Second".to_string()),
            set_aside_root: None,
            reports_root: None,
            theme: "day".to_string(),
            scan_retention_count: 10,
        };
        set_settings(&pool, &second).await.expect("second set");

        let read_back = get_settings(&pool).await.expect("get_settings");
        assert_eq!(read_back, second, "the second set fully replaced the row");
    }
}
