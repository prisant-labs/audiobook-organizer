//! F-507 (pack provenance report): turn a built plan's captured pack membership
//! into an exportable provenance report that lists every flattened pack and its
//! members.
//!
//! Provenance is captured at build time on every `flatten-packs` member move
//! (see [`crate::plan::builder`], AC-24): the `plan_ops.provenance_json` column
//! records the source pack identity and any award/rank marker (the `^`
//! Hugo-winner caret). This module reads those captured ops back into a
//! structured report and serializes it (JSON for machines, Markdown for people).
//!
//! # What this module is, and is not
//!
//! It is the pure GENERATOR: [`build_provenance_report`] over a
//! [`BuiltPlan`], plus serialization. It is path-agnostic and does no I/O; the
//! reports-folder plumbing that writes it beside the plan lands in Phase 7
//! (F-1002). Stable file base names are offered as constants for that phase.
//!
//! # Completeness over outcome (AC-25)
//!
//! The report is built from the plan's ops, which include EVERY flattened pack
//! member's move, regardless of any later validation verdict. Provenance is
//! about origin, not outcome: a member that validation blocks is still recorded
//! (its move op and its `provenance_json` remain in the plan), so the report
//! omits no member. A count assertion in the tests covers this.
//!
//! # FD-12
//!
//! The report states origin only. It never promises an ABS-side collection or
//! tag write (that is deferred to F-1102, v1.1+); the copy here is factual
//! ("extracted from pack X"), never a promise about the target app.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::plan::builder::BuiltPlan;

/// Stable base name (no directory) for the JSON provenance export the reports
/// folder writes in Phase 7.
pub const PROVENANCE_JSON_BASENAME: &str = "provenance-report.json";

/// Stable base name for the Markdown provenance export.
pub const PROVENANCE_MARKDOWN_BASENAME: &str = "provenance-report.md";

/// One flattened pack member in the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceMember {
    /// Where the book was extracted FROM (its path inside the pack).
    pub source_path: String,
    /// Where the plan moves it TO (its canonical library location).
    pub target_path: String,
    /// The award/rank marker recorded for this member (the `^` caret), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub award_marker: Option<String>,
}

/// One source pack and every member extracted from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenancePack {
    pub pack_name: String,
    pub pack_path: String,
    /// Members, sorted by source path (deterministic).
    pub members: Vec<ProvenanceMember>,
}

/// The whole provenance report: every flattened pack, plus roll-up counts. The
/// counts state their quantity explicitly (packs vs members) so no reader
/// confuses one for the other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvenanceReport {
    pub packs: Vec<ProvenancePack>,
    pub total_packs: usize,
    pub total_members: usize,
}

/// The minimal per-operation shape [`build_provenance_report`] needs: an op's
/// source, target, and captured provenance JSON. Both the build-time path (over
/// [`BuiltPlan`]'s [`crate::plan::builder::PlannedOp`]s) and the apply-time
/// re-emit (over persisted [`crate::db::plans::PlanOpRow`]s, AC-12) fold into this
/// one shape so the grouping logic lives in exactly one place.
struct ProvOp<'a> {
    source_path: &'a str,
    target_path: &'a str,
    provenance_json: Option<&'a str>,
}

/// Build the provenance report from a built plan. Reads every op that carries
/// captured provenance (the `flatten-packs` member moves), groups them by source
/// pack, and rolls up counts. Deterministic: packs sorted by pack path, members
/// by source path. An op whose `provenance_json` is absent or unparseable
/// contributes nothing (only the builder writes this field, and it always writes
/// valid JSON, so the unparseable case is defensive).
pub fn build_provenance_report(plan: &BuiltPlan) -> ProvenanceReport {
    build_from(plan.ops.iter().map(|op| ProvOp {
        source_path: &op.source_path,
        target_path: &op.target_path,
        provenance_json: op.provenance_json.as_deref(),
    }))
}

/// Build the provenance report from persisted plan-operation rows (the apply-time
/// re-emit reflecting final locations, AC-12). Identical grouping to
/// [`build_provenance_report`]; only the input type differs.
pub fn build_provenance_report_from_rows(ops: &[crate::db::plans::PlanOpRow]) -> ProvenanceReport {
    build_from(ops.iter().map(|op| ProvOp {
        source_path: &op.source_path,
        target_path: &op.target_path,
        provenance_json: op.provenance_json.as_deref(),
    }))
}

/// The shared grouping core (see [`build_provenance_report`]).
fn build_from<'a>(ops: impl Iterator<Item = ProvOp<'a>>) -> ProvenanceReport {
    // pack_path -> (pack_name, members)
    let mut packs: BTreeMap<String, (String, Vec<ProvenanceMember>)> = BTreeMap::new();

    for op in ops {
        let Some(raw) = op.provenance_json else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            continue;
        };
        let pack_path = value
            .get("pack_path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let pack_name = value
            .get("pack_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let award_marker = value
            .get("award_marker")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let entry = packs
            .entry(pack_path)
            .or_insert_with(|| (pack_name.clone(), Vec::new()));
        entry.1.push(ProvenanceMember {
            source_path: op.source_path.to_string(),
            target_path: op.target_path.to_string(),
            award_marker,
        });
    }

    let mut total_members = 0usize;
    let packs: Vec<ProvenancePack> = packs
        .into_iter()
        .map(|(pack_path, (pack_name, mut members))| {
            members.sort_by(|a, b| a.source_path.cmp(&b.source_path));
            total_members += members.len();
            ProvenancePack {
                pack_name,
                pack_path,
                members,
            }
        })
        .collect();

    ProvenanceReport {
        total_packs: packs.len(),
        total_members,
        packs,
    }
}

impl ProvenanceReport {
    /// Serialize to pretty JSON (the machine export). Deterministic: field order
    /// is fixed and the collections are already sorted.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a ProvenanceReport always serializes to JSON")
    }

    /// Render a plain-language Markdown report (the human export). States origin
    /// only; never promises an ABS-side change (FD-12). An award marker is noted
    /// as kept in the report rather than in a folder name (ties to FD-12).
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Where these books came from\n\n");
        if self.packs.is_empty() {
            out.push_str("No books were extracted from a box set or collection in this plan.\n");
            return out;
        }
        out.push_str(&format!(
            "This preview extracted {} book(s) from {} box set(s) or collection(s). \
             Each book's origin is recorded here so nothing about where it came from is lost. \
             Nothing is written back to your audiobook app.\n\n",
            self.total_members, self.total_packs
        ));
        for pack in &self.packs {
            out.push_str(&format!("## {}\n\n", pack.pack_name));
            for m in &pack.members {
                let award = match &m.award_marker {
                    Some(marker) => {
                        format!(" (award {marker} kept in this report, not in the folder name)")
                    }
                    None => String::new(),
                };
                out.push_str(&format!(
                    "- {} -> {}{}\n",
                    m.source_path, m.target_path, award
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
    fn analyze(
        nodes: &[PlanNode],
    ) -> (Vec<crate::classify::FolderClassification>, Vec<MergedEntry>) {
        let inputs = classify_inputs_from_plan_nodes(nodes);
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
        (cs, extract(&entry_inputs))
    }

    fn hugo_plan() -> BuiltPlan {
        let nodes =
            vec![
            dir(0, None, "lib/Hugo Collection"),
            dir(1, Some(0), "lib/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season"),
            aud(
                2,
                Some(1),
                "lib/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000,
            ),
            dir(3, Some(0), "lib/Hugo Collection/Charles Stross - Neptune's Brood"),
            aud(
                4,
                Some(3),
                "lib/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000,
            ),
        ];
        let (cs, merged) = analyze(&nodes);
        build_plan(&nodes, &cs, &merged, &default_ruleset(), ROOT)
    }

    /// AC-25: the report lists every flattened pack member (one per member move
    /// with captured provenance), and rolls the counts up correctly.
    #[test]
    fn report_lists_every_flattened_member() {
        let plan = hugo_plan();
        let member_moves = plan
            .ops
            .iter()
            .filter(|o| o.provenance_json.is_some())
            .count();
        assert!(member_moves >= 2, "the Hugo pack flattens two members");

        let report = build_provenance_report(&plan);
        assert_eq!(
            report.total_members, member_moves,
            "no flattened member may be omitted from the report"
        );
        assert_eq!(report.total_packs, 1, "both members share one pack");
        assert_eq!(report.packs[0].pack_name, "Hugo Collection");
    }

    /// The award marker (the `^` caret) is carried into the report member.
    #[test]
    fn award_marker_survives_into_the_report() {
        let report = build_provenance_report(&hugo_plan());
        let jemisin = report.packs[0]
            .members
            .iter()
            .find(|m| m.source_path.contains("Jemisin"))
            .expect("the award-marked member is present");
        assert_eq!(jemisin.award_marker.as_deref(), Some("^"));
    }

    /// The generator is deterministic and both serializations are stable.
    #[test]
    fn serializations_are_deterministic() {
        let a = build_provenance_report(&hugo_plan());
        let b = build_provenance_report(&hugo_plan());
        assert_eq!(a, b);
        assert_eq!(a.to_json(), b.to_json());
        assert_eq!(a.to_markdown(), b.to_markdown());
        // Markdown states origin, never an ABS-side promise (FD-12).
        let md = a.to_markdown();
        assert!(md.contains("Hugo Collection"));
        assert!(
            !md.to_lowercase().contains("audiobookshelf")
                && !md.to_lowercase().contains("collection written")
                && !md.to_lowercase().contains("tag written"),
            "provenance copy must not promise an ABS-side change"
        );
    }

    /// A plan with no pack extractions produces an empty, valid report that says
    /// so.
    #[test]
    fn empty_plan_produces_a_valid_empty_report() {
        let empty = BuiltPlan {
            ops: Vec::new(),
            stats: crate::plan::builder::PlanStats {
                total_ops: 0,
                manual_review_ops: 0,
                per_group: Vec::new(),
            },
        };
        let report = build_provenance_report(&empty);
        assert_eq!(report.total_packs, 0);
        assert_eq!(report.total_members, 0);
        assert!(report.to_markdown().contains("No books were extracted"));
        assert!(report.to_json().contains("\"total_members\": 0"));
    }
}
