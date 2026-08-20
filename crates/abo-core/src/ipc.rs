//! IPC payload structs shared between `abo-core` and the `src-tauri` command
//! layer, each deriving `serde::Serialize`, `serde::Deserialize`, and
//! `specta::Type`.
//!
//! v0.1.0 spine. Phase 3 introduces the scanner's two payload shapes here (the
//! natural home the Phase 1 stub reserved): [`ScanSummary`] (returned by
//! [`crate::scan::run_scan`]) and [`EntryRow`] (returned by
//! [`crate::scan::get_scan_entries`] for the Phase 6 tracer UI). Both derive
//! `serde` both ways and `specta::Type` so `Result<T, AppError>` is a valid
//! tauri-specta command return type once the seam is wired (Phase 5, AC-4).
//! [`EntryRow`] additionally derives `sqlx::FromRow` so the persistence layer
//! can read rows straight back out; this keeps the wire shape and the row shape
//! one and the same. Phase 4 may extend these additively.
//!
//! Phase 5 (tauri-specta seam) adds the shell's IPC payloads here so the
//! command/event contract stays in the Tauri-free core: [`JobStarted`] (returned
//! by `scan_start`), [`DbStatus`] (returned by `db_status`, the wire form of the
//! startup [`crate::db::DbOpenOutcome`]), and the three job-event payloads
//! [`JobCompletedPayload`], [`JobFailedPayload`], and [`JobProgressPayload`].
//! The `#[tauri_specta::Event]` wrappers and `#[tauri::command]` annotations live
//! in `src-tauri`; only the plain payload shapes (serde + `specta::Type`) live
//! here, so the core never gains a tauri dependency (AC-3).
//!
//! Placement invariant (AC-4): every type that crosses the IPC boundary is
//! reachable through this module and derives `serde::Serialize`,
//! `serde::Deserialize`, and `specta::Type`. The payload structs are defined
//! here; [`AppError`] is defined in [`crate::error`] and re-exported here so a
//! Phase 5 consumer can name the whole IPC surface under `abo_core::ipc`. The
//! [`contract`] test instantiates a bound-checking generic over each of these
//! types, so a dropped derive fails THIS crate's `cargo test` with a named
//! error instead of surfacing as an opaque trait-bound failure inside Phase 5's
//! generated bindings.
//!
//! Deliberately NOT re-exported: [`crate::db::DbOpenOutcome`]. It is a
//! startup-internal Rust type (it carries a `PathBuf`) that the shell reads once
//! and maps to [`AppError::DbCorruptRecovered`] BEFORE anything crosses the
//! boundary; the error, not the outcome, is the wire type. It therefore does not
//! need serde/specta and is not part of the IPC contract.

use serde::{Deserialize, Serialize};

/// Re-export so the entire IPC surface (payloads plus the error type) is
/// reachable under `abo_core::ipc` (AC-4). Defined in [`crate::error`].
pub use crate::error::AppError;

/// Re-exported so [`LibraryOverview`]'s embedded health report is reachable
/// under `abo_core::ipc` (AC-4), same pattern as [`AppError`] above. Defined in
/// [`crate::classify`], where the pure `health_metrics` computation lives.
pub use crate::classify::{ClassMetric, FolderClass, HealthMetrics, MetricUnit, ProblemMetric};

/// Re-exported so the `apply_start` command's `mode` argument is reachable under
/// `abo_core::ipc` (AC-4). Defined in [`crate::exec`], where the executor owns the
/// dry-run vs Real distinction (F-607).
pub use crate::exec::ApplyMode;

/// The category of a [`ScanWarning`].
///
/// The two per-entry kinds ([`JunctionSkipped`](ScanWarningKind::JunctionSkipped)
/// and [`PermissionDenied`](ScanWarningKind::PermissionDenied)) share their
/// kebab-case wire strings with the corresponding [`AppError`] codes on purpose:
/// a warning is the collected-during-scan record of the same condition the error
/// taxonomy names, so a caller can key off one vocabulary. The scanner records
/// these as warnings (the scan runs to completion, AC-11) rather than raising
/// them as errors; the `AppError` variants remain for the per-entry error case.
/// The two path-length kinds are scan-level (FD-19): [`LongPathsDisabled`] when a
/// recorded path exceeds the legacy 260-char limit while Windows long-path
/// support is off, and [`NearMaxPathInterop`] for paths near the limit that
/// other Windows tools may still mishandle.
///
/// [`LongPathsDisabled`]: ScanWarningKind::LongPathsDisabled
/// [`NearMaxPathInterop`]: ScanWarningKind::NearMaxPathInterop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ScanWarningKind {
    /// A junction or reparse point was recorded but deliberately not followed
    /// (shares the `junction-skipped` wire string with [`AppError::JunctionSkipped`]).
    JunctionSkipped,
    /// An entry (or a subtree) could not be read because the OS denied access;
    /// it was recorded where possible and the scan continued (shares the
    /// `permission-denied` wire string with [`AppError::PermissionDenied`]).
    PermissionDenied,
    /// One or more recorded paths exceed the legacy 260-char Windows limit and
    /// long-path support is disabled; carries a how-to link (FD-19, AC-101.4).
    LongPathsDisabled,
    /// A recorded path is at or near the legacy 260-char limit; some Windows
    /// tools may not handle it even though this scan did (FD-19 interop note).
    NearMaxPathInterop,
}

/// A non-fatal condition recorded during a scan, for the caller (and, from
/// v0.4.0, the GUI) to surface. Structured now so the shape is stable before any
/// UI renders it (spec F-101: "add a warning record to the ScanSummary;
/// structure it; the GUI renders it in v0.4.0").
///
/// `path` is the stored, human-readable path the warning concerns (the first
/// offending path, for the scan-level [`ScanWarningKind::LongPathsDisabled`]).
/// `detail` is a family-safe sentence; `how_to` is an optional link to guidance
/// (populated for [`ScanWarningKind::LongPathsDisabled`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ScanWarning {
    /// The category of condition (see [`ScanWarningKind`]).
    pub kind: ScanWarningKind,
    /// The stored, human-readable path the warning concerns.
    pub path: String,
    /// A family-safe, human-readable explanation.
    pub detail: String,
    /// An optional link to guidance (populated for `long-paths-disabled`).
    pub how_to: Option<String>,
}

impl ScanWarning {
    /// Build a `junction-skipped` warning for a recorded junction/reparse point.
    pub fn junction_skipped(path: &std::path::Path) -> Self {
        Self {
            kind: ScanWarningKind::JunctionSkipped,
            path: path.to_string_lossy().into_owned(),
            detail: "This item is a junction or reparse point (a link to another \
                     location). It was recorded but not opened, so the scan cannot \
                     loop back on itself."
                .to_string(),
            how_to: None,
        }
    }

    /// Build a `permission-denied` warning for a subtree the OS refused to read.
    pub fn permission_denied(path: &std::path::Path) -> Self {
        Self {
            kind: ScanWarningKind::PermissionDenied,
            path: path.to_string_lossy().into_owned(),
            detail: "This item could not be read because Windows denied access. It \
                     was skipped and the rest of the scan continued."
                .to_string(),
            how_to: None,
        }
    }
}

/// The result of a completed [`crate::scan::run_scan`]: the metadata of the one
/// immutable `scans` row it wrote (F-105), plus the count of entries skipped
/// during the walk (permission-denied subtrees, unstatable entries; AC-11) and
/// the structured [`ScanWarning`] records collected during the scan (FD-19).
///
/// `total_bytes` is the sum of file sizes (directories contribute 0). Timestamps
/// are ISO-8601 UTC, whole-second precision (see [`crate::scan`]). `root_path`
/// is the stored, human-readable form (no `\\?\` verbatim prefix).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ScanSummary {
    /// `scans.id` of the snapshot this scan wrote.
    pub scan_id: i64,
    /// The normalized scan root, stored form (no `\\?\` prefix).
    pub root_path: String,
    /// Number of `entries` rows written (files + directories, including root).
    pub entry_count: i64,
    /// Sum of file sizes in bytes (directories contribute 0).
    pub total_bytes: i64,
    /// Entries/subtrees skipped during the walk (permission-denied, unstatable).
    pub skipped_count: i64,
    /// When the scan started (ISO-8601 UTC).
    pub started_at: String,
    /// When the scan completed (ISO-8601 UTC).
    pub completed_at: String,
    /// Terminal status of the snapshot; `completed` for a successful scan.
    pub status: String,
    /// Structured non-fatal conditions recorded during the scan (junctions
    /// skipped, permission-denied subtrees, long-path warnings). Empty for a
    /// clean scan. The GUI renders these from v0.4.0; this release logs them.
    pub warnings: Vec<ScanWarning>,
}

/// One persisted `entries` row (F-101 / F-105), read back for the tracer.
///
/// The shape mirrors the `entries` table one-to-one. `parent_id` is the logical
/// tree edge (the `entries.id` of the containing directory), `None` for the
/// scan root. `kind` is `"file"` or `"dir"`; `file_class` is the F-103 class
/// string for files and `None` for directories. `path` is the stored,
/// human-readable full path (no `\\?\` prefix). `mtime` is ISO-8601 UTC, or
/// `None` when the entry could not be stat'd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, sqlx::FromRow)]
pub struct EntryRow {
    /// `entries.id` (assigned at insertion).
    pub id: i64,
    /// The owning snapshot (`scans.id`).
    pub scan_id: i64,
    /// Logical parent `entries.id`; `None` for the scan root.
    pub parent_id: Option<i64>,
    /// Full path, stored form (no `\\?\` prefix).
    pub path: String,
    /// Final path component (file or directory name).
    pub name: String,
    /// `"file"` or `"dir"`.
    pub kind: String,
    /// F-103 file class for files (`audio`, `video`, ...); `None` for dirs.
    pub file_class: Option<String>,
    /// Size in bytes; 0 for directories and unstatable entries.
    pub size: i64,
    /// Modification time (ISO-8601 UTC); `None` when unavailable.
    pub mtime: Option<String>,
    /// Depth below the scan root (root is 0).
    pub depth: i64,
}

/// The singleton application settings (F-803), the wire form of the one
/// `settings` row (migrations 0001/0002/0003). Returned by `settings_get` and
/// accepted by `settings_set`.
///
/// Field-to-column mapping and the one deliberate name divergence:
///
/// - `library_root` -> `settings.library_root`. The sanctioned scan root the
///   backend re-allows at startup (FD-29, F-909). `None` means no library is
///   configured yet, which is exactly how the shell detects first-run.
/// - `set_aside_root` -> `settings.quarantine_root`. The field is named for the
///   plain-language "set aside" vocabulary the UI uses (FD-31), while the column
///   and every internal type keep "quarantine". The IPC boundary is the seam
///   where the internal term becomes the family-facing one, so the crossing
///   payload speaks the user's vocabulary and the storage keeps the engine's.
/// - `reports_root` -> `settings.reports_root`. Optional Reports-folder override
///   (F-1002); `None` uses the default beside the app data.
/// - `theme` -> `settings.theme`. `"day"` or `"evening"` (FD-09). Always a valid
///   theme string: `get_settings` normalizes any unexpected stored value back to
///   `"day"` so the UI never receives a theme it cannot render.
/// - `scan_retention_count` -> `settings.scan_retention_count` (FD-20, AC-35).
///
/// A root stored as an empty string is normalized to `None` on read, and a
/// `Some("")` is written as SQL NULL, so "unset" has one representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AppSettings {
    /// The sanctioned library scan root (FD-29, F-909); `None` until first-run.
    pub library_root: Option<String>,
    /// Where set-aside items land (the `quarantine_root` column, FD-31); `None`
    /// uses the default beside the app data.
    pub set_aside_root: Option<String>,
    /// Optional override for the Reports folder (F-1002); `None` uses the default.
    pub reports_root: Option<String>,
    /// UI theme, `"day"` or `"evening"` (FD-09). Never any other value on read.
    pub theme: String,
    /// Snapshot retention: keep the last N scans (FD-20, AC-35). Default 10.
    pub scan_retention_count: i64,
}

/// One book's cover art (F-907, v0.4.0 Phase 3), the wire form returned by the
/// `cover_get` command. `None` from that command means "no cover" and the
/// frontend renders the deterministic fallback tile instead (AC-23).
///
/// The bytes cross IPC as base64 (`data:` payload), not as a filesystem path and
/// not as a raw byte array: base64 keeps the payload compact and, decisively,
/// means the WebView is handed already-read image data over typed IPC rather than
/// any filesystem or asset-protocol scope. The library root never becomes
/// WebView-readable (FD-29). The frontend composes `data:${mime};base64,${base64}`
/// as the `<img>` source.
///
/// `mime` is sniffed from the bytes (e.g. `image/jpeg`, `image/png`), so a
/// mislabeled sidecar or corrupt frame is rejected before it reaches here (the
/// book then shows the fallback tile). The bytes are the raw, UNDECODED image as
/// stored in the file (plan T-11 defers decoding/downscaling).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CoverImage {
    /// The image mime type, sniffed from the bytes (e.g. `image/jpeg`).
    pub mime: String,
    /// Standard base64 (RFC 4648, `=`-padded) of the raw image bytes.
    pub base64: String,
}

// ---- Phase 4 shell payloads (F-902 library home) ----

/// Which register a [`BookReason`] chip reads in (design-system Section 4.6):
/// a soft, tidy-adjacent note (`warn`, e.g. "loose file", "messy name") versus
/// a structural flag that needs a decision (`alert`, e.g. "7 books, 1 folder",
/// "2 copies"). Icon-plus-label always accompanies the kind; color is never
/// the only signal (Section 8 accessibility).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ReasonKind {
    Warn,
    Alert,
}

/// The plain-language "why this book is worth a look" chip under one
/// [`BookExample`] (design-system Section 4.6 `.bookslot` / `.why`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct BookReason {
    pub kind: ReasonKind,
    /// Plain-language chip text, e.g. "loose file", "messy name",
    /// "7 books, 1 folder", "2 copies". Never a raw class/rule id (Section 6).
    pub text: String,
}

/// One example book on the "Worth a look first" shelf (F-902, v0.4.0 Phase 4,
/// design-system Section 4.6). `entry_id` names a book WITHIN the snapshot that
/// produced the enclosing [`LibraryOverview`] (its `scan_id`), so the frontend
/// can pass `(scan_id, entry_id)` straight to `cover_get` for the book's cover
/// - the same snapshot-scoped identity [`EntryRow`] already uses.
///
/// `title` is never absent (a book the shelf shows always parsed a title, own
/// or derived); `author` is [`None`] when no author could be read, which the
/// frontend's `Cover`/`FallbackTile` already render gracefully (AC-23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct BookExample {
    pub entry_id: i64,
    pub title: String,
    pub author: Option<String>,
    pub reason: BookReason,
}

/// One series cluster on the "Series on your shelves" shelf (design-system
/// Section 4.8). `name` is the series name (falling back to the container
/// folder's own name when no series field parsed, so a real series-container
/// never disappears from the shelf for want of a label); `book_count` is the
/// number of book-like children the classifier folded into this container
/// (`Evidence::book_children`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SeriesCluster {
    pub name: String,
    pub author: Option<String>,
    pub book_count: u64,
}

/// The home's "good news" line (design-system Section 4, `.goodline`): what is
/// ALREADY tidy, stated in the same sentence-with-a-unit register as every
/// other home number (FD-08, AC-7). Every field is a plain count/byte total
/// read from the current snapshot's [`HealthMetrics`], never a hardcoded
/// sample (FD-27).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GoodNews {
    pub already_tidy_books: u64,
    pub series_shelved: u64,
    pub empty_folders: u64,
    pub duplicate_groups: u64,
    pub duplicate_bytes: u64,
}

/// The full library home payload (F-902, v0.4.0 Phase 4), returned by the
/// `classify_overview` command. [`None`] from that command means no scan has
/// ever completed - the honest pre-first-scan state (AC-6) - in which case the
/// frontend shows the empty-library state and the scan affordance rather than
/// any of this shape's zeroed fields (a real zero and "never scanned" must
/// never be confused).
///
/// Every count and byte figure here is computed from the snapshot named by
/// `scan_id` at render time (AC-7): nothing is a literal/sample value. See
/// [`crate::classify::build_overview`] for the derivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct LibraryOverview {
    /// The snapshot this overview was computed from (`scans.id`); the frontend
    /// passes this alongside a [`BookExample::entry_id`] to `cover_get`.
    pub scan_id: i64,
    /// Every book the library holds: one per `book`-class folder (a disc-split
    /// book still counts once) plus one per loose-root audio file plus the
    /// distinct titles F-203 found inside each `multi-book-suspect` folder.
    pub total_books: u64,
    /// Sum of file sizes across the whole snapshot (equals
    /// [`HealthMetrics::total_bytes`]).
    pub total_bytes: u64,
    /// How many of `total_books` carry at least one "worth a look" problem
    /// (loose file, messy folder name, a multi-book/box-set folder, or a
    /// duplicate copy). `total_books - needs_tidy_books` is already tidy.
    pub needs_tidy_books: u64,
    /// A bounded set of example books for the "Worth a look first" shelf
    /// (design-system Section 4.6-4.7): curated examples, not the exhaustive
    /// list - the full list is the Phase 5 review surface and the exported
    /// report (D-16).
    pub worth_a_look: Vec<BookExample>,
    /// Real series containers on the shelves (design-system Section 4.8).
    pub series: Vec<SeriesCluster>,
    pub good_news: GoodNews,
    /// The full per-class/per-problem health report this overview was built
    /// from, so the sidebar's Duplicates badge (FD-08 GROUP count) and any
    /// other per-group count reads the same numbers the home prose does.
    pub metrics: HealthMetrics,
}

// ---- Phase 5 shell payloads (F-903 plan review surface, v0.4.0 Phase 5) ----

/// F-405 approval state (`plan_ops.approval`), the wire form of
/// [`crate::plan::validate::ApprovalState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum PlanApproval {
    Pending,
    Approved,
    Rejected,
    Excluded,
}

/// F-404 per-operation validation verdict, the wire form of
/// [`crate::plan::validate::ValidationState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum PlanValidation {
    Valid,
    Warning,
    Blocked,
}

/// The review state of one campaign-group card (F-502, design-system 4.9
/// `.stag` tag variants). Distinct from [`PlanApproval`] (a per-OPERATION
/// state): this is the coarser, group-level summary the switch and status
/// chip render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum GroupStatus {
    /// The switch is on: the group's non-blocked, non-excluded ops are
    /// approved (design-system `.stag.in` "included").
    Included,
    /// The switch is off (a deliberate group-level skip); the group's ops
    /// are left out this round (design-system `.stag.out` "left out").
    LeftOut,
    /// The switch cannot be toggled yet: every member is blocked or
    /// excluded (F-908 blocked/empty-group state, AC-15), the group has no
    /// members this scan, or (the Copies group, OQ-3) the candidates are
    /// still waiting on the byte-by-byte check (design-system `.stag.hold`
    /// "checking copies").
    Checking,
}

/// One campaign-group card (F-502, FD-26). Exactly seven of these render, in
/// [`crate::plan::builder::CampaignGroup::ALL`] order, whatever the plan
/// holds (a group with no ops still renders its card, `op_count` 0, AC-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlanGroupView {
    /// The stable kebab-case group id (`crate::plan::builder::CampaignGroup::slug`),
    /// e.g. `"loose-books"`; what `plan_set_group_approval` takes back.
    pub group: String,
    /// The FD-26 canonical label, e.g. `"loose books"`.
    pub label: String,
    /// The card's plain-language headline with the real count folded in,
    /// e.g. "Give 238 loose books their own folders".
    pub headline: String,
    /// The plain-language "why" paragraph under the headline.
    pub reason: String,
    /// The user-facing CHANGE count (FD-08 unit: moves/renames/set-asides/
    /// removals; for the Copies group, duplicate GROUPS).
    pub op_count: i64,
    /// How many of the group's changes would ACTUALLY run: `op_count` minus the
    /// blocked and individually excluded ops. This is what the footer's
    /// "M changes" total sums over an included group, so a group holding
    /// blocked/excluded ops never inflates the count of what a tidy-up would do.
    pub actionable_count: i64,
    /// Total bytes the group's changes move/set aside.
    pub byte_size: i64,
    pub status: GroupStatus,
    /// How many of the group's ops carry a `warning` verdict.
    pub warning_count: i64,
    /// How many of the group's ops carry a `blocked` verdict.
    pub blocked_count: i64,
}

/// The full review-surface payload (`plan_generate`/`plan_get`): the seven
/// group cards plus plan identity. Deliberately does NOT carry the op
/// listing (that is [`PlanOpsPage`] via `plan_list_ops`) - the two are
/// fetched separately so toggling a group's switch (which only changes
/// aggregate counts) never re-sends the whole op list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlanReview {
    pub plan_id: i64,
    pub scan_id: i64,
    /// Always exactly seven, in canonical order (AC-10).
    pub groups: Vec<PlanGroupView>,
}

/// The matched F-301 pattern shown in an operation's "Show file details"
/// disclosure (F-504, AC-18): the stable id plus the human-readable shape,
/// always shown together (FD-13's "never a bare id" rule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct MatchedPatternView {
    /// The stable `pattern-N` id (`crate::parse::matchers::MatcherId::as_str`).
    pub id: String,
    /// The short human-readable shape (`MatcherId::handle`), e.g.
    /// "Title by Author (loose root file)".
    pub name: String,
}

/// One F-303 extracted field shown in the disclosure, with its own
/// confidence tier (AC-18, AC-20: low-confidence fields are shown, never
/// hidden).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ExtractedFieldView {
    /// `"title" | "author" | "series" | "series-index" | "year"`.
    pub field: String,
    pub value: String,
    /// `"high" | "medium" | "low"`.
    pub confidence: String,
}

/// One plan operation for the group-detail pane, the F-503 filter, and the
/// F-504 disclosure. `source_path`/`target_path` are raw Windows paths;
/// per FD-13 they must render ONLY inside the "Show file details"
/// disclosure on primary surfaces, never plainly in the row itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlanOpView {
    /// `plan_ops.id`; what `plan_exclude_op` takes.
    pub id: i64,
    /// The stable kebab-case group id (matches [`PlanGroupView::group`]).
    pub group: String,
    /// `"mkdir" | "move" | "rename" | "quarantine" | "rmdir-empty" | "no-op"`.
    pub kind: String,
    /// The reason qualifier for a `no-op` (`"manual-review"`, `"staging"`).
    pub kind_reason: Option<String>,
    pub source_path: String,
    pub target_path: String,
    /// The plain-language sentence stating what the op does and why.
    pub rationale: String,
    /// `"high" | "medium" | "low"`, the op's overall confidence.
    pub confidence: String,
    pub byte_size: i64,
    pub validation: PlanValidation,
    /// The stable machine code (e.g. `"snapshot-stale"`), when not `Valid`.
    pub validation_reason: Option<String>,
    /// A plain-language "needs you" (warning) or remediation (blocked)
    /// sentence, never a bare machine code; `None` when `Valid`.
    pub warning_text: Option<String>,
    pub approval: PlanApproval,
    /// The re-derived F-301 match on this op's own source name; `None` when
    /// the name matched nothing (rare: pattern 8 is a universal fallback) or
    /// the op has no meaningful source name (`mkdir`).
    pub matched_pattern: Option<MatchedPatternView>,
    /// The re-derived F-303 fields, each with its own confidence (AC-18,
    /// AC-20). Empty when nothing could be read from the name.
    pub extracted_fields: Vec<ExtractedFieldView>,
    /// What a rename op's own before/after names removed (only ever set on
    /// `kind == "rename"`).
    pub stripped_noise: Option<String>,
}

/// The flat, filterable op listing (`plan_list_ops`, F-503/F-504): every
/// non-`mkdir` row across all seven groups, capped defensively (see
/// `crate::plan::query::MAX_OPS_RETURNED`). The group-detail pane and the
/// filter box both read from this one list client-side; filtering is
/// view-only and never mutates approval (AC-17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlanOpsPage {
    pub ops: Vec<PlanOpView>,
    /// The real total row count (may exceed `ops.len()` if `truncated`).
    pub total: i64,
    pub truncated: bool,
}

// ---- Phase 6 shell payloads (F-906 ruleset editor + live re-plan) ----

/// One row for the F-906 ruleset editor's "load a different saved ruleset"
/// list (`ruleset_list`): light enough to list many rulesets without sending
/// every one's full policy body. `is_active` marks the one row `plan_generate`
/// currently builds against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RulesetSummary {
    pub id: i64,
    pub name: String,
    pub preset: crate::plan::templates::Preset,
    pub is_active: bool,
    pub updated_at: String,
}

/// One ruleset's full editable detail (`ruleset_get`/`ruleset_save`'s
/// success return): identity plus the complete, validated
/// [`crate::ruleset::Ruleset`] body the editor's preset picker and policy
/// toggles bind to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RulesetDetail {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
    pub ruleset: crate::ruleset::Ruleset,
}

/// The `ruleset_save` input (AC-32): `id: None` creates a new named ruleset;
/// `id: Some(existing)` overwrites that row in place. Either way, saving
/// makes the result the ACTIVE ruleset ("a saved change persists the active
/// ruleset; the review screen regenerates from it").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RulesetSaveRequest {
    pub id: Option<i64>,
    pub name: String,
    pub ruleset: crate::ruleset::Ruleset,
}

/// One F-401 preset's plain-language example (`ruleset_preset_examples`): a
/// fixed sample book rendered under this preset, so the editor's preset
/// picker can show what each choice actually looks like before the user
/// picks it. `example_path` is display-only sample data (never a real book;
/// see [`crate::plan::templates::example_path`]) - plain-language
/// description text lives in the frontend's centralized strings module
/// (FD-23), not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PresetExampleView {
    pub preset: crate::plan::templates::Preset,
    pub example_path: String,
}

/// The live re-plan preview (`plan_preview`, AC-33): the same seven-card
/// shape [`PlanReview::groups`] renders, but for a DRAFT ruleset that has not
/// been saved (and never will be, if the preview is all the user wanted) -
/// this is why there is no `plan_id` here: nothing is persisted to build it
/// (no `plans`/`plan_ops` row is ever written for a preview; plan_ops
/// immutability applies to real plans only, and a preview is not one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlanPreview {
    pub scan_id: i64,
    /// Always exactly seven, in canonical order, exactly like
    /// [`PlanReview::groups`] (AC-10). A group newly blocked by the draft
    /// ruleset shows up here with a non-zero `blocked_count`, never as a
    /// silent drop (F-906 edge case).
    pub groups: Vec<PlanGroupView>,
}

// ---- Phase 1 shell payloads (F-607 executor seam, v0.5.0 Phase 1) ----

/// The result of an `apply_start` run (v0.5.0 Phase 1 dry-run seam), returned by
/// the `apply_start` command. Reports the plan and apply-job the run belongs to,
/// whether it was a dry run, and how many approved operations the executor
/// walked.
///
/// This is the SKELETON return for the seam phase: it proves the dry-run walk ran
/// against a `MemFs` seeded from the plan's snapshot without touching disk (AC-2).
/// The F-904 apply + activity surface (a later phase) grows the real event-driven
/// progress contract on top of the same command; `dry_run` is always `true` here
/// because a Real apply is refused with `apply-not-supported` this phase (D-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ApplyReport {
    /// The `plans.id` this run applied.
    pub plan_id: i64,
    /// The apply `jobs.id` recorded for this run.
    pub job_id: i64,
    /// Whether this was a dry run (always `true` this phase).
    pub dry_run: bool,
    /// How many approved operations the executor walked.
    pub ops_walked: i64,
    /// How many of the walked operations the after-the-fact check (F-604, AC-18)
    /// confirmed: target exists, size matches the snapshot, source gone.
    pub verified_ops: i64,
    /// How many walked operations the after-the-fact check found a difference on
    /// (a move journaled as done that reality contradicts). Zero on a clean apply.
    pub discrepancy_count: i64,
    /// Whether this apply raised an unacknowledged discrepancy block (AC-20). When
    /// true, further FORWARD tidy-ups are paused until it is acknowledged; undo is
    /// never blocked.
    pub blocked: bool,
}

/// The result of preparing an undo (v0.5.0 Phase 5, F-604), returned by the
/// `rollback_prepare` / `rollback_prepare_partial` commands. Preparing an undo
/// builds the INVERSE of an applied tidy-up as an ordinary plan and persists it
/// (D-09: rollback is not a special code path), so the caller navigates to the
/// SAME review surface a forward plan uses (`plan_get(plan_id)`), previews it,
/// approves it, and applies it through the same executor. `plan_id` is that new
/// inverse plan; `op_count` is how many undo operations it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RollbackPrepared {
    /// The `plans.id` of the newly persisted inverse (undo) plan, ready to review.
    pub plan_id: i64,
    /// How many undo operations the inverse plan holds.
    pub op_count: i64,
}

/// The status of one apply `jobs` row plus its after-the-fact check (F-604),
/// returned by `job_status` and `acknowledge_check`. This is what the F-904
/// activity surface (P8) reads to render the done state and the
/// blocked-further-groups state after a verification discrepancy (AC-20, AC-29).
///
/// The block state is DURABLE (it survives a restart, living in
/// `verification_blocks`): a job that raised an unacknowledged discrepancy still
/// reports `blocks_further_tidying = true` after relaunch, so a blocked library
/// is never silently forgotten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobStatus {
    /// The `jobs.id` this status is about.
    pub job_id: i64,
    /// The job's lifecycle state (`running` | `completed` | `failed` | ...).
    pub state: String,
    /// The stable machine error code when the job `failed`, else `None`.
    pub error_code: Option<String>,
    /// Whether this job raised an UNACKNOWLEDGED discrepancy block, so further
    /// FORWARD tidy-ups are paused until it is acknowledged (AC-20). Undo is never
    /// blocked. Cleared by `acknowledge_check`.
    pub blocks_further_tidying: bool,
    /// How many operations the after-the-fact check found a difference on (from
    /// the block detail). Zero when there is no outstanding block.
    pub discrepancy_count: i64,
    /// Whether the job is currently paused at an operation boundary (P8 prelude,
    /// 0a). Sourced from the in-memory [`ApplyControlRegistry`] in the shell; always
    /// `false` for jobs that have already reached a terminal state (they have left
    /// the registry). `jobs.state` stays `"running"` while paused so the
    /// single-writer lock query remains correct.
    pub paused: bool,
    /// How many operations have finished so far, read from the DURABLE journal
    /// (`done` rows). This is the backfill source for the activity surface's
    /// progress line: a fast dry-run that finished before the UI attached its
    /// `apply:op-executed` listeners still shows the true count, not "0 of 0".
    /// The live event carries the same number for the low-latency in-flight case.
    pub done_count: i64,
    /// The total number of approved operations in this job's walk, found from the
    /// plan the job is applying. The backfill partner of `done_count`; `0` only for
    /// a job that has not journaled its first operation yet, which the live events
    /// then fill in.
    pub total: i64,
}

/// Payload of the `apply:op-executed` event (P8 prelude, 0b), emitted after each
/// operation's `done` journal row is committed in a running apply job. The event is
/// fire-and-forget: it never perturbs the walk, the journal, or the AC-3/AC-25
/// equality invariants.
///
/// The shell emits this from the [`crate::exec::OpObserver`] seam; the core never
/// imports Tauri. The UI uses the `kind` and `label` to compose a plain sentence
/// from the frontend strings module (FD-23). Raw paths are deliberately excluded
/// (design-system FD-13, R-12): the `label` is the last path component only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ApplyOpExecutedPayload {
    /// The `jobs.id` this event belongs to (for frontend filtering).
    pub job_id: i64,
    /// The `plan_ops.id` of the just-executed operation.
    pub op_id: i64,
    /// The operation kind: `"move"`, `"rename"`, `"quarantine"`, `"mkdir"`,
    /// `"rmdir-empty"`, or `"no-op"`.
    pub kind: String,
    /// A plain-language display label for the book or folder - the last path
    /// component of `source_path` (extension stripped for audio-file moves), or
    /// the last component of `target_path` for `mkdir`. Never a full raw path.
    pub label: String,
    /// How many operations have completed so far (the done count, including this one).
    pub done_count: i64,
    /// The total number of approved operations in this walk.
    pub total: i64,
}

// ---- Phase 5 shell payloads (tauri-specta seam) ----

/// Returned by the `scan_start` command the instant a scan is accepted (F-104).
///
/// `scan_start` inserts a `running` `jobs` row, spawns the scan on the async
/// runtime, and returns this immediately; the caller then waits for the
/// [`JobCompletedPayload`] / [`JobFailedPayload`] event carrying the same
/// `job_id`. This decouples the long-running scan from the IPC call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobStarted {
    /// `jobs.id` of the row created for this scan; correlates the later
    /// `job:completed` / `job:failed` event back to this call.
    pub job_id: i64,
}

/// Returned by the `db_status` command: the wire form of the startup
/// [`crate::db::DbOpenOutcome`] (P2), reporting whether corrupt-DB recovery ran.
///
/// [`crate::db::DbOpenOutcome`] is a startup-internal Rust type (it carries a
/// `PathBuf`) and is deliberately NOT part of the IPC contract; the shell maps it
/// to this serde/specta payload once, at the boundary. `recovered` is true
/// exactly when the existing database was unreadable and was reset; when true,
/// `backup_path` is where the prior database was preserved (the human-readable
/// stored form), else `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DbStatus {
    /// True exactly when startup found a corrupt/unopenable database and reset it
    /// (see [`crate::db::DbOpenOutcome::Recovered`]).
    pub recovered: bool,
    /// Where the corrupt database was preserved, when `recovered`; else `None`.
    pub backup_path: Option<String>,
}

/// Payload of the `job:completed` event, emitted when a spawned scan finishes
/// successfully. Carries the originating `job_id` and the `scan_id` of the
/// snapshot written, so the tracer can read the entries back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobCompletedPayload {
    /// The `jobs.id` returned earlier by `scan_start`.
    pub job_id: i64,
    /// The `scans.id` of the snapshot the completed scan wrote.
    pub scan_id: i64,
}

/// Payload of the `job:failed` event, emitted when a spawned scan errors. Carries
/// the originating `job_id` and the stable machine `code` from
/// [`AppError::code`], so the frontend can key off the same codes the command
/// results use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobFailedPayload {
    /// The `jobs.id` returned earlier by `scan_start`.
    pub job_id: i64,
    /// The stable kebab-case error code (equals [`AppError::code`]).
    pub code: String,
}

/// Payload of the `job:stopped` event (P8, IMPORTANT 3), emitted after an apply
/// job's DISTINCT `stopped` terminal state is durably marked in the `jobs` row
/// (a cooperative Stop, AC-26). It mirrors the `job:completed`/`job:failed`
/// terminal events so the activity surface transitions to "stopped between books"
/// reliably instead of racing the walk's final state write with a single status
/// poll. Fire-and-forget and post-durable-state: a dropped event never perturbs
/// the walk, and the status poll remains the fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobStoppedPayload {
    /// The `jobs.id` of the apply job that stopped cooperatively.
    pub job_id: i64,
}

/// Payload of the `job:progress` event.
///
/// Frozen NOW so the IPC contract is complete, even though the v0.1.0 spine
/// NEVER emits it: the spine scan is a single spawned unit with no progress
/// reporting or cancellation (this phase's brief). Wiring an emitter is a later
/// release's concern; pinning the shape here means adding progress later is an
/// additive change that does not perturb the frozen bindings surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct JobProgressPayload {
    /// The `jobs.id` this progress update belongs to.
    pub job_id: i64,
    /// Units of work completed so far.
    pub done: i64,
    /// Best-known total units, or `None` when the total is not yet known
    /// (an indeterminate phase).
    pub total_estimate: Option<i64>,
    /// A short human-readable label for the current step (for example a path or
    /// stage name).
    pub current_label: String,
}

// ---- F-703 / F-905 duplicates surface (v0.6.0 P5) ----------------------------

/// What is known about one copy's content (`F-702`).
///
/// `NotChecked` and `CouldNotRead` are deliberately different states rather than
/// one "not verified": the first is work nobody has done, the second is a file
/// the app tried to read and could not, and `AC-12`'s gate treats them the same
/// while the person deciding does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum CopyCheckState {
    NotChecked,
    Checked,
    CouldNotRead,
}

/// One copy inside a duplicate group (`AC-17`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DuplicateCopyView {
    /// `entries.id` in the scan this review was built from. Per-snapshot, which
    /// is why a confirmation carrying it is keyed to its scan.
    pub entry_id: i64,
    pub path: String,
    pub size_bytes: i64,
    pub check: CopyCheckState,
    /// The plain-language label for `check`, produced by `dupes::review` so the
    /// wording stays inside the sweep that governs it rather than being invented
    /// again in TypeScript.
    pub check_label: String,
    /// Why the read failed. `Some` only when `check` is `could-not-read`.
    pub check_reason: Option<String>,
    /// Whether the default suggestion would keep this copy (`AC-19`). A
    /// SUGGESTION, carrying no permission to act (`AC-26`).
    pub suggested_keeper: bool,
}

/// One duplicate group: one book, N copies (`FD-08`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DuplicateGroupCard {
    /// A readable name for the book (`AC-17`).
    pub book: String,
    /// The detector's join key. Never rendered as if it were a book name: it
    /// reads like `Dune.m4b|900`, and a card that showed it would be lying about
    /// what it is.
    pub group_key: String,
    /// The detector that found the group. With `group_key` this is the group's
    /// identity, and what a confirmation is recorded against.
    pub method: String,
    /// How the group was found, in plain language.
    pub found_by: String,
    pub copies: Vec<DuplicateCopyView>,
    pub copy_count: i64,
    /// Sum of every copy's bytes: an ESTIMATE of what is duplicated, never the
    /// space a resolution would reclaim (`AC-18`).
    pub candidate_bytes_estimate: i64,
    /// Why the suggested keeper was suggested, in plain language. Expect
    /// "the copies were equivalent" to be the COMMON answer rather than a rare
    /// one: the policies discriminate almost nowhere resolution is permitted.
    pub keeper_reason: Option<String>,
    /// `AC-12`'s gate: every copy hashed and all the hashes agreeing. False
    /// blocks the automatic path and is what `AC-13`'s two-step override
    /// overrides.
    pub content_verified: bool,
    /// `entries.id` of the copy the user CONFIRMED to keep (`AC-24`), or `None`
    /// while the group is still undecided.
    ///
    /// A decision, not a suggestion: `suggested_keeper` on a copy is what the
    /// app proposes, this is what a person chose, and only this one produces
    /// Archive operations. Keeping them as separate fields is deliberate, since
    /// a surface that showed one as the other would imply the app had already
    /// decided something it has not.
    pub confirmed_keeper: Option<i64>,
}

/// The duplicates surface's whole read model for one scan (`F-905`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DuplicatesReviewView {
    /// The scan this was detected from. The surface sends it back with any
    /// confirmation, which is what keeps a confirmation from outliving its
    /// snapshot.
    pub scan_id: i64,
    pub groups: Vec<DuplicateGroupCard>,
    /// The headline number, and it counts GROUPS (`AC-18`).
    pub group_count: i64,
    /// Copies across every group, reported BESIDE the group count and never
    /// instead of it: "14 copies" and "6 duplicated books" answer different
    /// questions, and a past release shipped the first while meaning the second.
    pub copy_count: i64,
    pub candidate_bytes_estimate: i64,
}

/// Where an export landed, so the surface can say so and offer to open it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ExportedFile {
    pub path: String,
}

#[cfg(test)]
mod contract {
    use super::*;

    /// Compile-time proof that a type carries the three derives tauri-specta
    /// requires of every IPC payload and return type (AC-4). This function has
    /// an empty body; its VALUE is its `where` clause. If any type below loses a
    /// derive, the instantiation in [`every_cross_boundary_type_is_ipc_ready`]
    /// stops compiling and the failure names the exact type, here in abo-core's
    /// test build, rather than deep inside the Phase 5 bindings generator.
    ///
    /// The deserialize bound is [`serde::de::DeserializeOwned`] (`for<'de>
    /// Deserialize<'de>`), which every owned payload satisfies and which is what
    /// `Result<T, AppError>` needs to round-trip across the boundary.
    fn assert_ipc_ready<T>()
    where
        T: serde::Serialize + serde::de::DeserializeOwned + specta::Type,
    {
    }

    /// One instantiation per cross-boundary type. Keep this list complete: every
    /// struct returned to or accepted from the frontend, plus [`AppError`],
    /// belongs here.
    #[test]
    fn every_cross_boundary_type_is_ipc_ready() {
        assert_ipc_ready::<ScanSummary>();
        assert_ipc_ready::<ScanWarning>();
        assert_ipc_ready::<ScanWarningKind>();
        assert_ipc_ready::<EntryRow>();
        assert_ipc_ready::<AppError>();
        // F-803 app settings (v0.4.0 Phase 2).
        assert_ipc_ready::<AppSettings>();
        // F-907 cover art (v0.4.0 Phase 3).
        assert_ipc_ready::<CoverImage>();
        // F-902 library home (v0.4.0 Phase 4).
        assert_ipc_ready::<ReasonKind>();
        assert_ipc_ready::<BookReason>();
        assert_ipc_ready::<BookExample>();
        assert_ipc_ready::<SeriesCluster>();
        assert_ipc_ready::<GoodNews>();
        assert_ipc_ready::<MetricUnit>();
        assert_ipc_ready::<FolderClass>();
        assert_ipc_ready::<ClassMetric>();
        assert_ipc_ready::<ProblemMetric>();
        assert_ipc_ready::<HealthMetrics>();
        assert_ipc_ready::<LibraryOverview>();
        // F-903 plan review surface (v0.4.0 Phase 5).
        assert_ipc_ready::<PlanApproval>();
        assert_ipc_ready::<PlanValidation>();
        assert_ipc_ready::<GroupStatus>();
        assert_ipc_ready::<PlanGroupView>();
        assert_ipc_ready::<PlanReview>();
        assert_ipc_ready::<MatchedPatternView>();
        assert_ipc_ready::<ExtractedFieldView>();
        assert_ipc_ready::<PlanOpView>();
        assert_ipc_ready::<PlanOpsPage>();
        // F-607 executor seam (v0.5.0 Phase 1).
        assert_ipc_ready::<ApplyMode>();
        assert_ipc_ready::<ApplyReport>();
        // F-604 rollback as an inverse plan (v0.5.0 Phase 5).
        assert_ipc_ready::<RollbackPrepared>();
        // F-604 after-the-fact check status (v0.5.0 Phase 6).
        assert_ipc_ready::<JobStatus>();
        // Phase 5 shell payloads (command returns + event payloads).
        assert_ipc_ready::<JobStarted>();
        assert_ipc_ready::<DbStatus>();
        assert_ipc_ready::<JobCompletedPayload>();
        assert_ipc_ready::<JobFailedPayload>();
        assert_ipc_ready::<JobProgressPayload>();
        // F-906 ruleset editor + live re-plan (v0.4.0 Phase 6).
        assert_ipc_ready::<crate::ruleset::Ruleset>();
        assert_ipc_ready::<crate::plan::templates::Preset>();
        assert_ipc_ready::<RulesetSummary>();
        assert_ipc_ready::<RulesetDetail>();
        assert_ipc_ready::<RulesetSaveRequest>();
        assert_ipc_ready::<PresetExampleView>();
        assert_ipc_ready::<PlanPreview>();
        // F-703/F-905 duplicates surface (v0.6.0 P5).
        assert_ipc_ready::<CopyCheckState>();
        assert_ipc_ready::<DuplicateCopyView>();
        assert_ipc_ready::<DuplicateGroupCard>();
        assert_ipc_ready::<DuplicatesReviewView>();
        assert_ipc_ready::<ExportedFile>();
    }
}
