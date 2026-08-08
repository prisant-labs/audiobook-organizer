//! F-505 (plan export): CSV, JSON, and Markdown projections of a built and
//! validated plan.
//!
//! # What this module is, and is not
//!
//! Like [`crate::plan::provenance`], this is a pure GENERATOR: no I/O, no
//! database access, path-agnostic. [`build_plan_export`] takes the same
//! shapes the caller already has after building and validating a plan (a
//! [`BuiltPlan`] plus its aligned [`OpVerdict`] slice, mirroring
//! [`crate::plan::validate::persist_validated_plan`]'s own contract) and
//! produces a [`PlanExport`], whose three `to_*` methods render the three
//! artifacts. The reports-folder file-writing that lands these beside the
//! plan is [`crate::reports`] (F-1002, Phase 7).
//!
//! # FD-31: internal vocabulary never reaches an export
//!
//! The builder already scrubs "quarantine" from every rationale sentence and
//! target path (P6 FIX 3), but three STRUCTURED fields still carry the word
//! as internal data: the `kind` value `"quarantine"`, and the rule ids
//! `"parallel-format-quarantine"` / `"clutter-quarantine"` (F-205/F-402).
//! [`scrub_internal_vocab`] rewrites the substring to `"archive"` wherever
//! it appears, and [`ExportOp::group`] is always the FD-26 user-facing label
//! (never the raw `op_group`, which can be the internal pass name
//! `"dedupe-quarantine"`). No exported field can carry "quarantine" as a
//! result; a banned-vocabulary test greps every rendered artifact for it.
//!
//! # Blocked ops stay visible (AC-19)
//!
//! `ExportOp::validation_state` (and `validation_reason`) are always present,
//! never dropped: an empty plan and a fully-blocked plan both produce valid,
//! non-empty artifacts that state their situation, per the F-505 edge cases.

use serde::{Deserialize, Serialize};

use crate::plan::builder::{group_for_op_group, BuiltPlan, CampaignGroup, PlanStats, PlannedOp};
use crate::plan::validate::OpVerdict;

/// Stable base name (no directory) for the CSV plan export.
pub const PLAN_CSV_BASENAME: &str = "plan.csv";
/// Stable base name for the JSON plan export.
pub const PLAN_JSON_BASENAME: &str = "plan.json";
/// Stable base name for the Markdown plan export.
pub const PLAN_MARKDOWN_BASENAME: &str = "plan.md";

/// The FD-10 deletion-guarantee canon copy, verbatim, reused everywhere the
/// guarantee appears (report, review, done, and here). F-506 (Phase 8) reuses
/// this same constant rather than carrying a second copy of the sentence.
pub const FD10_GUARANTEE_LINE: &str =
    "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.";

/// Rewrite every occurrence of the internal-only word "quarantine" to the
/// plain-language "archive" (FD-42, superseding FD-31's "set-aside"). A plain
/// substring replace is complete
/// because the only field values that ever carry the word are the `kind`
/// value `"quarantine"` and the two rule ids named in the module doc; none of
/// them embed it as part of an unrelated word.
fn scrub_internal_vocab(s: &str) -> String {
    s.replace("quarantine", "archive")
}

/// One exported operation row: the FD-31-scrubbed, spreadsheet/JSON-safe
/// projection of one [`PlannedOp`] plus its validation verdict. `group` is
/// always the FD-26 user-facing label (never the internal `op_group` pass
/// name), which is also what [`PlanExport::to_markdown`] groups by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportOp {
    pub group: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind_reason: Option<String>,
    pub source_path: String,
    pub target_path: String,
    pub rationale: String,
    pub rule_id: String,
    pub confidence: String,
    pub byte_size: i64,
    /// `valid` / `warning` / `blocked` (F-404). Always present: AC-19
    /// requires a fully-blocked plan's export to show the blocks, not hide
    /// them.
    pub validation_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_json: Option<String>,
}

impl ExportOp {
    fn from_op(op: &PlannedOp, verdict: &OpVerdict) -> Self {
        ExportOp {
            group: group_for_op_group(&op.op_group)
                .map(|g| g.label().to_string())
                .unwrap_or_else(|| scrub_internal_vocab(&op.op_group)),
            kind: scrub_internal_vocab(&op.kind),
            kind_reason: op.kind_reason.as_ref().map(|r| scrub_internal_vocab(r)),
            source_path: op.source_path.clone(),
            target_path: op.target_path.clone(),
            rationale: op.rationale.clone(),
            rule_id: scrub_internal_vocab(&op.rule_id),
            confidence: op.confidence.clone(),
            byte_size: op.byte_size,
            validation_state: verdict.state.as_str().to_string(),
            validation_reason: verdict.reason_code().map(|c| c.to_string()),
            provenance_json: op.provenance_json.clone(),
        }
    }
}

/// The whole exported plan: header identity, the builder's own per-group
/// stats (already FD-26-labeled and unaffected by validation), and every op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanExport {
    pub plan_id: i64,
    pub created_at: String,
    pub status: String,
    pub stats: PlanStats,
    pub ops: Vec<ExportOp>,
}

/// Build the export projection from a built plan plus its validation
/// verdicts (1:1 aligned with `plan.ops`, exactly [`crate::plan::validate::persist_validated_plan`]'s
/// own contract). `plan_id`/`created_at`/`status` are the identity the caller
/// already has after persisting (or is about to persist) the plan.
///
/// # Panics
///
/// Panics if `verdicts.len() != plan.ops.len()`, the same invariant
/// `persist_validated_plan` asserts.
pub fn build_plan_export(
    plan_id: i64,
    created_at: &str,
    status: &str,
    plan: &BuiltPlan,
    verdicts: &[OpVerdict],
) -> PlanExport {
    assert_eq!(
        plan.ops.len(),
        verdicts.len(),
        "verdicts must align 1:1 with plan ops"
    );
    PlanExport {
        plan_id,
        created_at: created_at.to_string(),
        status: status.to_string(),
        stats: plan.stats.clone(),
        ops: plan
            .ops
            .iter()
            .zip(verdicts.iter())
            .map(|(op, v)| ExportOp::from_op(op, v))
            .collect(),
    }
}

/// Title-case a single FD-26 label (`"loose books"` -> `"Loose books"`) for
/// Markdown headings. The label set is short, all-ASCII, single-space
/// separated; capitalizing only the first character matches the prototype's
/// plain-language register without a hardcoded second copy of the label list.
fn title_case(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

impl PlanExport {
    /// Serialize to pretty JSON (the machine export, AC-18). Round-trips
    /// losslessly via [`PlanExport::from_json`]: every op field this module
    /// carries, including provenance, survives the trip.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a PlanExport always serializes to JSON")
    }

    /// Parse a [`PlanExport`] back from its JSON export (AC-18's "JSON
    /// re-imports to an equal plan structure").
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Render the CSV export: one row per operation, stable column order,
    /// spreadsheet-friendly (AC-18). Uses the `csv` crate's RFC4180 writer, so
    /// a path holding a comma or a quote round-trips correctly through any
    /// spreadsheet application, not merely this crate's own reader.
    pub fn to_csv(&self) -> String {
        let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());
        wtr.write_record([
            "group",
            "kind",
            "kind_reason",
            "source_path",
            "target_path",
            "rationale",
            "rule_id",
            "confidence",
            "byte_size",
            "validation_state",
            "validation_reason",
        ])
        .expect("writing the header record to an in-memory buffer cannot fail");
        for op in &self.ops {
            let byte_size = op.byte_size.to_string();
            wtr.write_record([
                op.group.as_str(),
                op.kind.as_str(),
                op.kind_reason.as_deref().unwrap_or(""),
                op.source_path.as_str(),
                op.target_path.as_str(),
                op.rationale.as_str(),
                op.rule_id.as_str(),
                op.confidence.as_str(),
                byte_size.as_str(),
                op.validation_state.as_str(),
                op.validation_reason.as_deref().unwrap_or(""),
            ])
            .expect("writing a data record to an in-memory buffer cannot fail");
        }
        let bytes = wtr
            .into_inner()
            .expect("into_inner cannot fail for a Vec<u8> sink");
        String::from_utf8(bytes)
            .expect("every field is built from an owned Rust String, so output is valid UTF-8")
    }

    /// Render the human Markdown summary (AC-18): grouped by the seven FD-26
    /// campaign groups with per-group counts, carrying the FD-10 guarantee
    /// line verbatim. An empty plan and a fully-blocked plan both produce a
    /// valid, non-empty document that states the situation (AC-19); a blocked
    /// or warned op is annotated inline, never silently dropped.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Plan summary\n\n");
        out.push_str(FD10_GUARANTEE_LINE);
        out.push_str("\n\n");

        if self.ops.is_empty() {
            out.push_str(
                "This plan makes zero changes: the library already matches the chosen layout.\n\n",
            );
            return out;
        }

        let blocked = self
            .ops
            .iter()
            .filter(|o| o.validation_state == "blocked")
            .count();
        out.push_str(&format!("{} planned operation(s).\n\n", self.ops.len()));
        if blocked > 0 {
            out.push_str(&format!(
                "{blocked} operation(s) are blocked and need attention before this plan can be applied; they are listed below, never hidden.\n\n"
            ));
        }

        out.push_str("| Group | Operations |\n|---|---|\n");
        for g in &self.stats.per_group {
            out.push_str(&format!("| {} | {} |\n", title_case(&g.group), g.ops));
        }
        out.push('\n');

        for group in CampaignGroup::ALL {
            let label = group.label();
            // Resolve the stored token rather than string-matching the CURRENT label:
            // an export written before FD-46 stores "copies", and a raw comparison
            // would file none of its operations under any heading while the summary
            // table above still counted them. See CampaignGroup::from_stored_token.
            let in_group: Vec<&ExportOp> = self
                .ops
                .iter()
                .filter(|o| CampaignGroup::from_stored_token(&o.group) == Some(*group))
                .collect();
            out.push_str(&format!("## {}\n\n", title_case(label)));
            if in_group.is_empty() {
                out.push_str("No changes in this group.\n\n");
                continue;
            }
            for op in &in_group {
                let flag = match op.validation_state.as_str() {
                    "blocked" => format!(
                        " (blocked: {})",
                        op.validation_reason.as_deref().unwrap_or("see validation")
                    ),
                    "warning" => format!(
                        " (warning: {})",
                        op.validation_reason.as_deref().unwrap_or("see validation")
                    ),
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "- {}: {} -> {}{}\n",
                    op.kind, op.source_path, op.target_path, flag
                ));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};
    use crate::plan::builder::{build_plan, classify_inputs_from_plan_nodes, PlanNode};
    use crate::plan::validate::{ValidationReason, ValidationState};
    use crate::ruleset::default_ruleset;
    use crate::scan::typing::FileClass;

    const ROOT: &str = "lib";

    fn leaf(path: &str) -> String {
        path.rsplit('/').next().unwrap().to_string()
    }
    fn dir(id: usize, parent: Option<usize>, path: &str) -> PlanNode {
        PlanNode {
            id,
            parent,
            name: leaf(path),
            path: path.to_string(),
            kind: NodeKind::Folder,
            file_class: None,
            size: 0,
        }
    }
    fn aud(id: usize, parent: Option<usize>, path: &str, size: u64) -> PlanNode {
        PlanNode {
            id,
            parent,
            name: leaf(path),
            path: path.to_string(),
            kind: NodeKind::File,
            file_class: Some(FileClass::Audio),
            size,
        }
    }

    /// A tiny mixed plan: one clean loose-root book plus a parallel-format
    /// pair (mp3 loser beside an m4b), so the built plan carries a real
    /// `kind == "quarantine"` op and a `dedupe-quarantine` op_group, exactly
    /// the internal vocabulary [`scrub_internal_vocab`] exists to catch.
    fn mixed_plan() -> BuiltPlan {
        // `0 M4B` is a folder holding the combined m4b per the F-205
        // convention (Hugo-pack parallel-format shape): a direct mp3 chapter
        // beside it is the non-preferred-format loser the builder quarantines
        // (`kind == "quarantine"`, `op_group == "dedupe-quarantine"`), which
        // is exactly the internal vocabulary this module's tests exist to
        // catch.
        let nodes = vec![
            aud(0, None, "lib/Some Book by Some Author.m4b", 100_000),
            dir(1, None, "lib/Parallel Format Example"),
            aud(2, Some(1), "lib/Parallel Format Example/track01.mp3", 5_000),
            dir(3, Some(1), "lib/Parallel Format Example/0 M4B"),
            aud(
                4,
                Some(3),
                "lib/Parallel Format Example/0 M4B/Parallel Format Example.m4b",
                200_000,
            ),
        ];
        let inputs = classify_inputs_from_plan_nodes(&nodes);
        let cs = classify(&inputs);
        let entry_inputs: Vec<EntryInput> = nodes
            .iter()
            .map(|n| EntryInput {
                id: n.id,
                parent: n.parent,
                name: n.name.clone(),
                kind: n.kind,
            })
            .collect();
        let merged: Vec<MergedEntry> = extract(&entry_inputs);
        build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT)
    }

    /// All-valid verdicts, one per op (the common case for an export test that
    /// does not care about validation).
    fn all_valid(plan: &BuiltPlan) -> Vec<OpVerdict> {
        plan.ops.iter().map(|_| OpVerdict::valid()).collect()
    }

    /// AC-18: CSV has exactly one data row per plan op, over a fixture that
    /// includes a source path with a comma and an embedded quote (the RFC4180
    /// escaping case the controller flagged).
    #[test]
    fn csv_has_one_row_per_op_and_escapes_commas_and_quotes() {
        let plan = BuiltPlan {
            ops: vec![PlannedOp {
                op_group: "loose-root-books".to_string(),
                kind: "move".to_string(),
                kind_reason: None,
                source_path: "E:\\Books\\Weird, \"Quoted\" Name.m4b".to_string(),
                target_path: "E:\\Books\\Author\\Weird, \"Quoted\" Name.m4b".to_string(),
                rationale: "Moved a loose book into its author folder.".to_string(),
                rule_id: "loose-root-books".to_string(),
                confidence: "high".to_string(),
                byte_size: 12_345,
                provenance_json: None,
            }],
            stats: crate::plan::builder::PlanStats {
                total_ops: 1,
                manual_review_ops: 0,
                per_group: Vec::new(),
            },
        };
        let verdicts = all_valid(&plan);
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);

        let csv_text = export.to_csv();
        let mut rdr = csv::ReaderBuilder::new().from_reader(csv_text.as_bytes());
        let rows: Vec<_> = rdr.records().collect::<Result<_, _>>().expect("valid csv");
        assert_eq!(
            rows.len(),
            export.ops.len(),
            "CSV must have exactly one data row per op"
        );
        let source_field = &rows[0][3];
        assert_eq!(
            source_field, "E:\\Books\\Weird, \"Quoted\" Name.m4b",
            "the comma and embedded quote must round-trip through RFC4180 escaping"
        );
    }

    /// AC-18: JSON re-imports to an equal plan structure (field-for-field,
    /// including provenance).
    #[test]
    fn json_round_trips_to_an_equal_export() {
        let plan = mixed_plan();
        let verdicts = all_valid(&plan);
        let export = build_plan_export(7, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);

        let json = export.to_json();
        let parsed = PlanExport::from_json(&json).expect("valid json");
        assert_eq!(export, parsed, "JSON round-trip must be lossless");
    }

    /// AC-18: Markdown groups by the seven FD-26 campaign groups, every
    /// heading present with the count from `stats.per_group`.
    #[test]
    fn markdown_contains_all_seven_group_headings_with_correct_counts() {
        let plan = mixed_plan();
        let verdicts = all_valid(&plan);
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);
        let md = export.to_markdown();

        for group in CampaignGroup::ALL {
            let heading = format!("## {}\n", title_case(group.label()));
            assert!(
                md.contains(&heading),
                "missing heading for group {:?}: {heading}",
                group
            );
        }
        for g in &export.stats.per_group {
            let row = format!("| {} | {} |\n", title_case(&g.group), g.ops);
            assert!(
                md.contains(&row),
                "missing or wrong count row for group {}: {row}\n---\n{md}",
                g.group
            );
        }
    }

    /// AC-19: an empty plan exports valid, non-empty artifacts that state the
    /// situation.
    #[test]
    fn empty_plan_exports_valid_nonempty_artifacts() {
        let plan = BuiltPlan {
            ops: Vec::new(),
            stats: crate::plan::builder::PlanStats {
                total_ops: 0,
                manual_review_ops: 0,
                per_group: CampaignGroup::ALL
                    .iter()
                    .map(|g| crate::plan::builder::GroupCount {
                        group: g.label().to_string(),
                        ops: 0,
                    })
                    .collect(),
            },
        };
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &[]);

        let csv_text = export.to_csv();
        assert!(!csv_text.is_empty(), "CSV must still carry a header row");
        let mut rdr = csv::ReaderBuilder::new().from_reader(csv_text.as_bytes());
        assert_eq!(rdr.records().count(), 0, "zero data rows for zero ops");

        let json = export.to_json();
        assert!(!json.is_empty());
        let parsed = PlanExport::from_json(&json).expect("valid json");
        assert!(parsed.ops.is_empty());

        let md = export.to_markdown();
        assert!(
            md.to_lowercase().contains("zero changes"),
            "empty-plan Markdown must state the situation: {md}"
        );
    }

    /// AC-19: a fully-blocked plan exports with the blocks visible, never
    /// silently dropped.
    #[test]
    fn fully_blocked_plan_shows_every_block() {
        let plan = mixed_plan();
        let verdicts: Vec<OpVerdict> = plan
            .ops
            .iter()
            .map(|_| OpVerdict {
                state: ValidationState::Blocked,
                reason: Some(ValidationReason::CollisionOnDisk),
                how_to: None,
            })
            .collect();
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);

        assert!(
            export.ops.iter().all(|o| o.validation_state == "blocked"),
            "every op must carry the blocked verdict"
        );

        let csv_text = export.to_csv();
        let mut rdr = csv::ReaderBuilder::new().from_reader(csv_text.as_bytes());
        let rows: Vec<_> = rdr.records().collect::<Result<_, _>>().expect("valid csv");
        assert!(
            rows.iter().all(|r| &r[9] == "blocked"),
            "the validation_state column must read blocked for every row"
        );

        let json = export.to_json();
        assert!(json.contains("\"validation_state\": \"blocked\""));

        let md = export.to_markdown();
        assert!(
            md.contains("blocked and need attention"),
            "Markdown must surface the block count, not hide it: {md}"
        );
        assert!(md.contains("(blocked: collision-on-disk)"));
    }

    /// FD-31: no rendered export ever carries the internal-only word
    /// "quarantine", over a fixture that produces a real `kind ==
    /// "quarantine"` op and a `dedupe-quarantine` op_group.
    #[test]
    fn no_export_carries_the_banned_word_quarantine() {
        let plan = mixed_plan();
        assert!(
            plan.ops.iter().any(|o| o.kind == "quarantine"),
            "fixture must exercise a real quarantine-kind op"
        );
        let verdicts = all_valid(&plan);
        let export = build_plan_export(1, "2026-07-04T00:00:00Z", "draft", &plan, &verdicts);

        for (name, text) in [
            ("csv", export.to_csv()),
            ("json", export.to_json()),
            ("markdown", export.to_markdown()),
        ] {
            let lower = text.to_lowercase();
            assert!(
                !lower.contains("quarantine"),
                "{name} export must never contain the internal word \"quarantine\": {text}"
            );
            // Absence alone is a weak assertion: it passes for ANY replacement,
            // including one that silently drops the word or substitutes a term the
            // ledger has retired. Pin the value the scrub is contracted to produce.
            assert!(
                lower.contains("archive"),
                "{name} export must carry the plain-language replacement \"archive\" \
                 (FD-42, superseding FD-31's \"set-aside\"): {text}"
            );
            // The retired term must not come back, in either spelling.
            assert!(
                !lower.contains("set-aside") && !lower.contains("set aside"),
                "{name} export must not carry FD-42's retired \"set aside\": {text}"
            );
        }
    }
}
