//! F-604 (post-apply verification): the after-the-fact check.
//!
//! After any apply the tool proves to itself that the moves happened as planned,
//! re-reads reality, and refuses further FORWARD tidying until a human
//! acknowledges any discrepancy. This module is those three things:
//!
//! 1. **Per-op verification (AC-18)** - [`verify_job`]: for every operation the
//!    executor journaled as DONE, confirm the target exists, the moved item's
//!    size matches the snapshot, and the source path is gone. Discrepancies are
//!    collected PER-OPERATION, never as a blanket pass/fail. Verification runs
//!    through the SAME [`Vfs`] the job ran against (a dry run verifies its
//!    [`MemFs`](super::MemFs); a Real apply verifies [`RealFs`](super::RealFs)),
//!    so the AC-3 dry-run == Real posture extends to the check.
//!
//! 2. **Incremental rescan + delta metrics (AC-19)** - [`delta_health_metrics`]:
//!    re-read the affected library through the EXISTING F-101 scanner (read-only;
//!    no second scanning path) and compute the F-202 health metrics before and
//!    after, so the check shows the library moving toward tidy (for example,
//!    noisy-names count before to after).
//!
//! 3. **Block-further-groups (AC-20)** - the durable block state
//!    ([`record_block`] / [`acknowledge_block`] / [`forward_tidying_blocked`])
//!    plus the forward gate ([`ensure_forward_tidying_allowed`]). A discrepancy
//!    blocks forward tidying until acknowledged; the block SURVIVES a restart
//!    (it lives in the `verification_blocks` table) and is append-only (an
//!    acknowledgement adds a row, never rewrites the raised one).
//!
//! # Two load-bearing structural guards
//!
//! - **The block gates FORWARD tidying only, never undo.** The gate
//!   ([`ensure_forward_tidying_allowed`]) self-exempts an undo (inverse) plan
//!   via [`super::is_undo_plan_ops`], and the undo command path never calls it,
//!   so a blocked-on-discrepancy job stays undoable - undo is the REMEDY for a
//!   discrepancy, and blocking it would trap the user (P5's rollback contract).
//!
//! - **Verification works from the EXECUTED op set, never the forward plan.**
//!   [`verify_job`] takes the executor's OWN (job-id-substituted) ops and the
//!   walk [`ExecOutcome`], so a rollback job verifies its own executed inverse
//!   ops (whose teardown ops are synthesized and exist only in the inverse plan)
//!   exactly like any other job. It never re-derives what to check from a
//!   forward plan.
//!
//! Like the rest of `crate::exec`, this module reaches the filesystem only
//! through the [`Vfs`] seam (verification) or the existing scanner (rescan), and
//! carries no `cfg`-gated platform code of its own (the CFG RULE).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::classify::{classify, health_metrics, inputs_from_snapshot, HealthMetrics, MetricUnit};
use crate::db::plans::PlanOpRow;
use crate::error::AppError;
use crate::scan::{get_scan_entries, run_scan};

use super::{ApplyMode, ExecOutcome, JournalPhase, Vfs, APPROVED};

// ---- Per-op verification (AC-18) -------------------------------------------

/// Which of the three AC-18 facts one [`VerifyCheck`] asserts about an executed
/// operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerifyCheckKind {
    /// The operation's target exists after the walk.
    TargetExists,
    /// The moved item's size at the target matches the plan-time snapshot size.
    SizeMatches,
    /// The operation's source path is gone (the move actually happened).
    SourceGone,
}

impl VerifyCheckKind {
    /// A stable, plain-language-adjacent label for the check.
    pub fn as_str(self) -> &'static str {
        match self {
            VerifyCheckKind::TargetExists => "target-exists",
            VerifyCheckKind::SizeMatches => "size-matches",
            VerifyCheckKind::SourceGone => "source-gone",
        }
    }
}

/// One check's outcome for one operation: which fact it asserted and whether
/// reality agreed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyCheck {
    pub kind: VerifyCheckKind,
    pub passed: bool,
    /// A short developer-facing note (what was expected vs found) when a check
    /// fails; empty when it passed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// The verdict for one executed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpVerifyStatus {
    /// Every applicable check passed - the move happened as planned.
    Verified,
    /// At least one applicable check failed - reality contradicts what the
    /// executor journaled as done. This is what blocks forward tidying (AC-20).
    Discrepancy,
    /// This operation HALTED the walk (it has a `failed` journal row). It is not
    /// a verification discrepancy: the executor already surfaced its own error
    /// code, and this phase reports it as its own truth rather than folding it
    /// into a blanket failure (P5 teardown-halt fairness; P8 frames it).
    Halted,
    /// The operation was never reached (no journal row) - it sits after a halt.
    NotApplied,
}

impl OpVerifyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OpVerifyStatus::Verified => "verified",
            OpVerifyStatus::Discrepancy => "discrepancy",
            OpVerifyStatus::Halted => "halted",
            OpVerifyStatus::NotApplied => "not-applied",
        }
    }
}

/// The per-operation verification record: what the op was, and how reality
/// checked out against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpVerification {
    pub op_id: i64,
    pub seq: i64,
    pub kind: String,
    pub source_path: String,
    pub target_path: String,
    pub status: OpVerifyStatus,
    /// The individual checks run for this op (empty for a `no-op`, a `Halted`, or
    /// a `NotApplied` op, which are not re-checked against reality).
    pub checks: Vec<VerifyCheck>,
}

impl OpVerification {
    fn failed_check_labels(&self) -> Vec<&'static str> {
        self.checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.kind.as_str())
            .collect()
    }
}

/// The whole after-the-fact check's per-op result for one job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    pub job_id: i64,
    pub ops: Vec<OpVerification>,
}

impl VerifyReport {
    /// How many executed operations reality contradicted (AC-20 trigger).
    pub fn discrepancy_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|o| o.status == OpVerifyStatus::Discrepancy)
            .count()
    }

    /// How many executed operations verified cleanly.
    pub fn verified_count(&self) -> usize {
        self.ops
            .iter()
            .filter(|o| o.status == OpVerifyStatus::Verified)
            .count()
    }

    /// Whether any executed operation is a discrepancy (the block trigger).
    pub fn has_discrepancy(&self) -> bool {
        self.discrepancy_count() > 0
    }

    /// A compact JSON summary of the discrepancies for the block row's
    /// `detail_json` and the activity surface: how many, and for each the op id,
    /// seq, target, and which checks failed. Deterministic key order.
    pub fn discrepancy_summary_json(&self) -> String {
        let items: Vec<serde_json::Value> = self
            .ops
            .iter()
            .filter(|o| o.status == OpVerifyStatus::Discrepancy)
            .map(|o| {
                serde_json::json!({
                    "op_id": o.op_id,
                    "seq": o.seq,
                    "target_path": o.target_path,
                    "failed_checks": o.failed_check_labels(),
                })
            })
            .collect();
        serde_json::json!({
            "count": items.len(),
            "discrepancies": items,
        })
        .to_string()
    }
}

/// Verify a completed (or halted) walk against the SAME [`Vfs`] it ran on
/// (AC-18). `ops` are the EXECUTOR'S OWN ops (the job-id-substituted copies from
/// [`Executor::ops`](super::Executor::ops)), so a set-aside op's target is its
/// real `<set-aside>\<job-id>\...` path and a rollback job checks its own
/// executed inverse ops - never a path re-derived from a forward plan. `outcome`
/// classifies each op by its terminal journal phase.
///
/// Per op, by kind:
/// - `move` / `rename` / `quarantine`: target-exists, size-matches (against the
///   plan-time `byte_size` snapshot), and source-gone (the AC-18 triple);
/// - `mkdir`: target-exists (the created directory);
/// - `rmdir-empty`: source-gone (the removed directory);
/// - `no-op`: nothing to re-check.
///
/// An op the executor journaled as `done` whose applicable checks all pass is
/// `Verified`; any failing check makes it a `Discrepancy`. An op with a `failed`
/// row is `Halted` (its own error, not a discrepancy); an op with no row is
/// `NotApplied`. Only APPROVED ops are considered (the ones the walk could have
/// touched).
pub fn verify_job<V: Vfs>(vfs: &V, ops: &[PlanOpRow], outcome: &ExecOutcome) -> VerifyReport {
    let done: std::collections::HashSet<i64> = outcome
        .journal
        .iter()
        .filter(|e| e.phase == JournalPhase::Done)
        .map(|e| e.op_id)
        .collect();
    let failed: std::collections::HashSet<i64> = outcome
        .journal
        .iter()
        .filter(|e| e.phase == JournalPhase::Failed)
        .map(|e| e.op_id)
        .collect();

    let ops_out = ops
        .iter()
        .filter(|o| o.approval == APPROVED)
        .map(|op| {
            let (status, checks) = if failed.contains(&op.id) {
                (OpVerifyStatus::Halted, Vec::new())
            } else if !done.contains(&op.id) {
                (OpVerifyStatus::NotApplied, Vec::new())
            } else {
                let checks = checks_for(vfs, op);
                let status = if checks.iter().all(|c| c.passed) {
                    OpVerifyStatus::Verified
                } else {
                    OpVerifyStatus::Discrepancy
                };
                (status, checks)
            };
            OpVerification {
                op_id: op.id,
                seq: op.seq,
                kind: op.kind.clone(),
                source_path: op.source_path.clone(),
                target_path: op.target_path.clone(),
                status,
                checks,
            }
        })
        .collect();

    VerifyReport {
        job_id: outcome
            .journal
            .first()
            .map(|e| e.job_id)
            .unwrap_or_default(),
        ops: ops_out,
    }
}

/// The AC-18 checks applicable to `op` given its kind, run against `vfs`.
fn checks_for<V: Vfs>(vfs: &V, op: &PlanOpRow) -> Vec<VerifyCheck> {
    match op.kind.as_str() {
        "move" | "rename" | "quarantine" => {
            let target = Path::new(&op.target_path);
            let source = Path::new(&op.source_path);
            let meta = vfs.metadata(target);
            let target_exists = meta.is_ok();
            let (size_ok, size_detail) = match &meta {
                Ok(m) => (
                    m.size == op.byte_size.max(0) as u64,
                    if m.size == op.byte_size.max(0) as u64 {
                        String::new()
                    } else {
                        format!("expected {} bytes, found {}", op.byte_size, m.size)
                    },
                ),
                Err(_) => (
                    false,
                    "the target is missing, so its size cannot match".to_string(),
                ),
            };
            let source_gone = !vfs.exists(source);
            vec![
                VerifyCheck {
                    kind: VerifyCheckKind::TargetExists,
                    passed: target_exists,
                    detail: if target_exists {
                        String::new()
                    } else {
                        format!("nothing found at the target: {}", op.target_path)
                    },
                },
                VerifyCheck {
                    kind: VerifyCheckKind::SizeMatches,
                    passed: size_ok,
                    detail: size_detail,
                },
                VerifyCheck {
                    kind: VerifyCheckKind::SourceGone,
                    passed: source_gone,
                    detail: if source_gone {
                        String::new()
                    } else {
                        format!("the source is still present: {}", op.source_path)
                    },
                },
            ]
        }
        "mkdir" => {
            let target = Path::new(&op.target_path);
            let exists = vfs.exists(target);
            vec![VerifyCheck {
                kind: VerifyCheckKind::TargetExists,
                passed: exists,
                detail: if exists {
                    String::new()
                } else {
                    format!("the folder was not created: {}", op.target_path)
                },
            }]
        }
        "rmdir-empty" => {
            let source = Path::new(&op.source_path);
            let gone = !vfs.exists(source);
            vec![VerifyCheck {
                kind: VerifyCheckKind::SourceGone,
                passed: gone,
                detail: if gone {
                    String::new()
                } else {
                    format!("the folder was not removed: {}", op.source_path)
                },
            }]
        }
        // `no-op` (and any unknown kind) re-checks nothing.
        _ => Vec::new(),
    }
}

// ---- Affected roots + delta health metrics (AC-19) -------------------------

/// The distinct directories a job's approved ops touched (each op's source and
/// target parent), deduplicated and reduced to a minimal set (a directory that
/// sits under another in the set is dropped). Recorded on the check report so
/// the rescan scope is honest and auditable, per the AC-19 "keep the scope
/// honest and recorded" obligation.
pub fn affected_roots(ops: &[PlanOpRow]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for op in ops.iter().filter(|o| o.approval == APPROVED) {
        for p in [op.source_path.as_str(), op.target_path.as_str()] {
            if let Some(parent) = parent_dir(p) {
                if !dirs.iter().any(|d| norm(d) == norm(&parent)) {
                    dirs.push(parent);
                }
            }
        }
    }
    // Reduce to a minimal set: drop any dir that sits at-or-under another.
    let mut minimal: Vec<String> = Vec::new();
    for d in &dirs {
        if dirs
            .iter()
            .any(|other| !std::ptr::eq(other, d) && norm(other) != norm(d) && at_or_under(d, other))
        {
            continue;
        }
        if !minimal.iter().any(|m| norm(m) == norm(d)) {
            minimal.push(d.clone());
        }
    }
    minimal.sort();
    minimal
}

/// The parent directory of `path` (everything before the last separator), or
/// `None` for an empty path or a bare name with no parent.
fn parent_dir(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let trimmed = path.trim_end_matches(['/', '\\']);
    let idx = trimmed.rfind(['/', '\\'])?;
    if idx == 0 {
        return None;
    }
    Some(trimmed[..idx].to_string())
}

/// Normalize a path for set comparison (backslashes to slashes, lowercased, no
/// trailing separator).
fn norm(p: &str) -> String {
    let mut s = p.replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// Whether `child` is at or under `parent` (case- and separator-insensitively).
fn at_or_under(child: &str, parent: &str) -> bool {
    let c = norm(child);
    let p = norm(parent);
    c == p || c.starts_with(&format!("{p}/"))
}

/// One problem metric's movement across the apply (AC-19): its count before and
/// after, so the surface can render "noisy names: 12 to 0".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemDelta {
    /// The stable problem id (for example `"noisy-names"`).
    pub problem: String,
    /// The unit the counts are in (files / folders / groups).
    pub unit: MetricUnit,
    pub before: u64,
    pub after: u64,
}

impl ProblemDelta {
    /// The signed change (after - before); negative means the library got tidier.
    pub fn change(&self) -> i64 {
        self.after as i64 - self.before as i64
    }
}

/// The before/after health metrics for the affected library plus the per-problem
/// deltas (AC-19). A dry run produces none of this (it changed nothing on disk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthDelta {
    /// The `scans.id` of the snapshot the apply was built over (the "before").
    pub before_scan_id: i64,
    /// The `scans.id` of the post-apply rescan (the "after").
    pub after_scan_id: i64,
    pub before: HealthMetrics,
    pub after: HealthMetrics,
    /// Per-problem before-to-after movement, in the fixed problem order.
    pub problems: Vec<ProblemDelta>,
}

/// Compute the AC-19 delta health metrics: the F-202 metrics over the plan's
/// original snapshot (`before_scan_id`) versus a fresh, read-only rescan of
/// `library_root` through the EXISTING F-101 scanner ([`run_scan`]; no second
/// scanning path). The rescan writes its own immutable snapshot (the scanner's
/// normal behavior) and is read-only against the tree it scans.
pub async fn delta_health_metrics(
    pool: &SqlitePool,
    before_scan_id: i64,
    library_root: &Path,
) -> Result<HealthDelta, AppError> {
    let before = metrics_for_scan(pool, before_scan_id).await?;

    // Re-read reality through the existing scanner (read-only against the tree).
    let summary = run_scan(pool, library_root).await?;
    let after_scan_id = summary.scan_id;
    let after = metrics_for_scan(pool, after_scan_id).await?;

    let problems = before
        .problems
        .iter()
        .map(|b| {
            let after_count = after
                .problems
                .iter()
                .find(|a| a.problem == b.problem)
                .map(|a| a.count)
                .unwrap_or(0);
            ProblemDelta {
                problem: b.problem.clone(),
                unit: b.unit,
                before: b.count,
                after: after_count,
            }
        })
        .collect();

    Ok(HealthDelta {
        before_scan_id,
        after_scan_id,
        before,
        after,
        problems,
    })
}

/// The F-202 health metrics over one persisted snapshot (read entries, classify,
/// aggregate). Read-only.
async fn metrics_for_scan(pool: &SqlitePool, scan_id: i64) -> Result<HealthMetrics, AppError> {
    let rows = get_scan_entries(pool, scan_id).await?;
    let inputs = inputs_from_snapshot(&rows);
    let classes = classify(&inputs);
    Ok(health_metrics(&inputs, &classes))
}

// ---- The after-the-fact check report artifact (F-1002) ---------------------

/// The stable base names for the exported after-the-fact check report. Plain
/// language on the user-facing artifact ("the after-the-fact check"), never
/// "verification".
pub const CHECK_REPORT_JSON_BASENAME: &str = "after-the-fact-check.json";
pub const CHECK_REPORT_MARKDOWN_BASENAME: &str = "after-the-fact-check.md";

/// The exported after-the-fact check: the per-op verification, the recorded
/// affected roots, and (for a Real apply) the delta health metrics. Written to
/// the plan's Reports subfolder beside the undo file (F-1002), so the check is
/// auditable without the app database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    pub job_id: i64,
    /// `dry-run` or `real`.
    pub mode: String,
    pub verified_ops: usize,
    pub discrepancy_count: usize,
    /// The per-op verification (AC-18).
    pub ops: Vec<OpVerification>,
    /// The directories the job touched, recorded for scope honesty (AC-19).
    pub affected_roots: Vec<String>,
    /// The delta health metrics (AC-19), present only for a Real apply that
    /// re-read reality; `None` for a dry-run rehearsal (which changed nothing on
    /// disk, so there is no on-disk delta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<HealthDelta>,
}

/// The plain-language phrase for a FAILED check on the user-read report (the
/// after-the-fact check Markdown). The internal token ([`VerifyCheckKind::as_str`])
/// is printed in parentheses beside it for engineering traceability; this is the
/// family-facing half, in the design-system register (books, not "operations").
fn check_phrase(kind: VerifyCheckKind) -> &'static str {
    match kind {
        VerifyCheckKind::TargetExists => "the book is not in its new place",
        VerifyCheckKind::SizeMatches => "the file is not the size it should be",
        VerifyCheckKind::SourceGone => "the original is still in its old place",
    }
}

/// The plain-language label for an F-202 problem id on the user-read report. The
/// raw id is printed in parentheses beside it for traceability.
fn problem_label(id: &str) -> &str {
    match id {
        "loose-root-books" => "loose books at the top level",
        "noisy-names" => "messy folder names",
        "deep-nesting" => "deeply nested folders",
        "duplicate-candidate-groups" => "possible duplicate copies",
        "empty-folders" => "empty folders",
        other => other,
    }
}

impl CheckReport {
    /// Build the report from a verify result, the affected roots, and an optional
    /// delta.
    pub fn build(
        mode: ApplyMode,
        report: &VerifyReport,
        affected_roots: Vec<String>,
        delta: Option<HealthDelta>,
    ) -> Self {
        CheckReport {
            job_id: report.job_id,
            mode: mode.as_str().to_string(),
            verified_ops: report.verified_count(),
            discrepancy_count: report.discrepancy_count(),
            ops: report.ops.clone(),
            affected_roots,
            delta,
        }
    }

    /// Pretty JSON (deterministic field order).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a CheckReport always serializes to JSON")
    }

    /// A plain-language Markdown summary of the after-the-fact check.
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("# After-the-fact check\n\n");
        s.push_str(&format!(
            "Every change was double-checked afterwards. Checked {} change(s); {} matched, {} did not.\n\n",
            self.ops.len(),
            self.verified_ops,
            self.discrepancy_count
        ));
        if self.discrepancy_count > 0 {
            s.push_str(
                "Some changes did not check out. More tidy-ups are paused until this is \
                 acknowledged.\n\n",
            );
            s.push_str("## Changes that did not check out\n\n");
            for op in self
                .ops
                .iter()
                .filter(|o| o.status == OpVerifyStatus::Discrepancy)
            {
                s.push_str(&format!("- {} -> {}\n", op.source_path, op.target_path));
                for c in op.checks.iter().filter(|c| !c.passed) {
                    // Plain-language phrase on the user-read report; the internal
                    // check token stays in parentheses for engineering traceability.
                    s.push_str(&format!(
                        "  - {} ({})\n",
                        check_phrase(c.kind),
                        c.kind.as_str()
                    ));
                }
            }
            s.push('\n');
        }
        if let Some(delta) = &self.delta {
            s.push_str("## Library health, before and after\n\n");
            for p in &delta.problems {
                // Plain-language label on the user-read report; the internal
                // problem id stays in parentheses for engineering traceability.
                s.push_str(&format!(
                    "- {}: {} to {} {} ({})\n",
                    problem_label(&p.problem),
                    p.before,
                    p.after,
                    p.unit.as_str(),
                    p.problem
                ));
            }
            s.push('\n');
        }
        if !self.affected_roots.is_empty() {
            s.push_str("## Folders touched\n\n");
            for r in &self.affected_roots {
                s.push_str(&format!("- {r}\n"));
            }
        }
        s
    }
}

/// Write the after-the-fact check report (JSON + Markdown) into `dir` (creating
/// it if missing), returning the two written paths. Cross-platform `std::fs`
/// like [`crate::reports`]; writes only into the Reports folder, never the
/// library.
pub fn write_check_report(dir: &Path, report: &CheckReport) -> std::io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let json = dir.join(CHECK_REPORT_JSON_BASENAME);
    std::fs::write(&json, report.to_json())?;
    let markdown = dir.join(CHECK_REPORT_MARKDOWN_BASENAME);
    std::fs::write(&markdown, report.to_markdown())?;
    Ok((json, markdown))
}

// ---- Block state (AC-20): durable, append-only -----------------------------

/// The two states a `verification_blocks` row records.
const STATE_RAISED: &str = "raised";
const STATE_ACKNOWLEDGED: &str = "acknowledged";

/// Record a discrepancy block for `job_id` (AC-20): append a `raised` row
/// carrying the discrepancy summary. Append-only - never an UPDATE - so the fact
/// a check failed is permanent even after acknowledgement. Returns the new row
/// id.
pub async fn record_block(
    pool: &SqlitePool,
    job_id: i64,
    at: &str,
    detail_json: &str,
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO verification_blocks (job_id, state, at, detail_json) VALUES (?, ?, ?, ?)",
    )
    .bind(job_id)
    .bind(STATE_RAISED)
    .bind(at)
    .bind(detail_json)
    .execute(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not record the after-the-fact check result: {e}"),
    })?;
    Ok(result.last_insert_rowid())
}

/// Acknowledge a job's discrepancy (AC-20): append an `acknowledged` row for
/// `job_id`. This CLEARS the block (a later `acknowledged` row means the job's
/// `raised` row is no longer outstanding) WITHOUT rewriting the `raised` row -
/// history is added to, never erased. Idempotent at the edges: acknowledging a
/// job with no outstanding block just appends a harmless row.
pub async fn acknowledge_block(pool: &SqlitePool, job_id: i64, at: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO verification_blocks (job_id, state, at, detail_json) VALUES (?, ?, ?, NULL)",
    )
    .bind(job_id)
    .bind(STATE_ACKNOWLEDGED)
    .bind(at)
    .execute(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not record the acknowledgement: {e}"),
    })?;
    Ok(())
}

/// Whether `job_id` has an OUTSTANDING discrepancy block: a `raised` row with no
/// later `acknowledged` row (compared by the append id, which strictly
/// increases). Survives a restart because it reads the durable table.
pub async fn job_is_blocked(pool: &SqlitePool, job_id: i64) -> Result<bool, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_blocks r \
         WHERE r.job_id = ? AND r.state = 'raised' \
         AND NOT EXISTS ( \
            SELECT 1 FROM verification_blocks a \
            WHERE a.job_id = r.job_id AND a.state = 'acknowledged' AND a.id > r.id \
         )",
    )
    .bind(job_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not read the block state: {e}"),
    })?;
    Ok(count > 0)
}

/// Whether FORWARD tidying is blocked anywhere (AC-20): any job with an
/// outstanding `raised` block. This is the gate [`ensure_forward_tidying_allowed`]
/// reads. Durable across restart.
pub async fn forward_tidying_blocked(pool: &SqlitePool) -> Result<bool, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_blocks r \
         WHERE r.state = 'raised' \
         AND NOT EXISTS ( \
            SELECT 1 FROM verification_blocks a \
            WHERE a.job_id = r.job_id AND a.state = 'acknowledged' AND a.id > r.id \
         )",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not read the block state: {e}"),
    })?;
    Ok(count > 0)
}

/// The discrepancy summary (`detail_json`) of a job's outstanding `raised`
/// block, or `None` if the job is not blocked. Used by `job_status` to describe
/// the block on the activity surface without re-reading the check report.
pub async fn outstanding_block_detail(
    pool: &SqlitePool,
    job_id: i64,
) -> Result<Option<String>, AppError> {
    if !job_is_blocked(pool, job_id).await? {
        return Ok(None);
    }
    let detail: Option<String> = sqlx::query_scalar(
        "SELECT detail_json FROM verification_blocks \
         WHERE job_id = ? AND state = 'raised' ORDER BY id DESC LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not read the block detail: {e}"),
    })?
    .flatten();
    Ok(detail)
}

// ---- The forward gate (AC-20 + guard #1) -----------------------------------

/// The forward-tidying gate: refuse to start a FORWARD apply while the library's
/// state is in question. Structurally exempts an UNDO (inverse) plan -
/// [`super::is_undo_plan_ops`] - so undo is never blocked (undo is the REMEDY
/// for both conditions below). The undo command path deliberately does not call
/// this, so `rollback_prepare` is never gated; this is the ONLY forward gate,
/// called from `apply_start` before the walk.
///
/// # Two conditions, deliberately separate
///
/// - **AC-20**: a previous run's after-the-fact check FOUND a difference and
///   nobody has acknowledged it. The tool knows what happened and is waiting for
///   a human to say so.
/// - **Precondition 4** (added 2026-08-04 with the `P1c` interruption surface,
///   wired here): a previous run was cut short and the reconciler could NOT
///   establish what it did. The tool does not know what happened.
///
/// They return different errors because they ask the reader for different
/// things, and merging them would have meant one message that is wrong half the
/// time. The second is checked first: not knowing is the stronger reason to
/// refuse, so it should be the reason reported when both hold.
pub async fn ensure_forward_tidying_allowed(
    pool: &SqlitePool,
    ops: &[PlanOpRow],
) -> Result<(), AppError> {
    // Undo is the remedy for both conditions: an inverse plan is never gated.
    if super::is_undo_plan_ops(ops) {
        return Ok(());
    }
    if crate::exec::reconcile::unreconciled_apply_exists(pool).await? {
        return Err(AppError::InterruptionUnresolved);
    }
    if forward_tidying_blocked(pool).await? {
        return Err(AppError::TidyingBlocked);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use crate::db::plans::{get_plan_ops, insert_plan, NewPlan, NewPlanOp};
    use crate::db::rulesets::{insert_ruleset, NewRuleset};
    use crate::exec::{Executor, MemFs, MemJournal, SeedEntry, Vfs, VfsError, VfsMetadata};
    use tempfile::TempDir;

    const NOW: &str = "2026-07-18T00:00:00Z";

    fn op(id: i64, seq: i64, kind: &str, source: &str, target: &str, byte_size: i64) -> PlanOpRow {
        PlanOpRow {
            id,
            plan_id: 1,
            seq,
            op_group: "loose-root-books".to_string(),
            kind: kind.to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test op.".to_string(),
            rule_id: "test-rule".to_string(),
            confidence: "high".to_string(),
            byte_size,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: None,
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    /// Run the given ops against a MemFs seeded with `seed`, returning the
    /// executor (so its final MemFs can be verified) and the walk outcome.
    async fn run_ops(seed: &[SeedEntry], ops: Vec<PlanOpRow>) -> (Executor<MemFs>, ExecOutcome) {
        let memfs = MemFs::from_seed(seed);
        let executor = Executor::new(memfs, 7, ops);
        let journal = MemJournal::new();
        let outcome = executor.run(&journal, NOW).await.expect("walk");
        (executor, outcome)
    }

    fn seed_file(path: &str, size: u64) -> SeedEntry {
        SeedEntry {
            path: path.to_string(),
            size,
            is_dir: false,
        }
    }

    fn seed_dir(path: &str) -> SeedEntry {
        SeedEntry {
            path: path.to_string(),
            size: 0,
            is_dir: true,
        }
    }

    /// AC-18: a clean apply verifies every op - target exists, size matches the
    /// snapshot, source gone - and flags NO discrepancy, against the same MemFs
    /// the job ran on (the dry-run posture).
    #[tokio::test]
    async fn a_clean_apply_verifies_every_op_with_no_discrepancy() {
        let seed = vec![seed_dir("E:/lib"), seed_file("E:/lib/loose.m4b", 4096)];
        let ops = vec![op(
            1,
            0,
            "move",
            "E:/lib/loose.m4b",
            "E:/lib/Author/loose.m4b",
            4096,
        )];
        let (executor, outcome) = run_ops(&seed, ops).await;
        let report = verify_job(executor.vfs(), executor.ops(), &outcome);

        assert!(
            !report.has_discrepancy(),
            "a clean apply has no discrepancy"
        );
        assert_eq!(report.verified_count(), 1);
        assert_eq!(report.ops.len(), 1);
        let v = &report.ops[0];
        assert_eq!(v.status, OpVerifyStatus::Verified);
        assert_eq!(v.checks.len(), 3, "move gets the AC-18 triple");
        assert!(v.checks.iter().all(|c| c.passed));
    }

    /// AC-18: a target that goes missing AFTER the walk (an injected fault: the
    /// MemFs target removed to model an external deletion) is CAUGHT and reported
    /// PER-OPERATION, and it makes the report a discrepancy (the AC-20 trigger).
    #[tokio::test]
    async fn a_missing_target_after_the_walk_is_caught_per_op() {
        let seed = vec![seed_dir("E:/lib"), seed_file("E:/lib/loose.m4b", 4096)];
        let ops = vec![op(
            1,
            0,
            "move",
            "E:/lib/loose.m4b",
            "E:/lib/Author/loose.m4b",
            4096,
        )];
        let (executor, outcome) = run_ops(&seed, ops).await;

        // Injected fault: the target the walk created is removed from the same
        // MemFs, modelling an external process deleting it between apply and check.
        executor
            .vfs()
            .remove_file(Path::new("E:/lib/Author/loose.m4b"))
            .expect("remove target");

        let report = verify_job(executor.vfs(), executor.ops(), &outcome);
        assert!(report.has_discrepancy());
        assert_eq!(report.discrepancy_count(), 1);
        let v = &report.ops[0];
        assert_eq!(v.status, OpVerifyStatus::Discrepancy);
        let target_check = v
            .checks
            .iter()
            .find(|c| c.kind == VerifyCheckKind::TargetExists)
            .unwrap();
        assert!(
            !target_check.passed,
            "the missing target is the failed check"
        );
        // The summary carries the per-op discrepancy for the block detail.
        let summary = report.discrepancy_summary_json();
        assert!(summary.contains("target-exists"));
        assert!(summary.contains("\"op_id\":1"));
    }

    /// AC-18: a size that does not match the snapshot is caught as a discrepancy
    /// (an injected fault via a read-only Vfs wrapper that lies about one target's
    /// size, borrowing the executor's own MemFs). `verify_job` only reads, so a
    /// borrowing wrapper is enough.
    #[tokio::test]
    async fn a_size_mismatch_is_caught() {
        // A wrapper that reports a size one byte off for one target and otherwise
        // delegates to the borrowed inner MemFs.
        struct WrongSizeFs<'a> {
            inner: &'a MemFs,
            target: PathBuf,
        }
        impl Vfs for WrongSizeFs<'_> {
            fn exists(&self, p: &Path) -> bool {
                self.inner.exists(p)
            }
            fn is_dir(&self, p: &Path) -> bool {
                self.inner.is_dir(p)
            }
            fn metadata(&self, p: &Path) -> Result<VfsMetadata, VfsError> {
                let mut m = self.inner.metadata(p)?;
                if p == self.target {
                    m.size += 1; // a byte off, so size-matches must fail
                }
                Ok(m)
            }
            fn rename(&self, a: &Path, b: &Path) -> Result<(), VfsError> {
                self.inner.rename(a, b)
            }
            fn copy_file(&self, a: &Path, b: &Path) -> Result<u64, VfsError> {
                self.inner.copy_file(a, b)
            }
            fn remove_file(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_file(p)
            }
            fn remove_dir(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.remove_dir(p)
            }
            fn create_dir_all(&self, p: &Path) -> Result<(), VfsError> {
                self.inner.create_dir_all(p)
            }
        }

        let seed = vec![seed_dir("E:/lib"), seed_file("E:/lib/loose.m4b", 4096)];
        let ops = vec![op(
            1,
            0,
            "move",
            "E:/lib/loose.m4b",
            "E:/lib/Author/loose.m4b",
            4096,
        )];
        let (executor, outcome) = run_ops(&seed, ops).await;
        let wrong = WrongSizeFs {
            inner: executor.vfs(),
            target: PathBuf::from("E:/lib/Author/loose.m4b"),
        };
        let report = verify_job(&wrong, executor.ops(), &outcome);
        assert!(report.has_discrepancy());
        let v = &report.ops[0];
        let size_check = v
            .checks
            .iter()
            .find(|c| c.kind == VerifyCheckKind::SizeMatches)
            .unwrap();
        assert!(!size_check.passed);
    }

    /// AC-18: a move whose SOURCE is still present after the walk is a discrepancy
    /// (the source-gone check fails). Injected fault: after the clean move, a copy
    /// is put back at the source location (modelling a move that left the original
    /// behind), so the target and size still check out but the source lingers.
    #[tokio::test]
    async fn a_lingering_source_is_caught() {
        let seed = vec![seed_dir("E:/lib"), seed_file("E:/lib/loose.m4b", 4096)];
        let ops = vec![op(
            1,
            0,
            "move",
            "E:/lib/loose.m4b",
            "E:/lib/Author/loose.m4b",
            4096,
        )];
        let (executor, outcome) = run_ops(&seed, ops).await;

        // Injected fault: put a copy back at the original source location, so the
        // move looks done at the target but the source lingers.
        executor
            .vfs()
            .copy_file(
                Path::new("E:/lib/Author/loose.m4b"),
                Path::new("E:/lib/loose.m4b"),
            )
            .expect("recreate source");

        let report = verify_job(executor.vfs(), executor.ops(), &outcome);
        assert!(report.has_discrepancy());
        let v = &report.ops[0];
        assert_eq!(v.status, OpVerifyStatus::Discrepancy);
        let source_check = v
            .checks
            .iter()
            .find(|c| c.kind == VerifyCheckKind::SourceGone)
            .unwrap();
        assert!(
            !source_check.passed,
            "the lingering source is the failed check"
        );
        // The target and size still check out - only source-gone failed.
        assert!(
            v.checks
                .iter()
                .find(|c| c.kind == VerifyCheckKind::TargetExists)
                .unwrap()
                .passed
        );
        assert!(
            v.checks
                .iter()
                .find(|c| c.kind == VerifyCheckKind::SizeMatches)
                .unwrap()
                .passed
        );
        assert!(report.discrepancy_summary_json().contains("source-gone"));
    }

    /// The teardown-halt fairness guard (P5 foreign-file case, structural): an op
    /// that HALTED the walk is reported `Halted`, NOT a discrepancy, and the
    /// completed ops before it verify cleanly. So a rollback that fully restored
    /// the library but halted on a teardown op reports per-op truth (restorations
    /// good, teardown halted) rather than a blanket failure - and does NOT block
    /// forward tidying via a verification discrepancy.
    #[tokio::test]
    async fn a_halted_op_is_reported_halted_not_a_discrepancy() {
        // op0 moves cleanly; op1's source is absent, so it halts the walk; op2 is
        // never reached.
        let seed = vec![
            seed_dir("E:/lib"),
            seed_file("E:/lib/a.m4b", 10),
            // no b.m4b -> op1 halts source-vanished
            seed_file("E:/lib/c.m4b", 30),
        ];
        let ops = vec![
            op(1, 0, "move", "E:/lib/a.m4b", "E:/lib/A/a.m4b", 10),
            op(2, 1, "move", "E:/lib/b.m4b", "E:/lib/B/b.m4b", 20),
            op(3, 2, "move", "E:/lib/c.m4b", "E:/lib/C/c.m4b", 30),
        ];
        let (executor, outcome) = run_ops(&seed, ops).await;
        assert!(outcome.halt.is_some(), "op1 halts the walk");

        let report = verify_job(executor.vfs(), executor.ops(), &outcome);
        assert!(
            !report.has_discrepancy(),
            "a halt is not a verification discrepancy"
        );
        let by_id = |id: i64| report.ops.iter().find(|o| o.op_id == id).unwrap();
        assert_eq!(
            by_id(1).status,
            OpVerifyStatus::Verified,
            "op0 restored good"
        );
        assert_eq!(by_id(2).status, OpVerifyStatus::Halted, "op1 halted");
        assert_eq!(
            by_id(3).status,
            OpVerifyStatus::NotApplied,
            "op2 never reached"
        );
    }

    /// AC-19: the delta health metrics compute before/after over a real rescan.
    /// A loose book moved into an author folder drops the loose-root-books count.
    #[tokio::test]
    async fn delta_health_metrics_shows_the_library_getting_tidier() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");

        // A library with one loose root book.
        let lib = TempDir::new().expect("lib tempdir");
        let root = lib.path();
        std::fs::write(
            root.join("Sapiens by Yuval Noah Harari.m4b"),
            b"loose bytes",
        )
        .unwrap();

        // The "before" snapshot: the loose library as-is.
        let before = run_scan(&pool, root).await.expect("before scan");

        // Apply the tidy-up on disk: move the loose book into an author folder.
        std::fs::create_dir_all(root.join("Yuval Noah Harari")).unwrap();
        std::fs::rename(
            root.join("Sapiens by Yuval Noah Harari.m4b"),
            root.join("Yuval Noah Harari")
                .join("Sapiens by Yuval Noah Harari.m4b"),
        )
        .unwrap();

        let delta = delta_health_metrics(&pool, before.scan_id, root)
            .await
            .expect("delta");

        let loose = delta
            .problems
            .iter()
            .find(|p| p.problem == "loose-root-books")
            .expect("loose metric");
        assert_eq!(loose.before, 1, "one loose book before");
        assert_eq!(loose.after, 0, "none loose after");
        assert!(loose.change() < 0, "the library got tidier");
        assert_ne!(delta.before_scan_id, delta.after_scan_id);
    }

    /// AC-20: recording a discrepancy blocks the job AND forward tidying;
    /// acknowledging clears both. The acknowledgement is APPEND-ONLY: the raised
    /// row is never rewritten, and a new acknowledged row is appended.
    #[tokio::test]
    async fn record_and_acknowledge_block_is_append_only() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        // A jobs row to satisfy the FK.
        let job_id: i64 = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply','completed',?)",
        )
        .bind(NOW)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        assert!(!forward_tidying_blocked(&pool).await.unwrap());

        record_block(&pool, job_id, NOW, r#"{"count":1}"#)
            .await
            .expect("record block");
        assert!(job_is_blocked(&pool, job_id).await.unwrap());
        assert!(forward_tidying_blocked(&pool).await.unwrap());
        assert_eq!(
            outstanding_block_detail(&pool, job_id).await.unwrap(),
            Some(r#"{"count":1}"#.to_string())
        );

        acknowledge_block(&pool, job_id, NOW).await.expect("ack");
        assert!(!job_is_blocked(&pool, job_id).await.unwrap());
        assert!(!forward_tidying_blocked(&pool).await.unwrap());
        assert_eq!(outstanding_block_detail(&pool, job_id).await.unwrap(), None);

        // Append-only: BOTH the raised and acknowledged rows are present - the
        // raised row was never rewritten (history preserved).
        let states: Vec<String> = sqlx::query_scalar(
            "SELECT state FROM verification_blocks WHERE job_id = ? ORDER BY id",
        )
        .bind(job_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            states,
            vec!["raised".to_string(), "acknowledged".to_string()]
        );
    }

    /// AC-20 + guard #1 (structural): the forward gate refuses a FORWARD plan
    /// while blocked, but ALWAYS allows an UNDO (inverse) plan - undo is the
    /// remedy for a discrepancy and must never be gated. Acknowledging reopens
    /// the forward gate.
    #[tokio::test]
    async fn the_forward_gate_blocks_forward_but_never_undo() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id: i64 = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply','completed',?)",
        )
        .bind(NOW)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        let forward = vec![op(1, 0, "move", "E:/lib/a.m4b", "E:/lib/A/a.m4b", 10)];
        // An undo plan: every op carries the rollback rule id.
        let mut undo = op(1, 0, "move", "E:/lib/A/a.m4b", "E:/lib/a.m4b", 10);
        undo.rule_id = crate::exec::ROLLBACK_RULE_ID.to_string();
        let undo = vec![undo];

        // Not blocked yet: forward is allowed.
        ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect("forward allowed when clear");

        record_block(&pool, job_id, NOW, r#"{"count":1}"#)
            .await
            .unwrap();

        // Blocked: the forward plan is refused, the undo plan is still allowed.
        let err = ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect_err("forward refused when blocked");
        assert_eq!(err.code(), "tidying-blocked");
        ensure_forward_tidying_allowed(&pool, &undo)
            .await
            .expect("undo is never gated");

        // Acknowledge reopens the forward gate.
        acknowledge_block(&pool, job_id, NOW).await.unwrap();
        ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect("forward allowed after acknowledgement");
    }

    /// Precondition 4: a run that was cut short AND could not be reconciled
    /// blocks the forward path, because planning forward from a library nobody
    /// could read is the one move that turns a recoverable interruption into an
    /// unrecoverable one.
    ///
    /// Undo stays allowed throughout, for the same reason it is exempt from the
    /// AC-20 gate: it is the remedy, so gating it would strand the user with a
    /// mess and no way to undo it.
    #[tokio::test]
    async fn an_unreconciled_run_blocks_the_forward_path_but_never_undo() {
        use crate::exec::reconcile::RECONCILE_FAILED_CODE;

        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();

        let job_id: i64 =
            sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply','failed',?)")
                .bind(NOW)
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_rowid();

        let forward = vec![op(1, 0, "move", "E:/lib/a.m4b", "E:/lib/A/a.m4b", 10)];
        let mut undo = op(1, 0, "move", "E:/lib/A/a.m4b", "E:/lib/a.m4b", 10);
        undo.rule_id = crate::exec::ROLLBACK_RULE_ID.to_string();
        let undo = vec![undo];

        // A merely interrupted run does NOT block: the reconciler settled it, so
        // its outcome is known, and FD-39 re-plans from a fresh scan anyway.
        sqlx::query("UPDATE jobs SET error_code = 'interrupted' WHERE id = ?")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect("a settled interruption is not a reason to refuse");

        // An UNRECONCILED one does block, with its own error rather than the
        // after-the-fact-check one.
        sqlx::query("UPDATE jobs SET error_code = ? WHERE id = ?")
            .bind(RECONCILE_FAILED_CODE)
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        let err = ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect_err("forward refused while the last run is unreadable");
        assert_eq!(err.code(), "interruption-unresolved");
        ensure_forward_tidying_allowed(&pool, &undo)
            .await
            .expect("undo is the remedy and is never gated");

        // Settling it reopens the forward gate. Durable state, so this is what a
        // later successful reconcile pass does.
        sqlx::query("UPDATE jobs SET error_code = 'interrupted' WHERE id = ?")
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect("forward allowed once the run is settled");
    }

    /// When BOTH conditions hold, the reported reason is the stronger one. "We
    /// could not read what happened" is a bigger problem than "we read it and
    /// found a difference", and telling the user the smaller one would send them
    /// to acknowledge a check while the real blocker sat unmentioned.
    #[tokio::test]
    async fn not_knowing_outranks_an_unacknowledged_difference() {
        use crate::exec::reconcile::RECONCILE_FAILED_CODE;

        let dir = TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let job_id: i64 =
            sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply','failed',?)")
                .bind(NOW)
                .execute(&pool)
                .await
                .unwrap()
                .last_insert_rowid();

        record_block(&pool, job_id, NOW, r#"{"count":1}"#)
            .await
            .unwrap();
        sqlx::query("UPDATE jobs SET error_code = ? WHERE id = ?")
            .bind(RECONCILE_FAILED_CODE)
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();

        let forward = vec![op(1, 0, "move", "E:/lib/a.m4b", "E:/lib/A/a.m4b", 10)];
        let err = ensure_forward_tidying_allowed(&pool, &forward)
            .await
            .expect_err("still refused");
        assert_eq!(err.code(), "interruption-unresolved");
    }

    /// AC-20 (approval half): the SAME gate `plan_set_group_approval` calls, over
    /// REAL persisted plan ops (loaded via `get_plan_ops`, the command's exact data
    /// path), blocks approving a FORWARD plan's groups while a discrepancy is
    /// unacknowledged, yet always allows approving an UNDO plan's groups (the undo
    /// plan's ops genuinely carry the rollback rule id from `insert_plan`, so the
    /// self-exemption holds on persisted data, not just hand-built rows). Undo's
    /// approval must never be trapped - it is the remedy. Acknowledging reopens
    /// forward approval.
    #[tokio::test]
    async fn approving_further_groups_is_gated_for_forward_but_never_undo() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        let job_id: i64 = sqlx::query(
            "INSERT INTO jobs (kind, state, started_at) VALUES ('apply','completed',?)",
        )
        .bind(NOW)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();

        // Seed a scan + ruleset, then two real persisted plans: a forward plan and
        // an undo plan whose op carries the rollback rule id.
        let scan_id: i64 = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live','E:/lib',?,'completed')",
        )
        .bind(NOW)
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let ruleset_id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            NOW,
        )
        .await
        .unwrap();

        let persist = |rule_id: &'static str, src: &'static str, tgt: &'static str| {
            let pool = pool.clone();
            async move {
                let ops = vec![NewPlanOp {
                    op_group: "loose-root-books",
                    kind: "move",
                    kind_reason: None,
                    source_path: src,
                    target_path: tgt,
                    rationale: "op.",
                    rule_id,
                    confidence: "high",
                    byte_size: 10,
                    validation_state: "valid",
                    validation_reason: None,
                    provenance_json: None,
                }];
                insert_plan(
                    &pool,
                    &NewPlan {
                        scan_id,
                        ruleset_id,
                        status: "draft",
                        stats_json: None,
                    },
                    &ops,
                    NOW,
                )
                .await
                .unwrap()
            }
        };
        let forward_plan = persist("loose-root-books", "E:/lib/a.m4b", "E:/lib/A/a.m4b").await;
        let undo_plan = persist(
            crate::exec::ROLLBACK_RULE_ID,
            "E:/lib/A/a.m4b",
            "E:/lib/a.m4b",
        )
        .await;
        let forward_ops = get_plan_ops(&pool, forward_plan).await.unwrap();
        let undo_ops = get_plan_ops(&pool, undo_plan).await.unwrap();

        // Clear: forward-group approval is allowed.
        ensure_forward_tidying_allowed(&pool, &forward_ops)
            .await
            .expect("forward approval allowed when clear");

        record_block(&pool, job_id, NOW, r#"{"count":1}"#)
            .await
            .unwrap();

        // Blocked: approving a forward group is refused; approving an undo group is
        // still allowed (undo is the remedy - its approval is never trapped).
        let err = ensure_forward_tidying_allowed(&pool, &forward_ops)
            .await
            .expect_err("forward approval refused when blocked");
        assert_eq!(err.code(), "tidying-blocked");
        ensure_forward_tidying_allowed(&pool, &undo_ops)
            .await
            .expect("undo approval is never trapped");

        // Acknowledge reopens forward-group approval.
        acknowledge_block(&pool, job_id, NOW).await.unwrap();
        ensure_forward_tidying_allowed(&pool, &forward_ops)
            .await
            .expect("forward approval allowed after acknowledgement");
    }

    /// The check report artifact serializes and re-reads, carrying the per-op
    /// verification and (when present) the delta - the F-1002 auditable form.
    #[test]
    fn check_report_round_trips_through_json() {
        let report = VerifyReport {
            job_id: 7,
            ops: vec![OpVerification {
                op_id: 1,
                seq: 0,
                kind: "move".to_string(),
                source_path: "E:/lib/a.m4b".to_string(),
                target_path: "E:/lib/A/a.m4b".to_string(),
                status: OpVerifyStatus::Verified,
                checks: vec![VerifyCheck {
                    kind: VerifyCheckKind::TargetExists,
                    passed: true,
                    detail: String::new(),
                }],
            }],
        };
        let check = CheckReport::build(ApplyMode::Real, &report, vec!["E:/lib".to_string()], None);
        let json = check.to_json();
        let back: CheckReport = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(back, check);
        assert_eq!(back.verified_ops, 1);
        assert_eq!(back.discrepancy_count, 0);
        assert!(check.to_markdown().contains("After-the-fact check"));
    }

    /// The user-read Markdown report speaks the plain-language register (MINOR 1):
    /// a failed check and a health problem render as plain phrases, with the
    /// internal token only in parentheses for engineering traceability.
    #[test]
    fn check_report_markdown_uses_plain_language() {
        let empty = crate::classify::HealthMetrics {
            per_class: vec![],
            problems: vec![],
            total_bytes: 0,
        };
        let report = VerifyReport {
            job_id: 1,
            ops: vec![OpVerification {
                op_id: 1,
                seq: 0,
                kind: "move".to_string(),
                source_path: "E:/lib/a.m4b".to_string(),
                target_path: "E:/lib/A/a.m4b".to_string(),
                status: OpVerifyStatus::Discrepancy,
                checks: vec![VerifyCheck {
                    kind: VerifyCheckKind::SourceGone,
                    passed: false,
                    detail: "the source is still present".to_string(),
                }],
            }],
        };
        let delta = HealthDelta {
            before_scan_id: 1,
            after_scan_id: 2,
            before: empty.clone(),
            after: empty,
            problems: vec![ProblemDelta {
                problem: "loose-root-books".to_string(),
                unit: crate::classify::MetricUnit::Files,
                before: 3,
                after: 0,
            }],
        };
        let md = CheckReport::build(ApplyMode::Real, &report, vec![], Some(delta)).to_markdown();

        // Plain-language phrases, not the raw internal tokens, as the primary text.
        assert!(
            md.contains("the original is still in its old place"),
            "plain check phrase present"
        );
        assert!(
            md.contains("loose books at the top level"),
            "plain problem label present"
        );
        // The internal tokens survive only as parenthetical traceability.
        assert!(md.contains("(source-gone)"));
        assert!(md.contains("(loose-root-books)"));
    }

    /// `affected_roots` records the distinct touched directories, reduced to a
    /// minimal set (a child directory under another in the set is dropped).
    #[test]
    fn affected_roots_records_a_minimal_deduped_set() {
        let ops = vec![
            op(1, 0, "move", "E:/lib/a.m4b", "E:/lib/Author/a.m4b", 10),
            op(2, 1, "move", "E:/lib/b.m4b", "E:/lib/Author/b.m4b", 20),
            op(
                3,
                2,
                "quarantine",
                "E:/lib/dup.m4b",
                "E:/Set Aside/7/dup.m4b",
                30,
            ),
        ];
        let roots = affected_roots(&ops);
        // E:/lib and E:/lib/Author collapse to E:/lib; the set-aside job folder
        // is its own root.
        assert!(roots.iter().any(|r| norm(r) == "e:/lib"));
        assert!(roots.iter().any(|r| norm(r) == "e:/set aside/7"));
        assert!(
            !roots.iter().any(|r| norm(r) == "e:/lib/author"),
            "a child under E:/lib is dropped from the minimal set"
        );
    }
}
