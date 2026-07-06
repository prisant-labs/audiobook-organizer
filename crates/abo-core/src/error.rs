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

    /// The singleton application settings row (F-803) could not be read or
    /// written. A rare SQLite error on the settings CRUD path (`settings_get` /
    /// `settings_set`); `detail` is the developer-facing cause. Distinct from
    /// [`DbMigrationFailed`](AppError::DbMigrationFailed), which is the
    /// startup-time open/migrate failure, and from
    /// [`DbCorruptRecovered`](AppError::DbCorruptRecovered), the recovered
    /// startup case: this is a runtime read/write failure against an
    /// already-open, already-migrated database.
    #[error("settings could not be read or saved: {detail}")]
    SettingsFailed { detail: String },

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

    // ---- Ruleset family (v0.3.0 Phase 3: F-801 ruleset model) ----
    /// A ruleset's JSON body could not be accepted: it is not valid JSON, is
    /// missing a required field, has a field of the wrong type, carries a
    /// `schema_version` this build does not support, or holds an out-of-range
    /// value (for example a series-index width below 1). Returned by
    /// [`crate::ruleset::parse_and_validate`] BEFORE any row is written, so a
    /// bad body is never persisted half-valid (AC-29). `detail` is a
    /// developer-facing explanation; the remediation is the user-facing one.
    #[error("ruleset body is invalid: {detail}")]
    RulesetInvalid { detail: String },

    // ---- Plan family (v0.3.0 Phase 5: F-404 plan validation) ----
    //
    // The machine codes below are the breakdown Section 8 "Plan" family. They
    // are the SAME strings the per-operation validation verdicts carry in
    // `plan_ops.validation_reason` (see [`crate::plan::validate::ValidationReason`],
    // whose `code()` is contract-tested to agree with these variants), so a
    // blocked op's stored reason and the AppError a caller might raise for that
    // same hazard speak one vocabulary. Validation itself does not RETURN an
    // `AppError` (its output is a per-op verdict, not a hard failure); these
    // variants exist so the codes are part of the stable IPC taxonomy and so an
    // apply-time (v0.5.0) refusal can surface the matching code.
    /// A source path recorded at plan time no longer exists at validation time
    /// (the snapshot went stale). The operation cannot run against a vanished
    /// source.
    #[error("source no longer exists (snapshot is stale): {path}")]
    SnapshotStale { path: String },

    /// Two operations in the same plan produce the same target path (compared
    /// case-insensitively for NTFS), so one would clobber the other.
    #[error("two operations target the same path within the plan: {path}")]
    CollisionInPlan { path: String },

    /// An operation's target path already exists on disk (compared
    /// case-insensitively for NTFS) and is not being vacated by the plan.
    #[error("target already exists on disk: {path}")]
    CollisionOnDisk { path: String },

    /// A target path exceeds the maximum length even with the Windows
    /// extended-length (`\\?\`) allowance. `length` is the measured character
    /// count.
    #[error("target path is too long ({length} chars): {path}")]
    PathTooLong { path: String, length: usize },

    /// A path component is not a legal filesystem name (an illegal character, or
    /// a trailing dot/space). This is the backstop to the F-304 name normalizer.
    #[error("path component is not a legal name ({component}): {path}")]
    IllegalComponent { path: String, component: String },

    /// A path component is (or begins with) a reserved Windows device name
    /// (CON, PRN, AUX, NUL, COM1-9, LPT1-9). Backstop to F-304.
    #[error("path component is a reserved device name ({component}): {path}")]
    ReservedName { path: String, component: String },

    /// The cross-volume operations targeting `volume` sum to more bytes
    /// (`needed`) than the volume has free (`available`), so the
    /// copy+verify+delete moves cannot all complete.
    #[error("not enough free space on {volume}: need {needed} bytes, {available} available")]
    CrossVolumeSpaceInsufficient {
        volume: String,
        needed: u64,
        available: u64,
    },

    /// An operation would move a source into its own subtree (target lies inside
    /// source), which is a cycle no filesystem can perform.
    #[error("operation would move a folder into itself: {source_path} -> {target_path}")]
    CycleDetected {
        source_path: String,
        target_path: String,
    },

    /// A proceed/apply was requested but no operation is in the `approved`
    /// state, so there is nothing to do. Defined for the v0.5.0 apply path;
    /// v0.3.0 only plans and validates.
    #[error("no operations are approved")]
    NothingApproved,
}

impl AppError {
    /// Stable machine-readable code. These strings are part of the IPC contract
    /// and must not change; they equal the serde variant tag (contract-tested).
    pub fn code(&self) -> &'static str {
        match self {
            // Storage family
            AppError::DbMigrationFailed { .. } => "db-migration-failed",
            AppError::DbCorruptRecovered { .. } => "db-corrupt-recovered",
            AppError::SettingsFailed { .. } => "settings-failed",
            // Scan family
            AppError::RootNotFound { .. } => "root-not-found",
            AppError::RootNotDirectory { .. } => "root-not-directory",
            AppError::PermissionDenied { .. } => "permission-denied",
            AppError::JunctionSkipped { .. } => "junction-skipped",
            AppError::ScanFailed { .. } => "scan-failed",
            AppError::CsvParse { .. } => "csv-parse",
            // Ruleset family
            AppError::RulesetInvalid { .. } => "ruleset-invalid",
            // Plan family
            AppError::SnapshotStale { .. } => "snapshot-stale",
            AppError::CollisionInPlan { .. } => "collision-in-plan",
            AppError::CollisionOnDisk { .. } => "collision-on-disk",
            AppError::PathTooLong { .. } => "path-too-long",
            AppError::IllegalComponent { .. } => "illegal-component",
            AppError::ReservedName { .. } => "reserved-name",
            AppError::CrossVolumeSpaceInsufficient { .. } => "cross-volume-space-insufficient",
            AppError::CycleDetected { .. } => "cycle-detected",
            AppError::NothingApproved => "nothing-approved",
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
            AppError::SettingsFailed { .. } => {
                "Your settings could not be saved. Restart the app and try again. If this keeps \
                 happening, the disk may be full or the app data folder may be on a synced \
                 location (OneDrive); free space or move the app data out of the synced folder."
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
            // Ruleset family
            AppError::RulesetInvalid { .. } => {
                "This ruleset could not be saved because its settings were not valid. This is \
                 usually an out-of-date or hand-edited ruleset file; reset it to the defaults or \
                 re-create it, then save again."
            }
            // Plan family
            AppError::SnapshotStale { .. } => {
                "A file or folder in the plan has moved or been deleted since the library was \
                 last scanned. Scan again to refresh the plan, then review it."
            }
            AppError::CollisionInPlan { .. } => {
                "Two changes in this plan would end up with the same name and location, so one \
                 would overwrite the other. Adjust the naming rules or exclude one of them, then \
                 review the plan again."
            }
            AppError::CollisionOnDisk { .. } => {
                "A change would land on a file or folder that already exists. Rename or move the \
                 existing item, or exclude this change, then review the plan again."
            }
            AppError::PathTooLong { .. } => {
                "The new location's full path is too long for Windows to store. Choose a shorter \
                 library location or shorter naming rules, then review the plan again."
            }
            AppError::IllegalComponent { .. } => {
                "The new name contains a character Windows does not allow in file names, or ends \
                 in a dot or space. This should have been cleaned up automatically; re-create the \
                 plan, and if it recurs report it."
            }
            AppError::ReservedName { .. } => {
                "The new name matches a name Windows reserves for hardware devices (such as CON \
                 or COM1). This should have been cleaned up automatically; re-create the plan, \
                 and if it recurs report it."
            }
            AppError::CrossVolumeSpaceInsufficient { .. } => {
                "Some changes move files to a different drive, which copies them, and that drive \
                 does not have enough free space for all of them. Free space on the target drive, \
                 or exclude some of those changes, then review the plan again."
            }
            AppError::CycleDetected { .. } => {
                "A change would move a folder inside itself, which is not possible. Adjust the \
                 naming rules or exclude that change, then review the plan again."
            }
            AppError::NothingApproved => {
                "No changes have been approved yet, so there is nothing to do. Approve at least \
                 one group or operation first."
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
            AppError::SettingsFailed {
                detail: "database is locked".into(),
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
            AppError::RulesetInvalid {
                detail: "missing field `naming`".into(),
            },
            AppError::SnapshotStale {
                path: r"E:\Books\Gone.m4b".into(),
            },
            AppError::CollisionInPlan {
                path: r"E:\Books\Author\Title".into(),
            },
            AppError::CollisionOnDisk {
                path: r"E:\Books\Author\Title".into(),
            },
            AppError::PathTooLong {
                path: r"E:\Books\very\long\path".into(),
                length: 33_000,
            },
            AppError::IllegalComponent {
                path: r"E:\Books\bad?name".into(),
                component: "bad?name".into(),
            },
            AppError::ReservedName {
                path: r"E:\Books\CON".into(),
                component: "CON".into(),
            },
            AppError::CrossVolumeSpaceInsufficient {
                volume: "D:".into(),
                needed: 2_000,
                available: 1_000,
            },
            AppError::CycleDetected {
                source_path: r"E:\Books\A".into(),
                target_path: r"E:\Books\A\B".into(),
            },
            AppError::NothingApproved,
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
        // equal the serde variant tag. This locks the two together so a future
        // rename cannot drift the contract silently. Struct variants serialize
        // externally-tagged as a one-key object; a UNIT variant (e.g.
        // `NothingApproved`) serializes as the bare tag STRING - both forms are
        // valid, and in both the tag is exactly the machine code.
        for err in one_of_each() {
            let value = serde_json::to_value(&err).expect("serialize");
            let tag = match &value {
                serde_json::Value::Object(o) => {
                    o.keys().next().cloned().expect("one-key tagged object")
                }
                serde_json::Value::String(s) => s.clone(),
                other => panic!("unexpected AppError serialization: {other:?}"),
            };
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
