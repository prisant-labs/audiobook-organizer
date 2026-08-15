//! F-506 (dry-run HTML report): the single self-contained HTML file, written in
//! plain language for a non-engineer, that is the trust ceremony for the early
//! mini-campaign. Normative format: `docs/internal/releases/v0.3.0-planning/
//! F-506-report-spec.md`.
//!
//! # What this module is, and is not
//!
//! [`build_html_report`] is the pure GENERATOR: it takes a [`ReportInput`] (a
//! built-and-validated plan projected to a [`PlanExport`], its
//! [`ProvenanceReport`], the duplicate candidate groups, and a little identity)
//! and returns one complete, self-contained HTML document as a `String`. It does
//! no I/O and is `cfg`-free, exactly like [`crate::plan::export`] and
//! [`crate::plan::provenance`]. The font it embeds is baked into the crate with
//! `include_str!` (a base64 sidecar committed under `assets/fonts/`), so the
//! generator stays pure string building and the report needs no filesystem font.
//!
//! [`generate_and_report`] is the one impure entry point: the full-chain
//! orchestration (read snapshot -> classify -> extract -> build -> detect
//! duplicates -> validate -> persist -> export -> render HTML) that reads a
//! stored scan from the pool, persists the validated plan, and writes every
//! artifact (the F-505 exports, the F-507 provenance report, and this HTML
//! report) into the F-1002 Reports folder. It mirrors
//! [`crate::plan::validate::persist_validated_plan`]'s posture: an async,
//! DB-touching function that lives in the plan module but keeps all platform
//! reality behind seams ([`crate::plan::validate::RealFreeSpace`],
//! `crate::scan::longpath::long_paths_enabled`).
//!
//! # Self-contained, zero network (AC-20, FD-11)
//!
//! The document embeds Literata as a `data:font/woff2;base64,...` URI and inlines
//! every style and the masthead SVG. It references NO external host: there is no
//! `<link>`, no `<script>`, no `@import`, and no `://` anywhere in the output
//! (a colon-slash-slash cannot occur inside base64, so the font blob cannot
//! smuggle one in). A test greps the rendered sample to keep this true.
//!
//! # Plain-language register (PRODUCT.md Design Principle 1, FD-31)
//!
//! Books, library, duplicates, tidy-up, Archive - never operations, dedupe,
//! quarantine, manifest, dashboard. The list is revised by FD-47 ("shelves" ->
//! "library"), FD-46 ("copies" -> "duplicates" for the GROUP; members are still
//! copies) and FD-42 ("set aside" -> "Archive"). The [`PlanExport`] already
//! scrubbed the one internal-vocabulary field ("quarantine" -> "archive") at the export
//! boundary; this module adds no banned word of its own, and a test greps the
//! sample for the whole banned set.

use std::collections::BTreeSet;
use std::path::Path;

use crate::dupes::{
    book_folders_from_plan_nodes, detect_duplicates, dupe_entries_from_plan_nodes, DuplicateGroup,
};
use crate::plan::builder::CampaignGroup;
use crate::plan::export::{build_plan_export, ExportOp, PlanExport, FD10_GUARANTEE_LINE};
use crate::plan::provenance::{build_provenance_report, ProvenanceReport};

/// Stable base name (no directory) for the F-506 HTML report the Reports folder
/// writes beside the F-505 exports.
pub const PLAN_HTML_BASENAME: &str = "dry-run-report.html";

/// The Literata latin-subset woff2, base64-encoded, embedded as a compile-time
/// constant (see `assets/fonts/PROVENANCE.md`). One variable-weight file covers
/// both regular and bold. Runtime zero-network: the report inlines this into a
/// `data:` URI, never a URL.
const LITERATA_B64: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/fonts/Literata-latin.woff2.b64"
));

/// The op kinds that count as a user-facing "change" (what the summary table and
/// headline totals count). `mkdir` scaffolding and `no-op` hand-offs are not
/// changes.
const CHANGE_KINDS: &[&str] = &["move", "rename", "quarantine", "rmdir-empty"];

/// The op kinds shown in the per-group before/after example tables.
const EXAMPLE_KINDS: &[&str] = &["move", "rename"];

/// How many before/after examples to show per group before deferring the rest to
/// the complete change-list table (spec 4.4: "a small number", prototype 3-6).
const MAX_EXAMPLES: usize = 6;

/// How many decision items to list per category in the callout before deferring
/// the rest to a plain "and N more" line (keeps the callout family-readable even
/// over a library with hundreds of manual-review books).
const MAX_CALLOUT: usize = 6;

// ---- input ----

/// Everything the pure generator needs. The caller assembles this after building
/// and validating a plan (or the orchestrator does, from a stored scan).
pub struct ReportInput<'a> {
    /// The persisted plan id (for the factline and signature).
    pub plan_id: i64,
    /// The scan the plan was built over (for the factline and signature).
    pub scan_id: i64,
    /// The plan's ISO-8601 UTC `created_at` stamp; the human dateline is derived
    /// from it deterministically.
    pub created_at: &'a str,
    /// The library root path (may appear on the factline; this is a for-record
    /// report read by tier 1, per the F-506 spec 4.1).
    pub library_root: &'a str,
    /// A friendly library name for the factline (e.g. the root folder's name).
    pub library_label: &'a str,
    /// The shelf-layout label (e.g. "author first"), from the ruleset preset.
    pub shelf_layout: &'a str,
    /// The built-and-validated plan, projected to an export.
    pub export: &'a PlanExport,
    /// The F-507 provenance report for the same plan.
    pub provenance: &'a ProvenanceReport,
    /// The duplicate candidate groups (exact + version), for the copies count
    /// (groups, FD-08) and the decision callout.
    pub duplicate_groups: &'a [DuplicateGroup],
    /// Whether every illustrative figure is sample data (FD-27). `true` for the
    /// committed fixture sample; `false` for a report generated from a real plan.
    pub sample_data: bool,
}

/// Headline numbers a caller (the orchestrator, the operator) reports for a
/// generated plan. Every count states what it measures (FD-08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlineNumbers {
    /// Total user-facing changes across all seven groups (moves, renames,
    /// set-asides, empty-folder removals; plus the duplicate GROUP count for the
    /// copies group).
    pub total_changes: u64,
    /// Per user-facing group (FD-26 order): (label, change count).
    pub per_group: Vec<(String, u64)>,
    /// Exact basename+size duplicate GROUPS (FD-08 unit), held pending the check.
    pub duplicate_groups: u64,
    /// Normalized-title version candidates (each needs a keep decision).
    pub version_candidates: u64,
    /// Books the plan refuses to place without a human (`no-op(manual-review)`).
    pub manual_review: u64,
    /// Items surfaced in the decision callout (version candidates + manual
    /// review). Held duplicates are reported via `duplicate_groups`.
    pub warnings: u64,
}

// ---- small pure helpers ----

/// HTML-escape text for element/attribute content.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// The path separator this plan's paths use, inferred from the library root
/// (Windows backslash vs a POSIX/synthetic forward slash), matching the plan
/// builder's own separator inference.
fn separator_of(root: &str) -> char {
    if root.contains('\\') {
        '\\'
    } else {
        '/'
    }
}

/// Humanize a byte count: `0 bytes`, `938 bytes`, `67.9 GB`. 1024-based.
fn human_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["bytes", "KB", "MB", "GB", "TB"];
    if b < 1024 {
        return format!("{b} bytes");
    }
    let mut v = b as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

/// The final path component of `path` under separator `sep`.
fn base_name(path: &str, sep: char) -> &str {
    match path.rfind(sep) {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

/// Everything before the last separator, or the whole path if it has none.
fn parent_dir(path: &str, sep: char) -> &str {
    match path.rfind(sep) {
        Some(i) => &path[..i],
        None => path,
    }
}

/// `path` relative to the library root (root prefix and one separator removed);
/// unchanged if it does not sit under the root.
fn rel_to_root<'a>(path: &'a str, root: &str, sep: char) -> &'a str {
    match path.strip_prefix(root) {
        Some(rest) => rest.trim_start_matches(sep),
        None => path,
    }
}

/// Render a relative path as escaped segments joined by a subtle separator glyph
/// (not a raw backslash), the F-506 "After" treatment.
fn segments_html(rel: &str, sep: char) -> String {
    let parts: Vec<&str> = rel.split(sep).filter(|s| !s.is_empty()).collect();
    parts
        .iter()
        .map(|p| esc(p))
        .collect::<Vec<_>>()
        .join("<span class=\"sep\">\u{203a}</span>")
}

/// Render a "Now" name with any bracket-tag noise struck through (the F-506 `<s>`
/// treatment). The struck region is exactly the `[...]` ripper/size tag, matching
/// the visual reference; the cleaned name already stands on its own in the
/// "After" column, so meaning never depends on the strikethrough alone (FD-21).
fn strike_noise_html(name: &str) -> String {
    if let (Some(open), Some(close)) = (name.find('['), name.rfind(']')) {
        if open < close {
            let before = &name[..open];
            let tag = &name[open..=close];
            let after = &name[close + 1..];
            return format!(
                "{}<s>{}</s>{}",
                esc(before.trim_end()),
                esc(tag),
                esc(after)
            );
        }
    }
    esc(name)
}

/// Month name for the human dateline.
fn month_name(m: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ]
    .get((m as usize).saturating_sub(1))
    .copied()
    .unwrap_or("")
}

/// Weekday name from a civil date (Howard Hinnant's day count; 1970-01-01 was a
/// Thursday). Pure integer arithmetic, no date crate (matching `crate::scan`).
fn weekday_name(y: i64, m: u32, d: u32) -> &'static str {
    let yy = y - if m <= 2 { 1 } else { 0 };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = if m > 2 { m as i64 - 3 } else { m as i64 + 9 };
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let idx = (days.rem_euclid(7) + 4).rem_euclid(7); // 0 = Sunday
    [
        "Sunday",
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
    ][idx as usize]
}

/// Turn an ISO-8601 UTC stamp (`2026-07-04T18:14:03Z`) into a human dateline
/// (`Friday, July 4, 2026 at 6:14 pm`). Falls back to the raw stamp if it does
/// not parse. Pure and deterministic.
fn human_dateline(iso: &str) -> String {
    // Expect at least YYYY-MM-DDTHH:MM.
    let bytes = iso.as_bytes();
    let parse = || -> Option<(i64, u32, u32, u32, u32)> {
        let y: i64 = iso.get(0..4)?.parse().ok()?;
        let mo: u32 = iso.get(5..7)?.parse().ok()?;
        let d: u32 = iso.get(8..10)?.parse().ok()?;
        let h: u32 = iso.get(11..13)?.parse().ok()?;
        let mi: u32 = iso.get(14..16)?.parse().ok()?;
        Some((y, mo, d, h, mi))
    };
    if bytes.len() < 16 {
        return iso.to_string();
    }
    match parse() {
        Some((y, mo, d, h, mi)) if (1..=12).contains(&mo) => {
            let ampm = if h < 12 { "am" } else { "pm" };
            let h12 = match h % 12 {
                0 => 12,
                other => other,
            };
            format!(
                "{}, {} {}, {} at {}:{:02} {} UTC",
                weekday_name(y, mo, d),
                month_name(mo),
                d,
                y,
                h12,
                mi,
                ampm
            )
        }
        _ => iso.to_string(),
    }
}

// ---- per-group display metadata ----

/// The ops belonging to one user-facing group label.
///
/// Resolves each op's STORED group token rather than comparing it to the current
/// label. An export written before a rename stores the old word (FD-46's "copies"),
/// and raw equality would file none of its operations under any heading while the
/// summary table, read from stored stats, still counted them.
///
/// This helper feeds the counts, the examples, the sizes and the complete table,
/// so getting it wrong loses a group's operations in four places at once. The same
/// bug was fixed in `export::to_markdown` first; this is the second occurrence, in
/// a different function, which is why the resolution now lives in one shared place
/// ([`CampaignGroup::from_stored_token`]) rather than being re-derived per reader.
fn group_ops<'a>(export: &'a PlanExport, label: &str) -> Vec<&'a ExportOp> {
    let want = CampaignGroup::from_stored_token(label);
    export
        .ops
        .iter()
        .filter(|o| match want {
            // Both sides resolved: a stored "copies" matches a current "duplicates".
            Some(group) => CampaignGroup::from_stored_token(&o.group) == Some(group),
            // An unrecognized label is never silently widened to match everything.
            None => o.group == label,
        })
        .collect()
}

/// Count of user-facing changes in a group (moves/renames/set-asides/removals).
fn group_change_count(export: &PlanExport, label: &str) -> u64 {
    group_ops(export, label)
        .iter()
        .filter(|o| CHANGE_KINDS.contains(&o.kind.as_str()))
        .count() as u64
}

/// Exact basename+size duplicate groups (the FD-08 copies unit).
fn exact_dup_groups<'a>(input: &ReportInput<'a>) -> Vec<&'a DuplicateGroup> {
    input
        .duplicate_groups
        .iter()
        .filter(|g| g.is_exact())
        .collect()
}

/// Every duplicate CANDIDATE group: exact basename+size groups, plus the
/// `F-1110` folder groups whose members agree at least on the `AC-51`
/// fingerprint.
///
/// This is the FD-08 counted unit for the Copies row. It is deliberately wider
/// than [`exact_dup_groups`]: counting only exact groups reported zero for a
/// book split across twelve files, which is the silent under-reporting `F-1110`
/// closes.
fn duplicate_candidate_groups<'a>(input: &ReportInput<'a>) -> Vec<&'a DuplicateGroup> {
    input
        .duplicate_groups
        .iter()
        .filter(|g| g.is_duplicate_candidate())
        .collect()
}

/// Normalized-title version candidates that did NOT reach candidate strength
/// (each needs a keep decision).
///
/// Excludes folder groups that are now counted as duplicate candidates, so a
/// group is listed in exactly one place and the two figures never overlap.
fn version_candidate_groups<'a>(input: &ReportInput<'a>) -> Vec<&'a DuplicateGroup> {
    input
        .duplicate_groups
        .iter()
        .filter(|g| g.is_version_candidate() && !g.is_duplicate_candidate())
        .collect()
}

/// The displayed change count for a group (copies counts GROUPS, FD-08).
fn displayed_count(input: &ReportInput, group: CampaignGroup) -> u64 {
    if group == CampaignGroup::Copies {
        duplicate_candidate_groups(input).len() as u64
    } else {
        group_change_count(input.export, group.label())
    }
}

/// The summary-table headline for a group (plain-language, counts folded in).
fn group_headline(input: &ReportInput, group: CampaignGroup) -> String {
    let n = displayed_count(input, group);
    match group {
        CampaignGroup::Staging => {
            // The staging group is a deliberate 0-change no-op: the piles are
            // left alone now and tidied in a later step. The headline counts the
            // PILES found (a meaningful number) rather than changes made (always
            // 0 here), so the row reads coherently beside its 0 in the Changes
            // column.
            let piles = group_ops(input.export, group.label()).len();
            if piles == 1 {
                "Leave the 1 sorting pile for a later step".to_string()
            } else {
                format!("Leave the {piles} sorting piles for a later step")
            }
        }
        CampaignGroup::LooseBooks => format!("Give {n} loose books their own folders"),
        CampaignGroup::MessyNames => format!("Clean up {n} messy folder names"),
        CampaignGroup::BoxSets => {
            let folders = distinct_source_parents(input.export, group.label());
            format!("Split {folders} box-set folders into separate books")
        }
        CampaignGroup::Bundles => {
            let packs = input.provenance.total_packs;
            format!("Unpack {packs} collection bundles into your library")
        }
        CampaignGroup::Copies => format!("Move {n} duplicate copies to the Archive"),
        CampaignGroup::EmptyFolders => format!("Sweep out {n} empty folders"),
    }
}

/// The one-line description under a group headline.
fn group_description(group: CampaignGroup) -> &'static str {
    match group {
        CampaignGroup::Staging => {
            "the sorting piles are left alone for now; their books are tidied in a later step once you have reviewed them"
        }
        CampaignGroup::LooseBooks => "each file moves into a folder named for its author and title",
        CampaignGroup::MessyNames => "removes leftover labels; the books do not move",
        CampaignGroup::BoxSets => "each book in the set gets its own folder",
        CampaignGroup::Bundles => "books are lifted out of download packages into your library",
        CampaignGroup::Copies => "held until every group is double-checked byte by byte",
        CampaignGroup::EmptyFolders => "placeholders that hold no audio at all",
    }
}

/// The section heading for a group's before/after table.
fn group_section_title(group: CampaignGroup) -> &'static str {
    match group {
        CampaignGroup::Staging => "The sorting piles wait for a later step",
        CampaignGroup::LooseBooks => "Loose books get their own folders",
        CampaignGroup::MessyNames => "Messy names come clean",
        CampaignGroup::BoxSets => "Box sets become separate books",
        CampaignGroup::Bundles => "Collection bundles into your library",
        CampaignGroup::Copies => "Duplicate copies moved to the Archive",
        CampaignGroup::EmptyFolders => "Empty folders swept out",
    }
}

/// Count of distinct source parent folders among a group's move ops (used for the
/// "N box-set folders" headline).
fn distinct_source_parents(export: &PlanExport, label: &str) -> usize {
    let sep_hint = export
        .ops
        .iter()
        .find(|o| !o.source_path.is_empty())
        .map(|o| separator_of(&o.source_path))
        .unwrap_or('/');
    let set: BTreeSet<&str> = group_ops(export, label)
        .iter()
        .filter(|o| o.kind == "move")
        .map(|o| parent_dir(&o.source_path, sep_hint))
        .collect();
    set.len()
}

// ---- headline numbers ----

/// Compute the headline numbers for a report input (pure; the same counts the
/// document renders).
pub fn headline_numbers(input: &ReportInput) -> HeadlineNumbers {
    let per_group: Vec<(String, u64)> = CampaignGroup::ALL
        .iter()
        .map(|&g| (g.label().to_string(), displayed_count(input, g)))
        .collect();
    let total_changes = per_group.iter().map(|(_, n)| *n).sum();
    let duplicate_groups = exact_dup_groups(input).len() as u64;
    let version_candidates = version_candidate_groups(input).len() as u64;
    let manual_review = input
        .export
        .ops
        .iter()
        .filter(|o| o.kind == "no-op" && o.kind_reason.as_deref() == Some("manual-review"))
        .count() as u64;
    HeadlineNumbers {
        total_changes,
        per_group,
        duplicate_groups,
        version_candidates,
        manual_review,
        warnings: version_candidates + manual_review,
    }
}

// ---- the generator ----

/// Build the complete, self-contained dry-run HTML report (F-506). Pure: no I/O,
/// deterministic for a given input. The result is a full HTML document
/// (`<!doctype html> ... </html>`) that opens correctly from a `file://` path
/// with networking disabled.
pub fn build_html_report(input: &ReportInput) -> String {
    let sep = separator_of(input.library_root);
    let nums = headline_numbers(input);
    let sample = input.sample_data;
    let sample_tag = if sample {
        " <span class=\"sample\">(sample data)</span>"
    } else {
        ""
    };

    let mut h = String::with_capacity(64 * 1024);

    // ---- document head ----
    h.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    h.push_str("<meta charset=\"UTF-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    h.push_str(&format!(
        "<title>Dry run report - Audiobook Organizer - {}</title>\n",
        esc(&human_dateline(input.created_at))
    ));
    push_style(&mut h);
    h.push_str("</head>\n<body>\n<article class=\"sheet\">\n");

    // ---- masthead + dateline + factline (4.1) ----
    push_masthead(&mut h, input);

    // ---- lead (4.2) ----
    push_lead(&mut h, &nums, sample_tag);

    // ---- seven-group summary table (4.3) ----
    push_summary_table(&mut h, input);

    // ---- before/after example tables (4.4) ----
    push_example_tables(&mut h, input, sep);

    // ---- decision callout (4.5) ----
    push_warnings(&mut h, input);

    // ---- what will not happen (4.6) ----
    push_guarantees(&mut h);

    // ---- provenance (4.7) ----
    push_provenance(&mut h, input);

    // ---- complete change list (4.8) ----
    push_complete_table(&mut h, input);

    // ---- signature (4.9) ----
    push_signature(&mut h, input, &nums);

    h.push_str("</article>\n</body>\n</html>\n");
    h
}

/// The single light "paper" theme, serif register, print + responsive rules, and
/// the embedded Literata `@font-face` (FD-11/FD-28). Adapted from the ratified
/// prototype (`_local/gui/06-dryrun-report.html`) with the Google Fonts `<link>`
/// removed and the font inlined as a `data:` URI.
fn push_style(h: &mut String) {
    h.push_str("<style>\n");
    h.push_str("@font-face{font-family:'Literata';font-style:normal;font-weight:400 700;font-display:swap;src:url(data:font/woff2;base64,");
    h.push_str(LITERATA_B64.trim());
    h.push_str(") format('woff2');}\n");
    h.push_str(
        r#":root{
  --serif:"Literata",Georgia,"Times New Roman",serif;
  --sans:"Segoe UI Variable Text","Segoe UI",system-ui,sans-serif;
  --mono:"Cascadia Code",Consolas,monospace;
  --paper:#ffffff; --ink:#26252e; --ink-2:#4b4a56; --ink-3:#6d6c79;
  --border:#e4e4ea; --border-2:#cfcfd8; --tint:#f5f5f8;
  --primary:#4a3fb0;
  --good:#2f7a4c; --good-bg:#e7f2ea;
  --warn:#7c5a10; --warn-bg:#f5edd8;
}
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:var(--serif);background:#f2f2f5;color:var(--ink);line-height:1.6;-webkit-font-smoothing:antialiased;padding:40px 16px}
.sheet{max-width:820px;margin:0 auto;background:var(--paper);border:1px solid var(--border);border-radius:4px;padding:56px 64px 48px;box-shadow:0 1px 3px rgba(30,28,45,.08),0 12px 32px rgba(30,28,45,.07)}
.masthead{display:flex;align-items:center;gap:9px;font-family:var(--sans);font-size:12.5px;color:var(--ink-2)}
h1{font-size:34px;font-weight:600;letter-spacing:-.015em;margin:18px 0 4px;text-wrap:balance}
.dateline{font-family:var(--sans);font-size:13px;color:var(--ink-2)}
.factline{font-family:var(--sans);font-size:12.5px;color:var(--ink-3);margin-top:3px;word-break:break-word}
.rule{border:0;border-top:2px solid var(--ink);margin:26px 0 24px}
.rule.thin{border-top:1px solid var(--border);margin:30px 0 26px}
p.lead{font-size:16.5px;line-height:1.65;max-width:66ch;text-wrap:pretty}
p.lead b{font-weight:600}
.sample{font-family:var(--sans);font-size:12px;color:var(--warn);background:var(--warn-bg);border-radius:4px;padding:1px 7px;white-space:nowrap}
p{font-size:14.5px;color:var(--ink-2);max-width:72ch;text-wrap:pretty}
p + p{margin-top:10px}
h2{font-size:21px;font-weight:600;letter-spacing:-.01em;margin:34px 0 10px;text-wrap:balance}
h2 .count{font-family:var(--sans);font-size:12.5px;font-weight:400;color:var(--ink-3);margin-left:10px}
h3.sub{font-family:var(--sans);font-size:13px;font-weight:600;color:var(--ink-2);margin:20px 0 4px}
table{width:100%;border-collapse:collapse;margin-top:14px;font-family:var(--sans)}
th{font-size:11px;font-weight:600;color:var(--ink-3);text-align:left;padding:0 12px 8px 0;border-bottom:1.5px solid var(--ink);letter-spacing:.01em}
th.r,td.r{text-align:right;padding-right:0}
td{font-size:13px;color:var(--ink-2);padding:9px 12px 9px 0;border-bottom:1px solid var(--border);vertical-align:top}
td b{color:var(--ink);font-weight:600}
td.num{font-variant-numeric:tabular-nums;white-space:nowrap}
.chip{display:inline-block;font-size:11px;border-radius:999px;padding:2px 10px;white-space:nowrap}
.chip.in{color:var(--good);background:var(--good-bg)}
.chip.hold{color:var(--warn);background:var(--warn-bg)}
.ba{table-layout:fixed}
.ba td{font-size:12.5px;line-height:1.55;word-break:break-word}
.ba .arrowcell{width:26px;color:var(--ink-3);text-align:center;padding:9px 0}
.ba .old{color:var(--ink-2)}
.ba .old s{color:var(--ink-3);text-decoration-color:rgba(163,75,42,.6)}
.ba .new{color:var(--ink)}
.ba .new .sep{color:var(--ink-3);padding:0 3px}
.moreline{font-family:var(--sans);font-size:12px;color:var(--ink-3);padding:10px 0 0}
.note{background:var(--tint);border:1px solid var(--border);border-radius:6px;padding:16px 20px;margin-top:16px}
.note h3{font-family:var(--sans);font-size:13px;font-weight:600;margin-bottom:6px}
.note p{font-size:13px}
.note.warn{background:var(--warn-bg);border-color:#e7d5a6}
.note.warn h3{color:var(--warn)}
ul.plain{margin:12px 0 0 2px;list-style:none}
ul.plain li{font-family:var(--sans);font-size:13.5px;color:var(--ink-2);padding:5px 0 5px 26px;position:relative}
ul.plain li::before{content:"";position:absolute;left:2px;top:11px;width:12px;height:7px;border-left:2.2px solid var(--good);border-bottom:2.2px solid var(--good);transform:rotate(-45deg)}
ul.decisions{margin:8px 0 0 2px;list-style:none}
ul.decisions li{font-family:var(--sans);font-size:13px;color:var(--ink-2);padding:4px 0 4px 16px;position:relative}
ul.decisions li::before{content:"\2022";position:absolute;left:2px;color:var(--warn)}
.prov h3{font-family:var(--sans);font-size:14px;font-weight:600;color:var(--ink);margin:18px 0 4px}
.prov ul{margin:0 0 0 2px;list-style:none}
.prov li{font-family:var(--sans);font-size:12.5px;color:var(--ink-2);padding:3px 0 3px 16px;position:relative}
.prov li::before{content:"\203a";position:absolute;left:2px;color:var(--ink-3)}
.prov .award{color:var(--warn)}
.cltable td{font-size:11.5px;font-family:var(--sans);vertical-align:top}
.cltable td.path{font-family:var(--mono);font-size:11px;color:var(--ink-3);word-break:break-all}
.cltable tr.group-head td{font-family:var(--sans);font-size:12.5px;font-weight:600;color:var(--ink);background:var(--tint);border-bottom:1.5px solid var(--border-2);padding-top:14px}
.cltable tr.op-row .flag{color:var(--warn);font-weight:600}
.sig{margin-top:36px;padding-top:18px;border-top:1px solid var(--border);font-family:var(--sans);font-size:12px;color:var(--ink-3);display:flex;justify-content:space-between;gap:12px;flex-wrap:wrap}
@media print{
  body{background:#fff;padding:0}
  .sheet{border:0;box-shadow:none;padding:24px 8px;max-width:none}
  h2{break-after:avoid}
  table{break-inside:auto}
  tr{break-inside:avoid}
  .note{break-inside:avoid}
}
@media (max-width:720px){.sheet{padding:32px 22px}}
"#,
    );
    h.push_str("</style>\n");
}

fn push_masthead(h: &mut String, input: &ReportInput) {
    h.push_str("<div class=\"masthead\">");
    h.push_str("<svg width=\"16\" height=\"16\" viewBox=\"0 0 24 24\" aria-hidden=\"true\"><path d=\"M4 5.5C6.5 4 9 4 12 5.8c3-1.8 5.5-1.8 8-.3V18c-2.5-1.5-5-1.5-8 .3-3-1.8-5.5-1.8-8-.3Z\" fill=\"none\" stroke=\"#4a3fb0\" stroke-width=\"1.8\" stroke-linejoin=\"round\"/><path d=\"M12 5.8v12.5\" stroke=\"#4a3fb0\" stroke-width=\"1.8\"/></svg>");
    h.push_str("Audiobook Organizer</div>\n");
    h.push_str("<h1>Dry run report</h1>\n");
    h.push_str(&format!(
        "<div class=\"dateline\">{}</div>\n",
        esc(&human_dateline(input.created_at))
    ));
    h.push_str(&format!(
        "<div class=\"factline\">Library: {} &nbsp;&middot;&nbsp; Layout: {} &nbsp;&middot;&nbsp; Plan {}, scan {}</div>\n",
        esc(input.library_root),
        esc(input.shelf_layout),
        input.plan_id,
        input.scan_id
    ));
    h.push_str("<hr class=\"rule\">\n");
}

fn push_lead(h: &mut String, nums: &HeadlineNumbers, sample_tag: &str) {
    h.push_str("<p class=\"lead\">This is a preview. <b>Nothing has been changed.</b> ");
    if nums.total_changes == 0 {
        h.push_str(&format!(
            "The dry run worked out that this library already matches the chosen layout: there are <b>no changes</b> to make.{sample_tag}"
        ));
    } else {
        h.push_str(&format!(
            "The dry run worked out <b>{} change{}</b>{} that would make the library read cleanly in Audiobookshelf. They fall into seven groups, summarised just below and then shown in detail.",
            nums.total_changes,
            if nums.total_changes == 1 { "" } else { "s" },
            sample_tag
        ));
        let decisions = nums.warnings;
        if decisions > 0 {
            h.push_str(&format!(
                " {} book{} need{} a decision from you before anything is touched; {} called out below.",
                decisions,
                if decisions == 1 { "" } else { "s" },
                if decisions == 1 { "s" } else { "" },
                if decisions == 1 { "it is" } else { "they are" }
            ));
        }
    }
    h.push_str("</p>\n");
}

fn push_summary_table(h: &mut String, input: &ReportInput) {
    h.push_str("<h2>The seven groups at a glance</h2>\n");
    h.push_str("<table>\n<thead><tr><th>What would happen</th><th class=\"num\">Changes</th><th class=\"num\">Size</th><th class=\"r\">Status</th></tr></thead>\n<tbody>\n");
    for &group in CampaignGroup::ALL {
        let count = displayed_count(input, group);
        let size = group_size_cell(input, group);
        // Staging is a 0-change no-op held for a later step, so its row carries a
        // "later" hold chip rather than the "included" chip: that makes the 0 in
        // the Changes column read as a deliberate deferral, not an empty result.
        let (chip_class, chip_text) = match group {
            CampaignGroup::Copies => ("hold", "checking"),
            CampaignGroup::Staging => ("hold", "later"),
            _ => ("in", "included"),
        };
        h.push_str(&format!(
            "<tr><td><b>{}</b><br>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"r\"><span class=\"chip {}\">{}</span></td></tr>\n",
            esc(&group_headline(input, group)),
            esc(group_description(group)),
            count,
            esc(&size),
            chip_class,
            chip_text
        ));
    }
    h.push_str("</tbody>\n</table>\n");
}

/// The size cell for a group's summary row.
fn group_size_cell(input: &ReportInput, group: CampaignGroup) -> String {
    if group == CampaignGroup::MessyNames {
        return "renames".to_string();
    }
    if group == CampaignGroup::Copies {
        let bytes: u64 = exact_dup_groups(input).iter().map(|g| g.total_bytes).sum();
        return if bytes > 0 {
            human_bytes(bytes)
        } else {
            "0 bytes".to_string()
        };
    }
    let bytes: u64 = group_ops(input.export, group.label())
        .iter()
        .map(|o| o.byte_size.max(0) as u64)
        .sum();
    if bytes > 0 {
        human_bytes(bytes)
    } else {
        "0 bytes".to_string()
    }
}

fn push_example_tables(h: &mut String, input: &ReportInput, sep: char) {
    let mut first = true;
    for &group in CampaignGroup::ALL {
        let examples: Vec<&ExportOp> = group_ops(input.export, group.label())
            .into_iter()
            .filter(|o| EXAMPLE_KINDS.contains(&o.kind.as_str()))
            .collect();
        if examples.is_empty() {
            continue;
        }
        if first {
            h.push_str("<hr class=\"rule thin\">\n");
            first = false;
        }
        let total = examples.len();
        let shown = total.min(MAX_EXAMPLES);
        let noun = match (group == CampaignGroup::MessyNames, total == 1) {
            (true, true) => "rename",
            (true, false) => "renames",
            (false, true) => "change",
            (false, false) => "changes",
        };
        h.push_str(&format!(
            "<h2>{}<span class=\"count\">{} {} &middot; showing {}</span></h2>\n",
            esc(group_section_title(group)),
            total,
            noun,
            shown
        ));
        h.push_str("<table class=\"ba\">\n<thead><tr><th>Now</th><th></th><th>After</th></tr></thead>\n<tbody>\n");
        let mut has_award = false;
        for op in examples.iter().take(shown) {
            if op.source_path.contains('^') || op.target_path.contains('^') {
                has_award = true;
            }
            let now_html = example_now(op, input.library_root, sep);
            let after_html = example_after(op, input.library_root, sep);
            h.push_str(&format!(
                "<tr><td class=\"old\">{}</td><td class=\"arrowcell\">&rarr;</td><td class=\"new\">{}</td></tr>\n",
                now_html, after_html
            ));
        }
        h.push_str("</tbody>\n</table>\n");
        if total > shown {
            let more = total - shown;
            let mut line = format!(
                "&hellip;and {} more. Every one is listed in the complete table at the end of this file.",
                more
            );
            if has_award || group == CampaignGroup::MessyNames {
                line.push_str(" The caret (for example &ldquo;2016^&rdquo;) marks an award winner; awards are kept in this report rather than in folder names.");
            }
            h.push_str(&format!("<div class=\"moreline\">{line}</div>\n"));
        } else if has_award {
            h.push_str("<div class=\"moreline\">The caret marks an award winner; awards are kept in this report rather than in folder names.</div>\n");
        }
    }
}

/// The "Now" cell for a before/after example.
fn example_now(op: &ExportOp, root: &str, sep: char) -> String {
    if op.kind == "rename" {
        strike_noise_html(base_name(&op.source_path, sep))
    } else {
        segments_html(rel_to_root(&op.source_path, root, sep), sep)
    }
}

/// The "After" cell for a before/after example.
fn example_after(op: &ExportOp, root: &str, sep: char) -> String {
    if op.kind == "rename" {
        return esc(base_name(&op.target_path, sep));
    }
    // A pack-member folder move lands the folder itself at the target; a file
    // move lands the file inside a new folder, so show the destination FOLDER.
    if op.group == CampaignGroup::Bundles.label() {
        segments_html(rel_to_root(&op.target_path, root, sep), sep)
    } else {
        let dest_dir = parent_dir(&op.target_path, sep);
        segments_html(rel_to_root(dest_dir, root, sep), sep)
    }
}

fn push_warnings(h: &mut String, input: &ReportInput) {
    let versions = version_candidate_groups(input);
    let held = exact_dup_groups(input);
    let manual: Vec<&ExportOp> = input
        .export
        .ops
        .iter()
        .filter(|o| o.kind == "no-op" && o.kind_reason.as_deref() == Some("manual-review"))
        .collect();

    let sep = separator_of(input.library_root);

    if versions.is_empty() && held.is_empty() && manual.is_empty() {
        h.push_str("<div class=\"note\">\n<h3>Nothing needs a decision</h3>\n<p>No book in this plan is waiting on a choice from you. Every change above is ready to review.</p>\n</div>\n");
        return;
    }

    h.push_str(
        "<div class=\"note warn\">\n<h3>What needs your eyes before anything happens</h3>\n",
    );

    // Held duplicates are the tool's job to check, not a per-item decision, so
    // they are summarised in one line (never one bullet per group, which would
    // bury the callout under hundreds of near-identical rows).
    if !held.is_empty() {
        let copies: usize = held.iter().map(|g| g.copies()).sum();
        h.push_str(&format!(
            "<p><b>{} group{} of identical copies</b> ({} file{} in all) are held until the byte-by-byte check finishes, so you never end up with two folders for one book. Nothing moves to the Archive until you confirm.</p>\n",
            held.len(),
            if held.len() == 1 { "" } else { "s" },
            copies,
            if copies == 1 { "" } else { "s" }
        ));
    }

    // The real per-item decisions: version clashes and books that could not be
    // placed. Both are capped with a plain-language "and N more" tail so the
    // callout stays readable no matter how large the library is.
    if !versions.is_empty() {
        h.push_str("<h3 class=\"sub\">Two versions that would land in one folder</h3>\n<ul class=\"decisions\">\n");
        for g in versions.iter().take(MAX_CALLOUT) {
            let name = dup_display_name(g, sep);
            h.push_str(&format!(
                "<li><b>{}</b> has more than one version that would land in the same folder. Pick a keeper, or keep both as separate editions. Neither is touched until you decide.</li>\n",
                esc(&name)
            ));
        }
        h.push_str("</ul>\n");
        if versions.len() > MAX_CALLOUT {
            h.push_str(&format!(
                "<div class=\"moreline\">&hellip;and {} more version clash{}. Every one is in the complete list below.</div>\n",
                versions.len() - MAX_CALLOUT,
                if versions.len() - MAX_CALLOUT == 1 { "" } else { "es" }
            ));
        }
    }

    if !manual.is_empty() {
        h.push_str("<h3 class=\"sub\">Books to sort by hand</h3>\n<ul class=\"decisions\">\n");
        for op in manual.iter().take(MAX_CALLOUT) {
            let name = base_name(&op.source_path, sep);
            h.push_str(&format!(
                "<li><b>{}</b> could not be placed automatically: {} It is left exactly where it is for you to sort by hand.</li>\n",
                esc(name),
                esc(&op.rationale)
            ));
        }
        h.push_str("</ul>\n");
        if manual.len() > MAX_CALLOUT {
            h.push_str(&format!(
                "<div class=\"moreline\">&hellip;and {} more book{} the plan left in place for you. Every one is listed in the complete table below.</div>\n",
                manual.len() - MAX_CALLOUT,
                if manual.len() - MAX_CALLOUT == 1 { "" } else { "s" }
            ));
        }
    }

    h.push_str("</div>\n");
}

/// A readable name for a duplicate/version group: the containing FOLDER of the
/// first member (the book), falling back to the member file name. Generic track
/// files (`00.mp3`) live inside a book folder, so the folder name is the
/// meaningful identity a reader recognises.
fn dup_display_name(g: &DuplicateGroup, sep: char) -> String {
    let first = g.members.first().map(|m| m.path.as_str()).unwrap_or("");
    let parent = parent_dir(first, sep);
    let folder = base_name(parent, sep);
    if folder.is_empty() || parent == first {
        base_name(first, sep).to_string()
    } else {
        folder.to_string()
    }
}

fn push_guarantees(h: &mut String) {
    h.push_str("<h2>What will not happen</h2>\n");
    // The FD-10 canon deletion-guarantee line, verbatim.
    h.push_str(&format!("<p><b>{}</b></p>\n", esc(FD10_GUARANTEE_LINE)));
    h.push_str("<ul class=\"plain\">\n");
    h.push_str("<li>Duplicate copies would move to the Archive folder beside the library, where they stay until you empty it yourself.</li>\n");
    h.push_str("<li>No file contents are edited. Books are moved and folders renamed; the audio inside is untouched.</li>\n");
    h.push_str("<li>An undo record is written as changes happen, so the whole tidy-up can be reversed.</li>\n");
    h.push_str("<li>Nothing leaves this computer. The dry run, this report, and the tidy-up itself all run locally.</li>\n");
    h.push_str("</ul>\n");
}

fn push_provenance(h: &mut String, input: &ReportInput) {
    let prov = input.provenance;
    h.push_str("<h2>Where these books came from</h2>\n");
    if prov.packs.is_empty() {
        h.push_str("<p>No books were lifted out of a box set or collection in this plan, so there is no origin to record here.</p>\n");
        return;
    }
    h.push_str(&format!(
        "<p>This preview lifts {} book{} out of {} box set{} or collection{}. Each book&rsquo;s origin is kept here so nothing about where it came from is lost. Pack and award membership is recorded in this report only; nothing is written back to your audiobook app.</p>\n",
        prov.total_members,
        if prov.total_members == 1 { "" } else { "s" },
        prov.total_packs,
        if prov.total_packs == 1 { "" } else { "s" },
        if prov.total_packs == 1 { "" } else { "s" }
    ));
    h.push_str("<div class=\"prov\">\n");
    for pack in &prov.packs {
        h.push_str(&format!("<h3>{}</h3>\n<ul>\n", esc(&pack.pack_name)));
        for m in &pack.members {
            let name = base_name(&m.target_path, separator_of(input.library_root));
            match &m.award_marker {
                Some(marker) => h.push_str(&format!(
                    "<li>{} <span class=\"award\">(award {} kept in this report, not in the folder name)</span></li>\n",
                    esc(name),
                    esc(marker)
                )),
                None => h.push_str(&format!("<li>{}</li>\n", esc(name))),
            }
        }
        h.push_str("</ul>\n");
    }
    h.push_str("</div>\n");
}

fn push_complete_table(h: &mut String, input: &ReportInput) {
    let export = input.export;
    h.push_str(&format!(
        "<h2>Complete change list<span class=\"count\">{} row{}</span></h2>\n",
        export.ops.len(),
        if export.ops.len() == 1 { "" } else { "s" }
    ));
    if export.ops.is_empty() {
        h.push_str("<p>This plan makes no changes, so the complete list is empty. The library already matches the chosen layout.</p>\n");
        return;
    }
    h.push_str("<p>Every single change is listed below, grouped the same seven ways, with nothing left out. Raw folder paths are shown here for the record.</p>\n");
    h.push_str("<table class=\"cltable\">\n<thead><tr><th>Book / item</th><th>From</th><th>To</th><th>Note</th></tr></thead>\n<tbody>\n");
    let sep = separator_of(input.library_root);
    for &group in CampaignGroup::ALL {
        let ops = group_ops(export, group.label());
        if ops.is_empty() {
            continue;
        }
        h.push_str(&format!(
            "<tr class=\"group-head\"><td colspan=\"4\">{} &middot; {} row{}</td></tr>\n",
            esc(group.label()),
            ops.len(),
            if ops.len() == 1 { "" } else { "s" }
        ));
        for op in ops {
            let title = if !op.target_path.is_empty() {
                base_name(&op.target_path, sep)
            } else {
                base_name(&op.source_path, sep)
            };
            let from = if op.source_path.is_empty() {
                "-".to_string()
            } else {
                esc(&op.source_path)
            };
            let to = if op.target_path.is_empty() {
                "-".to_string()
            } else {
                esc(&op.target_path)
            };
            let mut note = esc(&op.rationale);
            if op.validation_state != "valid" {
                let code = op.validation_reason.as_deref().unwrap_or("see review");
                note.push_str(&format!(
                    " <span class=\"flag\">[{}: {}]</span>",
                    esc(&op.validation_state),
                    esc(code)
                ));
            }
            h.push_str(&format!(
                "<tr class=\"op-row\"><td>{}</td><td class=\"path\">{}</td><td class=\"path\">{}</td><td>{}</td></tr>\n",
                esc(title),
                from,
                to,
                note
            ));
        }
    }
    h.push_str("</tbody>\n</table>\n");
}

fn push_signature(h: &mut String, input: &ReportInput, _nums: &HeadlineNumbers) {
    h.push_str("<div class=\"sig\">\n");
    h.push_str(&format!(
        "<span>Generated locally by Audiobook Organizer &middot; plan {} over scan {} &middot; this report is a preview, not a receipt.</span>\n",
        input.plan_id, input.scan_id
    ));
    h.push_str(&format!(
        "<span>Complete change list: {} row{}, included above in this file.</span>\n",
        input.export.ops.len(),
        if input.export.ops.len() == 1 { "" } else { "s" }
    ));
    h.push_str("</div>\n");
}

// ---- full-chain orchestration (impure) ----

/// The artifacts and numbers a full [`generate_and_report`] run produced.
#[derive(Debug, Clone)]
pub struct GeneratedReport {
    /// The persisted plan id.
    pub plan_id: i64,
    /// Where the five F-505/F-507 artifacts and the HTML report were written
    /// (the deterministic Reports-folder subfolder).
    pub written: crate::reports::WrittenPlanReports,
    /// The HTML report path (inside the same subfolder).
    pub html_path: std::path::PathBuf,
    /// The rendered HTML (returned so a caller can copy it elsewhere without a
    /// second read).
    pub html: String,
    /// The headline numbers for the plan.
    pub numbers: HeadlineNumbers,
}

/// Why a [`generate_and_report`] run could not complete.
#[derive(Debug)]
pub enum GenerateError {
    /// A database read or the plan insert failed.
    Db(sqlx::Error),
    /// Writing an artifact to the Reports folder failed.
    Io(std::io::Error),
    /// The referenced scan or ruleset was not found.
    NotFound(&'static str),
    /// An engine step returned an [`crate::error::AppError`] (a snapshot read, or
    /// a ruleset body that failed to parse/validate).
    App(crate::error::AppError),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateError::Db(e) => write!(f, "database error: {e}"),
            GenerateError::Io(e) => write!(f, "reports-folder write failed: {e}"),
            GenerateError::NotFound(what) => write!(f, "{what} not found"),
            GenerateError::App(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for GenerateError {}
impl From<sqlx::Error> for GenerateError {
    fn from(e: sqlx::Error) -> Self {
        GenerateError::Db(e)
    }
}
impl From<std::io::Error> for GenerateError {
    fn from(e: std::io::Error) -> Self {
        GenerateError::Io(e)
    }
}
impl From<crate::error::AppError> for GenerateError {
    fn from(e: crate::error::AppError) -> Self {
        GenerateError::App(e)
    }
}

/// Map a [`GenerateError`] onto the stable IPC taxonomy (v0.4.0 Phase 5,
/// `plan_generate` command). A `GenerateError::App` already carries a real
/// [`crate::error::AppError`] (for example `RulesetInvalid`) and passes
/// through unchanged so the caller sees the precise family-safe code; every
/// other variant (a database error, a Reports-folder write failure, or a
/// missing scan/ruleset) folds into [`crate::error::AppError::PlanGenerationFailed`]
/// with a developer-facing detail string.
impl From<GenerateError> for crate::error::AppError {
    fn from(e: GenerateError) -> Self {
        match e {
            GenerateError::App(app_err) => app_err,
            other => crate::error::AppError::PlanGenerationFailed {
                detail: other.to_string(),
            },
        }
    }
}

/// The friendly shelf-layout label for a naming preset.
fn shelf_layout_label(preset: crate::plan::templates::Preset) -> &'static str {
    use crate::plan::templates::Preset;
    match preset {
        Preset::AbsAuthorFirst => "author first",
        Preset::TitleFirst => "title first",
        Preset::HybridGenre => "genre folders",
    }
}

/// Full-chain orchestration (F-506 signature entry point): from a stored scan and
/// a stored ruleset, build -> detect duplicates -> validate -> persist -> export
/// -> render the HTML report, writing every artifact into the F-1002 Reports
/// folder under `app_data_dir`. Read-only against the real filesystem: it reads
/// the SNAPSHOT (never the live library) and writes only into the Reports folder.
///
/// This is the function the real-library gate test drives. It confines platform
/// reality to the existing seams (`RealFreeSpace`, `long_paths_enabled`), so it
/// carries no `cfg` of its own.
pub async fn generate_and_report(
    pool: &sqlx::SqlitePool,
    scan_id: i64,
    ruleset_id: i64,
    app_data_dir: &Path,
) -> Result<GeneratedReport, GenerateError> {
    let built = build_and_persist_plan(pool, scan_id, ruleset_id).await?;

    // 7. Build the export projections.
    let export = build_plan_export(
        built.plan_id,
        &built.created_at,
        "draft",
        &built.plan,
        &built.verdicts,
    );
    let provenance = build_provenance_report(&built.plan);

    // 8. Render the HTML report.
    let library_label = base_name(&built.root_path, separator_of(&built.root_path)).to_string();
    let input = ReportInput {
        plan_id: built.plan_id,
        scan_id,
        created_at: &built.created_at,
        library_root: &built.root_path,
        library_label: &library_label,
        shelf_layout: shelf_layout_label(built.ruleset.naming.preset),
        export: &export,
        provenance: &provenance,
        duplicate_groups: &built.duplicate_groups,
        sample_data: false,
    };
    let numbers = headline_numbers(&input);
    let html = build_html_report(&input);

    // 9. Write every artifact into the Reports folder (F-1002).
    let written = crate::reports::write_plan_reports(
        app_data_dir,
        built.plan_id,
        &built.created_at,
        &export,
        &provenance,
    )?;
    let html_path = crate::reports::write_html_report(&written.dir, &html)?;

    Ok(GeneratedReport {
        plan_id: built.plan_id,
        written,
        html_path,
        html,
        numbers,
    })
}

/// Everything [`build_and_persist_plan`] produced along the way, so a caller
/// that only needs the persisted plan (v0.4.0 Phase 5's `plan_generate`
/// command) never re-reads the snapshot, and [`generate_and_report`] can keep
/// building its export/report projections from the same in-memory values.
pub struct PersistedPlan {
    pub plan_id: i64,
    pub scan_id: i64,
    pub ruleset_id: i64,
    pub created_at: String,
    pub root_path: String,
    pub ruleset: crate::ruleset::Ruleset,
    pub plan: crate::plan::builder::BuiltPlan,
    pub verdicts: Vec<crate::plan::validate::OpVerdict>,
    pub duplicate_groups: Vec<DuplicateGroup>,
}

/// The shared prefix of [`generate_and_report`]: read the stored scan and
/// ruleset, classify and extract, build the plan, detect duplicate
/// candidates, validate, and persist ONE validated plan (real verdicts, never
/// `pending`). This is "the v0.3.0 full chain minus report-file writing"
/// (v0.4.0 Phase 5 plan): the `plan_generate` IPC command calls this directly
/// so the review surface renders a REAL generated plan without also writing
/// the F-505/F-507/F-506 Reports-folder artifacts, which stay generate-on-
/// demand via [`generate_and_report`] / the "Save report" action.
///
/// Read-only against the real filesystem: it reads the SNAPSHOT (never the
/// live library) and writes only the plan header/ops rows.
pub async fn build_and_persist_plan(
    pool: &sqlx::SqlitePool,
    scan_id: i64,
    ruleset_id: i64,
) -> Result<PersistedPlan, GenerateError> {
    use crate::classify::classify;
    use crate::parse::extract::{extract, EntryInput};
    use crate::plan::builder::{
        build_plan_with_set_aside_root, classify_inputs_from_plan_nodes, default_set_aside_root,
        plan_nodes_from_snapshot,
    };
    use crate::plan::validate::{
        persist_validated_plan, validate_plan, RealFreeSpace, ValidationEnv,
    };
    use crate::ruleset::parse_and_validate;
    use crate::scan::walk::now_iso8601_utc;
    use std::collections::HashSet;

    // 1. Load the stored ruleset body and the scan root.
    let ruleset_row = crate::db::rulesets::get_ruleset(pool, ruleset_id)
        .await?
        .ok_or(GenerateError::NotFound("ruleset"))?;
    let ruleset = parse_and_validate(&ruleset_row.body_json)?;

    let root_path: Option<String> = sqlx::query_scalar("SELECT root_path FROM scans WHERE id = ?")
        .bind(scan_id)
        .fetch_optional(pool)
        .await?;
    let root_path = root_path.ok_or(GenerateError::NotFound("scan"))?;

    // 2. Read the immutable snapshot (never the live filesystem).
    let rows = crate::scan::get_scan_entries(pool, scan_id).await?;
    let nodes = plan_nodes_from_snapshot(&rows);

    // 3. Classify + extract (the pure pipeline the builder consumes).
    let classify_inputs = classify_inputs_from_plan_nodes(&nodes);
    let classifications = classify(&classify_inputs);
    let entry_inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    let merged = extract(&entry_inputs);

    // FD-34: resolve the set-aside root (F-803 `set_aside_root` when configured,
    // else the default sibling `<library-parent>\Audiobook Archive\`). The builder stamps
    // set-aside targets under this root, and validation checks targets against it.
    let set_aside_root = crate::db::settings::get_settings(pool)
        .await
        .ok()
        .and_then(|s| s.set_aside_root)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_set_aside_root(&root_path));

    // 4. Build the plan and detect duplicate candidates.
    let plan = build_plan_with_set_aside_root(
        &nodes,
        &classifications,
        &merged,
        &ruleset,
        &root_path,
        &set_aside_root,
    );
    let dupe_entries = dupe_entries_from_plan_nodes(&nodes, &merged);
    let books = book_folders_from_plan_nodes(&nodes, &merged);
    let duplicate_groups = detect_duplicates(&dupe_entries, &books);

    // 5. Validate against a fresh view of the snapshot's own paths (all present),
    // with the FD-34 target-scope check enabled (a persisted plan's targets must
    // stay inside the library or under the set-aside root before it can apply).
    let existing: HashSet<String> = nodes.iter().map(|n| n.path.clone()).collect();
    let free_space = RealFreeSpace;
    let long_paths = crate::scan::longpath::long_paths_enabled();
    let env = ValidationEnv::new(&existing, long_paths, &free_space)
        .with_scope(&root_path, &set_aside_root);
    let verdicts = validate_plan(&plan, &env);

    // 6. Persist the validated plan (one insert; real verdicts).
    let created_at = now_iso8601_utc();
    let plan_id = persist_validated_plan(
        pool,
        scan_id,
        ruleset_id,
        "draft",
        &plan,
        &verdicts,
        &created_at,
    )
    .await?;

    Ok(PersistedPlan {
        plan_id,
        scan_id,
        ruleset_id,
        created_at,
        root_path,
        ruleset,
        plan,
        verdicts,
        duplicate_groups,
    })
}

/// The F-906 ruleset editor's live re-plan preview (AC-33): the SAME
/// build -> detect-duplicates -> validate pipeline [`build_and_persist_plan`]
/// runs (steps 1-5 there), but for a DRAFT [`crate::ruleset::Ruleset`] that
/// has not been (and may never be) saved, and with step 6
/// (`persist_validated_plan`) skipped entirely: this NEVER writes a
/// `plans`/`plan_ops` row (plan_ops immutability applies to real, persisted
/// plans; a preview is not one, ever - not even a draft one).
///
/// Returns the same seven-card shape [`crate::plan::query::build_plan_review`]
/// already produces for a real plan (via
/// [`crate::plan::query::synthetic_op_rows`]), so the editor's live counts
/// use the exact same group-status/count logic the review screen does, with
/// zero duplicated aggregation code. `ruleset` is already validated by the
/// caller (or arrives pre-validated from a typed IPC struct); this function
/// does not re-validate it, since a live preview runs on every toggle change
/// and should not re-derive a validation error the caller already has a
/// clearer path to report.
pub async fn preview_plan_review(
    pool: &sqlx::SqlitePool,
    scan_id: i64,
    ruleset: &crate::ruleset::Ruleset,
) -> Result<crate::ipc::PlanPreview, GenerateError> {
    use crate::classify::classify;
    use crate::parse::extract::{extract, EntryInput};
    use crate::plan::builder::{
        build_plan, classify_inputs_from_plan_nodes, plan_nodes_from_snapshot,
    };
    use crate::plan::query::{build_plan_review, synthetic_op_rows};
    use crate::plan::validate::{validate_plan, RealFreeSpace, ValidationEnv};
    use std::collections::HashSet;

    // 1. Load the scan root (no ruleset row to load: the draft ruleset comes
    // straight from the caller).
    let root_path: Option<String> = sqlx::query_scalar("SELECT root_path FROM scans WHERE id = ?")
        .bind(scan_id)
        .fetch_optional(pool)
        .await?;
    let root_path = root_path.ok_or(GenerateError::NotFound("scan"))?;

    // 2. Read the immutable snapshot (never the live filesystem).
    let rows = crate::scan::get_scan_entries(pool, scan_id).await?;
    let nodes = plan_nodes_from_snapshot(&rows);

    // 3. Classify + extract (the pure pipeline the builder consumes).
    let classify_inputs = classify_inputs_from_plan_nodes(&nodes);
    let classifications = classify(&classify_inputs);
    let entry_inputs: Vec<EntryInput> = nodes
        .iter()
        .map(|n| EntryInput {
            id: n.id,
            parent: n.parent,
            name: n.name.clone(),
            kind: n.kind,
        })
        .collect();
    let merged = extract(&entry_inputs);

    // 4. Build the plan (in memory only) and detect duplicate candidates.
    let plan = build_plan(&nodes, &classifications, &merged, ruleset, &root_path);
    let dupe_entries = dupe_entries_from_plan_nodes(&nodes, &merged);
    let books = book_folders_from_plan_nodes(&nodes, &merged);
    let duplicate_groups = detect_duplicates(&dupe_entries, &books);

    // 5. Validate against a fresh view of the snapshot's own paths (all
    // present) - the same env `build_and_persist_plan` uses, so a preview's
    // blocked/warning counts match what a real regenerate would show.
    let existing: HashSet<String> = nodes.iter().map(|n| n.path.clone()).collect();
    let free_space = RealFreeSpace;
    let long_paths = crate::scan::longpath::long_paths_enabled();
    let env = ValidationEnv::new(&existing, long_paths, &free_space);
    let verdicts = validate_plan(&plan, &env);

    // 6. Project to the seven-card shape - NO persistence step here, ever.
    let synthetic_rows = synthetic_op_rows(&plan, &verdicts);
    let review = build_plan_review(0, scan_id, &synthetic_rows, &duplicate_groups);
    Ok(crate::ipc::PlanPreview {
        scan_id,
        groups: review.groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::classify;
    use crate::parse::extract::{extract, EntryInput, MergedEntry, NodeKind};
    use crate::plan::builder::{build_plan, classify_inputs_from_plan_nodes, BuiltPlan, PlanNode};
    use crate::plan::validate::OpVerdict;
    use crate::ruleset::default_ruleset;
    use crate::scan::typing::FileClass;

    const ROOT: &str = "E:/Library";

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

    /// A rich fixture: a loose root book, a noisy-named book folder (strip-noise),
    /// a Hugo pack with two members (one award-marked) for provenance, and two
    /// identical loose files that form one exact-duplicate group. Exercises every
    /// section the report renders.
    fn rich_nodes() -> Vec<PlanNode> {
        vec![
            dir(0, None, "E:/Library"),
            // loose root book
            aud(1, Some(0), "E:/Library/Sapiens by Yuval Noah Harari.m4b", 180_000_000),
            // a noisy-named book folder
            dir(2, Some(0), "E:/Library/Cold Days [320k 18;48;03 2.52GB]"),
            aud(
                3,
                Some(2),
                "E:/Library/Cold Days [320k 18;48;03 2.52GB]/Cold Days.m4b",
                200_000_000,
            ),
            // a Hugo pack with two members (one award ^)
            dir(4, Some(0), "E:/Library/Hugo Collection"),
            dir(
                5,
                Some(4),
                "E:/Library/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season",
            ),
            aud(
                6,
                Some(5),
                "E:/Library/Hugo Collection/2016^ - N.K. Jemisin - The Fifth Season/The Fifth Season.m4b",
                187_000_000,
            ),
            dir(
                7,
                Some(4),
                "E:/Library/Hugo Collection/Charles Stross - Neptune's Brood",
            ),
            aud(
                8,
                Some(7),
                "E:/Library/Hugo Collection/Charles Stross - Neptune's Brood/Neptune's Brood.m4b",
                165_000_000,
            ),
            // two identical copies -> one exact-duplicate group
            aud(9, Some(0), "E:/Library/Dupe/Starter Villain.m4b", 90_000_000),
            aud(10, Some(0), "E:/Library/Dupe2/Starter Villain.m4b", 90_000_000),
            dir(11, Some(0), "E:/Library/Dupe"),
            dir(12, Some(0), "E:/Library/Dupe2"),
        ]
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

    fn built(nodes: &[PlanNode]) -> (BuiltPlan, Vec<MergedEntry>) {
        let (cs, merged) = analyze(nodes);
        let plan = build_plan(nodes, &cs, &merged, &default_ruleset(), ROOT);
        (plan, merged)
    }

    fn all_valid(plan: &BuiltPlan) -> Vec<OpVerdict> {
        plan.ops.iter().map(|_| OpVerdict::valid()).collect()
    }

    /// Assemble a full ReportInput over the rich fixture, owning the pieces.
    struct Owned {
        export: PlanExport,
        provenance: ProvenanceReport,
        dupes: Vec<DuplicateGroup>,
    }
    fn owned() -> Owned {
        let nodes = rich_nodes();
        let (plan, merged) = built(&nodes);
        let verdicts = all_valid(&plan);
        let export = build_plan_export(7, "2026-07-04T18:14:03Z", "draft", &plan, &verdicts);
        let provenance = build_provenance_report(&plan);
        let dupe_entries = dupe_entries_from_plan_nodes(&nodes, &merged);
        let books = book_folders_from_plan_nodes(&nodes, &merged);
        let dupes = detect_duplicates(&dupe_entries, &books);
        Owned {
            export,
            provenance,
            dupes,
        }
    }
    fn input_of(o: &Owned, sample: bool) -> ReportInput<'_> {
        ReportInput {
            plan_id: 7,
            scan_id: 4,
            created_at: "2026-07-04T18:14:03Z",
            library_root: ROOT,
            library_label: "Library",
            shelf_layout: "author first",
            export: &o.export,
            provenance: &o.provenance,
            duplicate_groups: &o.dupes,
            sample_data: sample,
        }
    }

    #[test]
    fn human_dateline_formats_a_known_instant() {
        assert_eq!(
            human_dateline("2026-07-04T18:14:03Z"),
            "Saturday, July 4, 2026 at 6:14 pm UTC"
        );
        assert_eq!(
            human_dateline("2026-07-03T06:05:00Z"),
            "Friday, July 3, 2026 at 6:05 am UTC"
        );
        // Midnight renders as 12 am, not 0.
        assert!(human_dateline("2026-01-01T00:00:00Z").contains("12:00 am"));
    }

    /// AC-20: the report is self-contained - no external host, no `<link>`,
    /// `<script>`, `@import`, and no `://` anywhere (a colon-slash-slash cannot
    /// occur inside the base64 font blob).
    #[test]
    fn report_is_self_contained_zero_network() {
        let o = owned();
        let html = build_html_report(&input_of(&o, true));
        assert!(!html.contains("://"), "no scheme://host anywhere");
        assert!(!html.contains("<link"), "no external stylesheet link");
        assert!(!html.contains("<script"), "no script tag");
        assert!(!html.contains("@import"), "no CSS @import");
        assert!(!html.contains("src=\"//"), "no protocol-relative src");
        assert!(!html.contains("href=\"//"), "no protocol-relative href");
        // The one embedded asset is a data: URI, and it is present.
        assert!(html.contains("data:font/woff2;base64,"), "font is embedded");
    }

    /// AC-23: the FD-10 canon deletion-guarantee line appears verbatim.
    #[test]
    fn fd10_guarantee_line_is_verbatim() {
        let o = owned();
        let html = build_html_report(&input_of(&o, true));
        assert!(
            html.contains(FD10_GUARANTEE_LINE),
            "FD-10 canon must appear verbatim"
        );
        assert!(html.contains(
            "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone."
        ));
    }

    /// AC-21 / spec 4.3: all seven FD-26 group headlines appear in the summary
    /// table, and the copies group carries the "checking" hold chip.
    #[test]
    fn seven_group_summary_present_with_copies_held() {
        let o = owned();
        let html = build_html_report(&input_of(&o, false));
        for &g in CampaignGroup::ALL {
            // Each group's description is a stable per-group string.
            assert!(
                html.contains(group_description(g)),
                "missing summary row for group {:?}",
                g
            );
        }
        assert!(
            html.contains("chip hold\">checking"),
            "copies chip is checking"
        );
        // The copies count is the exact-duplicate GROUP count (FD-08): one group.
        assert!(html.contains("Move 1 duplicate copies to the Archive"));
    }

    /// AC-21: the complete change-list table has exactly one row per plan op.
    #[test]
    fn complete_table_has_one_row_per_op() {
        let o = owned();
        let html = build_html_report(&input_of(&o, false));
        let op_rows = html.matches("class=\"op-row\"").count();
        assert_eq!(
            op_rows,
            o.export.ops.len(),
            "one complete-list row per plan op"
        );
        assert!(op_rows > 0, "the fixture produced ops");
    }

    /// AC-25/AC-26: every flattened pack member is listed in the provenance
    /// section, with the award marker kept in the report (not the folder name),
    /// and no copy promises an ABS-side change (FD-12).
    #[test]
    fn provenance_lists_members_and_makes_no_abs_promise() {
        let o = owned();
        let html = build_html_report(&input_of(&o, false));
        assert!(html.contains("Where these books came from"));
        assert!(html.contains("Hugo Collection"));
        assert!(html.contains("The Fifth Season"));
        assert!(html.contains("Neptune"));
        // Award kept in the report, not the folder name (FD-12 tie).
        assert!(html.contains("kept in this report, not in the folder name"));
        // AC-26 concrete grep: the report never promises an ABS-side write.
        let lower = html.to_lowercase();
        assert!(!lower.contains("written to audiobookshelf"));
        assert!(!lower.contains("collection written"));
        assert!(!lower.contains("tag written"));
        assert!(!lower.contains("push to audiobookshelf"));
        assert!(!lower.contains("write back to"));
    }

    /// The banned internal vocabulary never reaches the report (FD-31 + register).
    ///
    /// This is the Rust half of a two-part gate; `src/lib/__tests__/vocabulary.ts`
    /// is the other, covering the app's own copy. Both halves must know the same
    /// words, or a term retired on one surface survives on the other. That is not
    /// hypothetical: this test knew only the never-allowed list and had no opinion
    /// about RETIRED words, so the exported report carried "Set Aside" and six
    /// "shelves" strings after FD-42 and FD-47 had already retired them.
    #[test]
    fn no_banned_vocabulary() {
        let o = owned();
        let html = build_html_report(&input_of(&o, false)).to_lowercase();

        // "Audiobookshelf" is the product this app complements and appears in the
        // report by design. A substring scan for "shelf" would flag it forever, so
        // neutralize it first rather than weakening the pattern. Doing this
        // explicitly beats a cleverer match: the exception stays visible.
        let html = html.replace("audiobookshelf", "<the-product>");

        // Engineering terms that were NEVER allowed on a user-facing surface.
        for word in [
            "quarantine",
            "dedupe",
            "operations",
            "manifest",
            "dashboard",
        ] {
            assert!(
                !html.contains(word),
                "banned word {word:?} leaked into the report"
            );
        }

        // Words RETIRED by a ledger decision: each was the approved term until a
        // decision replaced it. Kept separate from the list above because the fix
        // differs - banned jargon is rewritten, a retired word is swapped for its
        // successor. NOT listed: "copies", which FD-46 kept for the members of a
        // group even though the group itself became "Duplicates".
        for (word, decision, successor) in [
            // "aside" bare: the adjacent-only form missed "set it aside".
            ("aside", "FD-42", "Archive"),
            ("shelf", "FD-47", "library"),
            ("shelves", "FD-47", "library"),
        ] {
            assert!(
                !html.contains(word),
                "{word:?} was retired by {decision} in favour of {successor:?}, \
                 but it reached the exported report"
            );
        }
    }

    /// The print stylesheet is present (spec 5).
    #[test]
    fn print_css_present() {
        let o = owned();
        let html = build_html_report(&input_of(&o, true));
        assert!(html.contains("@media print"), "print rules must be present");
        assert!(html.contains("break-inside:avoid"));
    }

    /// FD-27: the sample flag renders a visible "sample data" label; a real plan
    /// carries none.
    #[test]
    fn sample_data_label_is_conditional() {
        let o = owned();
        assert!(build_html_report(&input_of(&o, true)).contains("(sample data)"));
        assert!(!build_html_report(&input_of(&o, false)).contains("(sample data)"));
    }

    /// The before/after tables strike bracket noise through in the "Now" column.
    #[test]
    fn messy_names_show_struck_noise() {
        let o = owned();
        let html = build_html_report(&input_of(&o, false));
        assert!(
            html.contains("<s>[320k 18;48;03 2.52GB]</s>"),
            "the ripper tag is struck through: {html}"
        );
    }

    /// The document is a complete, standalone HTML file.
    #[test]
    fn output_is_a_complete_document() {
        let o = owned();
        let html = build_html_report(&input_of(&o, true));
        assert!(html.trim_start().starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"));
    }

    /// Emit the committed FIXTURE sample report (labeled sample data, FD-27).
    /// Ignored so it never runs in the normal suite; run explicitly to refresh
    /// the committed artifact:
    /// `cargo test -p abo-core --lib report::tests::emit_sample_report_fixture -- --ignored`.
    #[test]
    #[ignore = "writes the committed docs sample; run explicitly to refresh it"]
    fn emit_sample_report_fixture() {
        let o = owned();
        let html = build_html_report(&input_of(&o, true));
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("internal")
            .join("releases")
            .join("v0.3.0-planning")
            .join("sample-report-fixture.html");
        std::fs::write(&out, &html).expect("write sample fixture");
        println!("wrote sample report fixture to {}", out.display());
    }

    /// Headline numbers match what the document renders.
    #[test]
    fn headline_numbers_are_coherent() {
        let o = owned();
        let n = headline_numbers(&input_of(&o, false));
        assert_eq!(n.duplicate_groups, 1, "one exact-duplicate group");
        assert_eq!(n.per_group.len(), 7);
        // Total equals the sum of the seven displayed group counts.
        let sum: u64 = n.per_group.iter().map(|(_, c)| *c).sum();
        assert_eq!(sum, n.total_changes);
    }
}
