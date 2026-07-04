//! `AppError` taxonomy (thiserror; deriving `serde` and `specta::Type`).
//!
//! One enum, grouped into families as doc-sections so later phases extend it in
//! place. v0.1.0 spine, Phase 2 seeds the **Storage** family (breakdown Section
//! 8). Phase 4 adds the **Scan** family (`root-not-found`, `permission-denied`,
//! `junction-skipped`, ...) as another doc-section below, without disturbing the
//! variants defined here.
//!
//! Wire codes are the serde variant tags, kebab-case via `rename_all`, so the
//! machine code a caller keys off is exactly what serde emits (see
//! [`AppError::code`], which is contract-tested to agree with serialization).
//! Every variant carries a stable code plus a family-safe remediation sentence
//! (never a raw OS error on its own); this pairs one-to-one with the FD-04 UI
//! surfaces.

use serde::{Deserialize, Serialize};

/// The Audiobook Organizer error taxonomy.
///
/// Serialized across IPC as an externally-tagged enum whose tag is the stable
/// kebab-case machine code (e.g. `db-migration-failed`). Deriving `serde` both
/// ways and `specta::Type` makes `Result<T, AppError>` a valid tauri-specta
/// command return type once the seam is wired (Phase 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, thiserror::Error)]
#[serde(rename_all = "kebab-case")]
pub enum AppError {
    // ---- Storage family (Phase 2: database open, migration, recovery) ----
    /// The database could not be created, opened, or migrated (and recovery to a
    /// fresh database also failed). This is the hard-failure end of the startup
    /// path; the recoverable case surfaces as [`DbCorruptRecovered`].
    ///
    /// [`DbCorruptRecovered`]: AppError::DbCorruptRecovered
    #[error("database migration failed: {detail}")]
    DbMigrationFailed { detail: String },

    /// The existing database was unreadable and was reset. The corrupt file was
    /// preserved (moved aside, never deleted) at `backup_path`, and a fresh,
    /// migrated database is in use. Non-fatal: surfaced by the shell as a
    /// one-time family-safe notice, not a blocking error.
    #[error("database was corrupt and has been recovered; prior data preserved at {backup_path}")]
    DbCorruptRecovered { backup_path: String },

    // ---- Scan family (Phase 3: scanner, file typing, snapshot persistence) ----
    //
    // Phase 3 seeds the two root-validation codes plus one internal write-path
    // code. Phase 4 formalizes the rest of the Scan taxonomy (`permission-denied`,
    // `junction-skipped`, `csv-parse`; architecture Section 7). Note: per-entry
    // permission-denied and junction events are NOT hard errors here - the scan
    // records the entry, counts it as skipped on the summary, and continues to
    // completion (AC-11); only a bad root or a failed DB write aborts a scan.
    /// The scan root does not exist. Return before any DB row is written.
    #[error("scan root not found: {path}")]
    RootNotFound { path: String },

    /// The scan root exists but is not a directory (e.g. a file was chosen).
    #[error("scan root is not a directory: {path}")]
    RootNotDirectory { path: String },

    /// A single entry could not be read because the OS denied access. Defined
    /// and ready for v0.2.0: the v0.1.0 walk records such entries and counts
    /// them in [`ScanSummary::skipped_count`](crate::ipc::ScanSummary) rather
    /// than surfacing this per-entry error (AC-11), so a locked subtree never
    /// aborts a scan. Wiring the walk to emit this per entry is a v0.2.0
    /// concern; the code, message, and remediation are pinned here so the IPC
    /// contract is stable before then. Not produced by `run_scan` yet.
    #[error("permission denied for entry: {path}")]
    PermissionDenied { path: String },

    /// A junction or reparse point was recorded but deliberately not followed
    /// (D-09), so the walk cannot loop through a link back into the tree.
    /// Defined and ready for v0.2.0 on the same footing as
    /// [`PermissionDenied`](AppError::PermissionDenied): the v0.1.0 walk counts
    /// the skip on the summary rather than emitting this per-entry error, and
    /// wiring it per entry is a v0.2.0 concern. Not produced by `run_scan` yet.
    #[error("junction/reparse point skipped (not followed): {path}")]
    JunctionSkipped { path: String },

    /// An internal failure while writing the snapshot (a SQLite/transaction
    /// error during the scans/entries write path). The walk itself never fails
    /// this way; this is the DB-side hard-failure end of `run_scan`.
    #[error("scan failed: {detail}")]
    ScanFailed { detail: String },

    /// A WizTree CSV import (F-102) could not fully parse. `row` is the 1-based
    /// index of the offending data row (the row after the header, preamble
    /// lines not counted), OR `0` reserved for a file-level/structural failure
    /// (the file could not be opened, or no WizTree header line was found at
    /// all) rather than a single bad data row.
    ///
    /// Isolated bad data rows (`row >= 1`) are recovered from: the row is
    /// skipped and the import continues with the rest of the file, so this
    /// variant surfaces as a per-row record the caller collects rather than as
    /// a hard `Err` in that case. A wholly invalid file (`row == 0`) IS a hard
    /// `Err` returned before any `scans` row is written, mirroring
    /// [`AppError::RootNotFound`]'s before-any-write posture.
    #[error("CSV row {row} could not be parsed")]
    CsvParse { row: usize },
}

impl AppError {
    /// Stable machine-readable code. These strings are part of the IPC contract
    /// and must not change; they equal the serde variant tag (contract-tested).
    pub fn code(&self) -> &'static str {
        match self {
            // Storage family
            AppError::DbMigrationFailed { .. } => "db-migration-failed",
            AppError::DbCorruptRecovered { .. } => "db-corrupt-recovered",
            // Scan family
            AppError::RootNotFound { .. } => "root-not-found",
            AppError::RootNotDirectory { .. } => "root-not-directory",
            AppError::PermissionDenied { .. } => "permission-denied",
            AppError::JunctionSkipped { .. } => "junction-skipped",
            AppError::ScanFailed { .. } => "scan-failed",
            AppError::CsvParse { .. } => "csv-parse",
        }
    }

    /// Family-safe, actionable guidance shown alongside the message. Never empty.
    pub fn remediation(&self) -> &'static str {
        match self {
            // Storage family
            AppError::DbMigrationFailed { .. } => {
                "The app's database could not be prepared. Restart the app. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
            }
            AppError::DbCorruptRecovered { .. } => {
                "The app's database was unreadable and has been reset so the app can run. Your \
                 previous data was preserved as a backup in the corrupt-backups folder and can \
                 be recovered manually if needed."
            }
            // Scan family
            AppError::RootNotFound { .. } => {
                "The folder to scan could not be found. Check that the drive is connected and \
                 the folder still exists, then choose it again."
            }
            AppError::RootNotDirectory { .. } => {
                "The item chosen to scan is a file, not a folder. Choose a folder to scan."
            }
            AppError::PermissionDenied { .. } => {
                "This item could not be read because Windows denied access. It was skipped and \
                 the rest of the scan continued. If you need it included, run the app as a user \
                 who can read it, or adjust the folder's permissions, then scan again."
            }
            AppError::JunctionSkipped { .. } => {
                "This item is a junction or reparse point (a link to another location). It was \
                 recorded but not opened, so the scan cannot loop back on itself. This is \
                 expected behavior and needs no action."
            }
            AppError::ScanFailed { .. } => {
                "The scan could not be saved. Restart the app and try again. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
            }
            AppError::CsvParse { .. } => {
                "One or more rows in the WizTree CSV file could not be read. Rows that could be \
                 read were imported; check that the file is an unmodified WizTree export and try \
                 again if entries are missing."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One instance of every variant. Keep in sync with the enum; the code and
    /// remediation coverage tests iterate this list.
    fn one_of_each() -> Vec<AppError> {
        vec![
            AppError::DbMigrationFailed {
                detail: "boom".into(),
            },
            AppError::DbCorruptRecovered {
                backup_path: "C:/x/corrupt-backups/abo-1.db".into(),
            },
            AppError::RootNotFound {
                path: r"C:\Users\x\missing".into(),
            },
            AppError::RootNotDirectory {
                path: r"C:\Users\x\a-file.txt".into(),
            },
            AppError::PermissionDenied {
                path: r"C:\Users\x\locked\secret.m4b".into(),
            },
            AppError::JunctionSkipped {
                path: r"C:\Users\x\library\link-to-elsewhere".into(),
            },
            AppError::ScanFailed {
                detail: "database is locked".into(),
            },
            AppError::CsvParse { row: 42 },
        ]
    }

    #[test]
    fn every_code_is_stable_kebab_and_unique() {
        let codes: Vec<&str> = one_of_each().iter().map(AppError::code).collect();
        for c in &codes {
            assert!(!c.is_empty(), "code must be non-empty");
            assert!(
                c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "code must be kebab-case: {c}"
            );
        }
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), codes.len(), "all codes must be unique");
    }

    #[test]
    fn every_variant_has_non_empty_remediation() {
        for err in one_of_each() {
            assert!(
                !err.remediation().is_empty(),
                "remediation must be non-empty for {}",
                err.code()
            );
        }
    }

    #[test]
    fn code_matches_serde_tag() {
        // The machine code is defined "via serde rename": the code() string must
        // equal the externally-tagged serde variant tag. This locks the two
        // together so a future rename cannot drift the contract silently.
        for err in one_of_each() {
            let value = serde_json::to_value(&err).expect("serialize");
            let tag = value
                .as_object()
                .and_then(|o| o.keys().next().cloned())
                .expect("externally-tagged object with one key");
            assert_eq!(tag, err.code(), "serde tag must equal code()");
        }
    }

    #[test]
    fn round_trips_through_serde() {
        for err in one_of_each() {
            let json = serde_json::to_string(&err).expect("serialize");
            let back: AppError = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, err, "AppError must round-trip through serde");
        }
    }
}
