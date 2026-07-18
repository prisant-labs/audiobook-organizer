//! F-1002 (reports folder): the stable file-writing location every export
//! (F-505 plan exports, the F-507 provenance report, and later the F-506 HTML
//! report) lands in, beside the app data directory.
//!
//! # The one platform seam this module needs
//!
//! [`reports_root`] and [`plan_export_dir`] take `app_data_dir` as a
//! PARAMETER rather than calling [`crate::paths::app_data_dir`] themselves,
//! exactly like [`crate::db::open_db`] takes its directory as a parameter
//! (see that module's doc comment). Tests pass a temp dir; the one production
//! caller (the shell) passes `crate::paths::app_data_dir()`. This module has
//! no `cfg`-gating of its own: `std::fs` is already cross-platform, so the
//! CFG RULE's "platform reality is confined to `paths.rs`" holds here too.
//!
//! # Deterministic naming (no wall-clock in file CONTENT)
//!
//! Every plan's export subfolder name is a pure function of the plan id and
//! its DB `created_at` timestamp, never the wall clock at export time: running
//! the same export for the same plan twice always lands in the same folder
//! and overwrites the same five files rather than accumulating duplicates.
//! `created_at` is the ISO-8601 UTC stamp already stored on the `plans` row
//! (`crate::db::plans::PlanRow::created_at`); its `:` characters are replaced
//! with `-` because a colon is illegal in an NTFS path component. The
//! resulting name (`2026-07-04T00-00-00Z_plan-1`) sorts chronologically first,
//! so a family browsing `Reports\` sees exports in the order they were made.
//!
//! # Two destinations, one write path (AC-31)
//!
//! F-505/F-1002 requires every export to land in the reports folder PLUS
//! anywhere the user picks (e.g. an export-to-folder dialog). Both are the
//! same five files; only the destination directory differs. [`write_plan_reports`]
//! resolves the deterministic Reports-folder subfolder; [`write_plan_reports_to`]
//! writes the identical five files into a caller-supplied directory verbatim.
//! Both funnel through one private `write_all`, so directory creation and
//! write-failure handling live in exactly one place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::plan::export::{self, PlanExport};
use crate::plan::provenance::{self, ProvenanceReport};

/// The fixed folder name under the app data directory (architecture.md
/// Section 10: "a `Reports/` folder beside the app data").
const REPORTS_DIRNAME: &str = "Reports";

/// The reports root: `<app_data_dir>\Reports`. A pure path join; does not
/// touch the filesystem (mirrors `crate::paths::app_data_dir`'s "does not
/// create the directory" contract - [`write_plan_reports`] creates it at
/// write time via `fs::create_dir_all`).
pub fn reports_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(REPORTS_DIRNAME)
}

/// Sanitize an ISO-8601 `created_at` stamp into an NTFS-legal path component:
/// `:` becomes `-`. `created_at` never carries any other NTFS-reserved
/// character (it is always the `crate::scan` hand-formatted
/// `YYYY-MM-DDTHH-MM-SSZ`-shape stamp), so this single substitution is
/// complete, not a general-purpose sanitizer.
fn sanitize_created_at(created_at: &str) -> String {
    created_at.replace(':', "-")
}

/// The deterministic per-plan export subfolder:
/// `<reports_root>\<sanitized created_at>_plan-<plan_id>`. A pure function of
/// `plan_id` and `created_at` (both read from the plan's own `plans` row),
/// never the wall clock at export time, so exporting the same plan twice
/// always lands in the same folder.
pub fn plan_export_dir(app_data_dir: &Path, plan_id: i64, created_at: &str) -> PathBuf {
    reports_root(app_data_dir).join(format!(
        "{}_plan-{plan_id}",
        sanitize_created_at(created_at)
    ))
}

/// Every file [`write_plan_reports`]/[`write_plan_reports_to`] writes for one
/// plan: the three F-505 exports plus the two F-507 provenance exports.
/// Returned so a caller (a future IPC command, or a test) can find every
/// artifact without re-deriving names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenPlanReports {
    pub dir: PathBuf,
    pub csv: PathBuf,
    pub json: PathBuf,
    pub markdown: PathBuf,
    pub provenance_json: PathBuf,
    pub provenance_markdown: PathBuf,
}

/// Write `contents` to `dir/basename`, creating `dir` (and its ancestors) if
/// missing.
fn write_file(dir: &Path, basename: &str, contents: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(basename);
    fs::write(&path, contents)?;
    Ok(path)
}

/// The one write path both public entry points funnel through: five files
/// into `dir`, from an already-built [`PlanExport`] and [`ProvenanceReport`].
fn write_all(
    dir: &Path,
    plan: &PlanExport,
    provenance: &ProvenanceReport,
) -> io::Result<WrittenPlanReports> {
    Ok(WrittenPlanReports {
        csv: write_file(dir, export::PLAN_CSV_BASENAME, &plan.to_csv())?,
        json: write_file(dir, export::PLAN_JSON_BASENAME, &plan.to_json())?,
        markdown: write_file(dir, export::PLAN_MARKDOWN_BASENAME, &plan.to_markdown())?,
        provenance_json: write_file(
            dir,
            provenance::PROVENANCE_JSON_BASENAME,
            &provenance.to_json(),
        )?,
        provenance_markdown: write_file(
            dir,
            provenance::PROVENANCE_MARKDOWN_BASENAME,
            &provenance.to_markdown(),
        )?,
        dir: dir.to_path_buf(),
    })
}

/// Write the full F-505 export set plus the F-507 provenance report for one
/// plan into its deterministic Reports-folder subfolder (F-1002, AC-31).
/// `app_data_dir` is the platform-seam parameter (see the module doc);
/// production calls this with `crate::paths::app_data_dir()`, tests with a
/// temp dir.
pub fn write_plan_reports(
    app_data_dir: &Path,
    plan_id: i64,
    created_at: &str,
    plan: &PlanExport,
    provenance: &ProvenanceReport,
) -> io::Result<WrittenPlanReports> {
    let dir = plan_export_dir(app_data_dir, plan_id, created_at);
    write_all(&dir, plan, provenance)
}

/// Write the identical five artifacts into a caller-supplied `dir` verbatim,
/// no Reports-folder subfolder naming applied. The "plus anywhere the user
/// picks" half of F-1002/AC-31 (e.g. an export-to-folder dialog result).
pub fn write_plan_reports_to(
    dir: &Path,
    plan: &PlanExport,
    provenance: &ProvenanceReport,
) -> io::Result<WrittenPlanReports> {
    write_all(dir, plan, provenance)
}

/// Write the F-506 self-contained HTML dry-run report into `dir` (creating it if
/// missing) under the stable [`crate::plan::report::PLAN_HTML_BASENAME`], and
/// return its path. Kept separate from [`write_plan_reports`]'s five-file set
/// because the HTML is rendered from a wider input (duplicate candidates, the
/// dateline) than the plan/provenance exports; the orchestrator
/// ([`crate::plan::report::generate_and_report`]) calls both against the same
/// deterministic subfolder.
pub fn write_html_report(dir: &Path, html: &str) -> io::Result<PathBuf> {
    write_file(dir, crate::plan::report::PLAN_HTML_BASENAME, html)
}

/// Re-emit ONLY the F-507 provenance report (JSON + Markdown) into `dir`, creating
/// it if missing, and return the two written paths. Used by the v0.5.0 apply path
/// to re-emit provenance reflecting the applied (final) locations (AC-12), reusing
/// the same basenames and one write path as the full plan-report set.
pub fn write_provenance_report(
    dir: &Path,
    provenance: &ProvenanceReport,
) -> io::Result<(PathBuf, PathBuf)> {
    let json = write_file(
        dir,
        provenance::PROVENANCE_JSON_BASENAME,
        &provenance.to_json(),
    )?;
    let markdown = write_file(
        dir,
        provenance::PROVENANCE_MARKDOWN_BASENAME,
        &provenance.to_markdown(),
    )?;
    Ok((json, markdown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::builder::{BuiltPlan, CampaignGroup, GroupCount, PlanStats, PlannedOp};
    use crate::plan::export::build_plan_export;
    use crate::plan::provenance::build_provenance_report;
    use crate::plan::validate::OpVerdict;
    use tempfile::TempDir;

    fn sample_plan() -> BuiltPlan {
        BuiltPlan {
            ops: vec![PlannedOp {
                op_group: "loose-root-books".to_string(),
                kind: "move".to_string(),
                kind_reason: None,
                source_path: "E:\\Books\\Some Book.m4b".to_string(),
                target_path: "E:\\Books\\Author\\Some Book.m4b".to_string(),
                rationale: "Moved a loose book into its author folder.".to_string(),
                rule_id: "loose-root-books".to_string(),
                confidence: "high".to_string(),
                byte_size: 1_000,
                provenance_json: None,
            }],
            stats: PlanStats {
                total_ops: 1,
                manual_review_ops: 0,
                per_group: CampaignGroup::ALL
                    .iter()
                    .map(|g| GroupCount {
                        group: g.label().to_string(),
                        ops: if *g == CampaignGroup::LooseBooks {
                            1
                        } else {
                            0
                        },
                    })
                    .collect(),
            },
        }
    }

    /// AC-31: every artifact lands in the reports folder, at the deterministic
    /// path derived from plan id + created_at.
    #[test]
    fn write_plan_reports_lands_every_artifact_under_the_reports_root() {
        let tmp = TempDir::new().expect("tempdir");
        let app_data_dir = tmp.path();
        let plan = sample_plan();
        let verdicts = vec![OpVerdict::valid()];
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);
        let provenance = build_provenance_report(&plan);

        let written = write_plan_reports(
            app_data_dir,
            1,
            "2026-07-04T00:00:00Z",
            &export,
            &provenance,
        )
        .expect("write_plan_reports");

        let expected_dir = reports_root(app_data_dir).join("2026-07-04T00-00-00Z_plan-1");
        assert_eq!(written.dir, expected_dir);
        for path in [
            &written.csv,
            &written.json,
            &written.markdown,
            &written.provenance_json,
            &written.provenance_markdown,
        ] {
            assert!(
                path.starts_with(&expected_dir),
                "{path:?} must live under the deterministic plan export dir"
            );
            assert!(path.exists(), "{path:?} must actually be written");
            assert!(
                path.starts_with(app_data_dir.join("Reports")),
                "{path:?} must live under the Reports root"
            );
        }
    }

    /// Deterministic naming: the same plan id + created_at always resolves to
    /// the same directory, and re-writing overwrites rather than
    /// accumulating.
    #[test]
    fn same_plan_id_and_created_at_always_lands_in_the_same_dir() {
        let tmp = TempDir::new().expect("tempdir");
        let app_data_dir = tmp.path();
        let plan = sample_plan();
        let verdicts = vec![OpVerdict::valid()];
        let export = build_plan_export(9, "2026-01-02T03:04:05Z", "draft", &plan, &verdicts);
        let provenance = build_provenance_report(&plan);

        let first = write_plan_reports(
            app_data_dir,
            9,
            "2026-01-02T03:04:05Z",
            &export,
            &provenance,
        )
        .expect("first write");
        let second = write_plan_reports(
            app_data_dir,
            9,
            "2026-01-02T03:04:05Z",
            &export,
            &provenance,
        )
        .expect("second write");
        assert_eq!(first.dir, second.dir);

        let entries: Vec<_> = fs::read_dir(reports_root(app_data_dir))
            .expect("read reports root")
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "re-exporting must not create a second folder"
        );
    }

    /// The colon in an ISO-8601 `created_at` is replaced (NTFS-illegal in a
    /// path component).
    #[test]
    fn created_at_colons_are_sanitized_in_the_folder_name() {
        let tmp = TempDir::new().expect("tempdir");
        let dir = plan_export_dir(tmp.path(), 3, "2026-07-04T12:30:00Z");
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(
            !name.contains(':'),
            "folder name must not contain a colon: {name}"
        );
        assert_eq!(name, "2026-07-04T12-30-00Z_plan-3");
    }

    /// AC-31 sibling: the "anywhere the user picks" destination writes the
    /// same five files with no Reports-folder subfolder applied.
    #[test]
    fn write_plan_reports_to_writes_the_same_five_files_verbatim() {
        let tmp = TempDir::new().expect("tempdir");
        let user_dir = tmp.path().join("wherever the user picked");
        let plan = sample_plan();
        let verdicts = vec![OpVerdict::valid()];
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);
        let provenance = build_provenance_report(&plan);

        let written =
            write_plan_reports_to(&user_dir, &export, &provenance).expect("write_plan_reports_to");
        assert_eq!(written.dir, user_dir);
        for path in [
            &written.csv,
            &written.json,
            &written.markdown,
            &written.provenance_json,
            &written.provenance_markdown,
        ] {
            assert_eq!(path.parent(), Some(user_dir.as_path()));
            assert!(path.exists());
        }
    }
}
