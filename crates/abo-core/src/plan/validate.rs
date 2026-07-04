//! F-404 (plan validation): the backstop that assigns every built operation a
//! verdict - `valid`, `warning(reason)`, or `blocked(reason)` - before a plan
//! could ever reach an executor.
//!
//! Validation is the wall between a generated plan and anything that will touch
//! disk. Every hazard Windows reality can produce is caught here, mechanically,
//! and each blocking hazard's machine code is one of the breakdown Section 8
//! "Plan" family codes (the same strings [`crate::error::AppError`] carries, so
//! a stored `plan_ops.validation_reason` and an apply-time refusal speak one
//! vocabulary; the agreement is contract-tested below).
//!
//! # Purity and the platform seam (the CFG RULE)
//!
//! Like the rest of `crate::plan`, this module has NO `cfg`-gating of any kind.
//! It is pure logic over the built plan plus an injected [`ValidationEnv`]. The
//! two facts that genuinely depend on the host - whether Windows long-path
//! support is enabled, and how many bytes a volume has free - enter through the
//! env: `long_paths_enabled` is a plain `bool`, and free space arrives through
//! the [`FreeSpace`] trait. Tests inject both deterministically ([`FixedFreeSpace`]);
//! the one production wiring reads the registry (`crate::scan::longpath::long_paths_enabled`)
//! and the disk (`crate::paths::available_bytes`, via [`RealFreeSpace`]), both of
//! which confine their `cfg(windows)` to the platform seam, never to this module.
//!
//! # The hazard list (spec F-404, AC-12..AC-15)
//!
//! - in-plan target collisions (two ops -> one path, compared case-insensitively
//!   for NTFS): both `blocked(collision-in-plan)` (AC-15);
//! - on-disk collisions (a target that already exists and is not vacated by the
//!   plan): `blocked(collision-on-disk)`;
//! - source-inside-target cycles (a move into its own subtree):
//!   `blocked(cycle-detected)`;
//! - path length: `blocked(path-too-long)` beyond the `\\?\` extended-length
//!   allowance, with a `warning(path-length-near-260)` interop note near/over the
//!   legacy 260-char limit and a `warning(long-paths-disabled)` how-to note when
//!   `LongPathsEnabled=0` (AC-13, AC-14);
//! - illegal components and reserved device names (backstop to F-304):
//!   `blocked(illegal-component)` / `blocked(reserved-name)`;
//! - cross-volume moves marked `copy+verify+delete`: `warning(cross-volume-copy)`
//!   when the summed byte estimate fits the target volume's free space,
//!   `blocked(cross-volume-space-insufficient)` when it does not;
//! - snapshot staleness (a source that vanished since the scan):
//!   `blocked(snapshot-stale)`.
//!
//! Junctions/reparse points are never followed: the scanner records them and
//! does not descend (`crate::scan`), so a plan never contains an operation whose
//! source lies beyond a junction; validation inherits that safety and adds
//! nothing that would traverse one.

use std::collections::{HashMap, HashSet};

use crate::plan::builder::{group_for_op_group, BuiltPlan, CampaignGroup, PlannedOp};
use crate::scan::longpath::{LONG_PATHS_HOWTO, MAX_PATH_LEGACY, NEAR_LIMIT_THRESHOLD};

/// The maximum full-path length once the Windows extended-length (`\\?\`)
/// allowance is in play (FD-19). A target longer than this cannot be stored
/// even with long-path support, so it is `blocked(path-too-long)`. This is the
/// BLOCK threshold; [`MAX_PATH_LEGACY`] (260) is the separate WARN threshold.
pub const EXTENDED_LENGTH_LIMIT: usize = 32_767;

// ---- verdict model ----

/// A per-operation validation state, stored in `plan_ops.validation_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationState {
    /// Safe to execute as-is.
    Valid,
    /// Executable, but the user should be told something first.
    Warning,
    /// Must not execute; fixed upstream or excluded (AC-17: never approved).
    Blocked,
}

impl ValidationState {
    /// The stable string stored in `plan_ops.validation_state`.
    pub fn as_str(self) -> &'static str {
        match self {
            ValidationState::Valid => "valid",
            ValidationState::Warning => "warning",
            ValidationState::Blocked => "blocked",
        }
    }
}

/// The machine-readable reason a verdict is a warning or a block. The BLOCKING
/// reasons' [`ValidationReason::code`] strings are exactly the breakdown Section
/// 8 "Plan" family codes (contract-tested against [`crate::error::AppError`]);
/// the three WARNING-only reasons carry validation-specific codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationReason {
    // ---- blocking (Section 8 Plan family) ----
    /// A source path no longer exists at validation time.
    SnapshotStale,
    /// A move/rename would place a folder inside its own subtree.
    CycleDetected,
    /// Two ops in the plan produce one path (case-insensitive).
    CollisionInPlan,
    /// A target already exists on disk and is not vacated by the plan.
    CollisionOnDisk,
    /// A path component is a reserved Windows device name.
    ReservedName,
    /// A path component holds an illegal character or a trailing dot/space.
    IllegalComponent,
    /// A target path exceeds the extended-length allowance.
    PathTooLong,
    /// Cross-volume moves to one volume sum beyond its free space.
    CrossVolumeSpaceInsufficient,
    /// A pack shell's quarantine is blocked because a member move is blocked, so
    /// the shell would strand an un-extracted book (AC-5 guard, folded to
    /// validation time). Validation-specific (not a Section 8 code).
    PackMemberBlocked,
    // ---- warning only ----
    /// The target is near/over 260 chars: still openable via `\\?\`, but other
    /// Windows tools may mishandle it. Validation-specific.
    PathLengthNear260,
    /// The target is over 260 chars AND long-path support is disabled; carries a
    /// how-to link. Validation-specific.
    LongPathsDisabled,
    /// A cross-volume move that fits: executed as copy+verify+delete rather than
    /// a metadata rename. Validation-specific.
    CrossVolumeCopy,
}

impl ValidationReason {
    /// The stable machine code stored in `plan_ops.validation_reason`.
    pub fn code(self) -> &'static str {
        match self {
            ValidationReason::SnapshotStale => "snapshot-stale",
            ValidationReason::CycleDetected => "cycle-detected",
            ValidationReason::CollisionInPlan => "collision-in-plan",
            ValidationReason::CollisionOnDisk => "collision-on-disk",
            ValidationReason::ReservedName => "reserved-name",
            ValidationReason::IllegalComponent => "illegal-component",
            ValidationReason::PathTooLong => "path-too-long",
            ValidationReason::CrossVolumeSpaceInsufficient => "cross-volume-space-insufficient",
            ValidationReason::PackMemberBlocked => "pack-member-blocked",
            ValidationReason::PathLengthNear260 => "path-length-near-260",
            ValidationReason::LongPathsDisabled => "long-paths-disabled",
            ValidationReason::CrossVolumeCopy => "cross-volume-copy",
        }
    }

    /// Whether this reason blocks (vs merely warns).
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            ValidationReason::SnapshotStale
                | ValidationReason::CycleDetected
                | ValidationReason::CollisionInPlan
                | ValidationReason::CollisionOnDisk
                | ValidationReason::ReservedName
                | ValidationReason::IllegalComponent
                | ValidationReason::PathTooLong
                | ValidationReason::CrossVolumeSpaceInsufficient
                | ValidationReason::PackMemberBlocked
        )
    }
}

/// The verdict for one operation, aligned by index with `BuiltPlan::ops`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpVerdict {
    pub state: ValidationState,
    pub reason: Option<ValidationReason>,
    /// A how-to link reference for the `long-paths-disabled` warning (AC-14);
    /// `None` for every other verdict.
    pub how_to: Option<&'static str>,
}

impl OpVerdict {
    /// A clean verdict (no issue found).
    pub fn valid() -> Self {
        OpVerdict {
            state: ValidationState::Valid,
            reason: None,
            how_to: None,
        }
    }

    fn warning(reason: ValidationReason, how_to: Option<&'static str>) -> Self {
        OpVerdict {
            state: ValidationState::Warning,
            reason: Some(reason),
            how_to,
        }
    }

    fn blocked(reason: ValidationReason) -> Self {
        OpVerdict {
            state: ValidationState::Blocked,
            reason: Some(reason),
            how_to: None,
        }
    }

    /// The stored `validation_reason` string, or `None` for a valid op.
    pub fn reason_code(self) -> Option<&'static str> {
        self.reason.map(|r| r.code())
    }

    pub fn is_blocked(self) -> bool {
        self.state == ValidationState::Blocked
    }
    pub fn is_warning(self) -> bool {
        self.state == ValidationState::Warning
    }
}

// ---- free-space seam ----

/// Bytes available on a named volume. The pure validator depends only on this
/// trait, so tests inject scarcity ([`FixedFreeSpace`]) and production reads the
/// real disk ([`RealFreeSpace`]). `None` means "unknown": validation then marks
/// a cross-volume move but never falsely blocks it for lack of a measurement.
pub trait FreeSpace {
    /// Available bytes on `volume` (e.g. `"E:"`), or `None` if unknown.
    fn available_bytes(&self, volume: &str) -> Option<u64>;
}

/// The production free-space source: [`crate::paths::available_bytes`] on the
/// volume root. Windows-real; on other hosts the underlying call returns `None`.
pub struct RealFreeSpace;

impl FreeSpace for RealFreeSpace {
    fn available_bytes(&self, volume: &str) -> Option<u64> {
        // `volume` is a root like "E:"; query "E:\" so the OS resolves the drive.
        let root = format!("{volume}\\");
        crate::paths::available_bytes(std::path::Path::new(&root))
    }
}

/// A deterministic free-space source for tests and injected scenarios: a map of
/// volume -> available bytes. A volume absent from the map reads as `None`
/// (unknown). Public so integration tests and the hostile fixture can inject
/// scarcity without reaching the real disk.
#[derive(Debug, Clone, Default)]
pub struct FixedFreeSpace {
    by_volume: HashMap<String, u64>,
}

impl FixedFreeSpace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the available bytes for `volume` (chainable).
    pub fn with(mut self, volume: &str, available: u64) -> Self {
        self.by_volume.insert(volume.to_string(), available);
        self
    }
}

impl FreeSpace for FixedFreeSpace {
    fn available_bytes(&self, volume: &str) -> Option<u64> {
        self.by_volume.get(volume).copied()
    }
}

// ---- validation environment ----

/// Everything validation needs beyond the plan itself. Callers assemble this
/// from a fresh view of the filesystem (the real path) or seed it directly
/// (tests / the hostile fixture).
pub struct ValidationEnv<'a> {
    /// Paths that exist on disk at validation time (the snapshot's sources plus
    /// anything else present). Compared case-insensitively. Used for BOTH the
    /// on-disk collision check (a target already here) and the staleness check
    /// (a source no longer here).
    pub existing_paths: &'a HashSet<String>,
    /// Whether Windows long-path support is enabled on this host (FD-19). Feeds
    /// the `long-paths-disabled` warning (AC-14).
    pub long_paths_enabled: bool,
    /// The free-space source for cross-volume sizing.
    pub free_space: &'a dyn FreeSpace,
}

impl<'a> ValidationEnv<'a> {
    /// Convenience constructor.
    pub fn new(
        existing_paths: &'a HashSet<String>,
        long_paths_enabled: bool,
        free_space: &'a dyn FreeSpace,
    ) -> Self {
        ValidationEnv {
            existing_paths,
            long_paths_enabled,
            free_space,
        }
    }
}

// ---- path helpers (pure string semantics, no cfg) ----

/// Case- and separator-insensitive normal form for comparison: backslashes to
/// forward slashes, lowercased (NTFS is case-insensitive). Never persisted.
fn norm(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// Whether `child` lies strictly inside `parent` (both already `norm`ed).
fn is_descendant(child: &str, parent: &str) -> bool {
    let mut prefix = String::with_capacity(parent.len() + 1);
    prefix.push_str(parent);
    prefix.push('/');
    child.starts_with(&prefix)
}

/// The volume a path belongs to: an uppercased drive root (`"E:"`) or a UNC
/// share (`"//server/share"`, normalized to forward slashes). `None` for a
/// relative or synthetic (rootless) path, which validation treats as a single
/// implicit volume (never cross-volume).
fn volume_of(path: &str) -> Option<String> {
    let unified = path.replace('\\', "/");
    let bytes = unified.as_bytes();
    // Drive path: `X:/...` or `X:`.
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some(format!("{}:", (bytes[0] as char).to_ascii_uppercase()));
    }
    // UNC: `//server/share/...`.
    if let Some(rest) = unified.strip_prefix("//") {
        let mut it = rest.split('/').filter(|s| !s.is_empty());
        if let (Some(server), Some(share)) = (it.next(), it.next()) {
            return Some(format!("//{}/{}", server.to_lowercase(), share.to_lowercase()));
        }
    }
    None
}

/// The path components to name-check: everything but the drive/UNC root and any
/// empty segment. Original case preserved (reserved/illegal checks case-fold
/// themselves where needed).
fn checkable_components(path: &str) -> Vec<String> {
    let unified = path.replace('\\', "/");
    unified
        .split('/')
        .filter(|c| !c.is_empty())
        // Drop a drive-letter token like `E:` (its colon is legal only there).
        .filter(|c| !(c.len() == 2 && c.as_bytes()[1] == b':' && c.as_bytes()[0].is_ascii_alphabetic()))
        .map(|c| c.to_string())
        .collect()
}

/// The reserved DOS device names (case-insensitive), checked against a
/// component's base (the part before the first `.`).
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Whether `component`'s base name is a reserved Windows device name.
fn is_reserved_component(component: &str) -> bool {
    let base = component.split('.').next().unwrap_or(component);
    let upper = base.trim().to_ascii_uppercase();
    RESERVED_DEVICE_NAMES.contains(&upper.as_str())
}

/// Whether `component` holds a character illegal in a Windows filename, or ends
/// in a dot or space (which Windows silently strips).
fn is_illegal_component(component: &str) -> bool {
    if component.is_empty() {
        return false;
    }
    let illegal_char = component
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || (c as u32) < 0x20);
    let trailing = component.ends_with('.') || component.ends_with(' ');
    illegal_char || trailing
}

/// The character length of a path as the length checks measure it.
fn path_len(path: &str) -> usize {
    path.chars().count()
}

/// Whether an op's kind creates something at its target (so its target is a
/// collision / name / length subject).
fn creates_target(kind: &str) -> bool {
    matches!(kind, "move" | "rename" | "mkdir" | "quarantine")
}

/// Whether an op acts on an existing source (so its source is a staleness
/// subject and can vacate a path).
fn acts_on_source(kind: &str) -> bool {
    matches!(kind, "move" | "rename" | "quarantine" | "rmdir-empty")
}

/// Whether an op is a cross-volume move (source and target on different, known
/// volumes). Renames are same-directory, so never cross-volume.
fn is_cross_volume(op: &PlannedOp) -> bool {
    if !matches!(op.kind.as_str(), "move" | "quarantine") {
        return false;
    }
    match (volume_of(&op.source_path), volume_of(&op.target_path)) {
        (Some(s), Some(t)) => s != t,
        _ => false,
    }
}

// ---- the validator ----

/// Validate a built plan, returning one [`OpVerdict`] per op, aligned by index
/// with `plan.ops`. Pure: identical `(plan, env)` yields identical verdicts.
pub fn validate_plan(plan: &BuiltPlan, env: &ValidationEnv) -> Vec<OpVerdict> {
    let existing: HashSet<String> = env.existing_paths.iter().map(|p| norm(p)).collect();

    // Pre-pass 1: in-plan target collisions. Count normalized targets among
    // creating ops with a non-empty target.
    let mut target_counts: HashMap<String, u32> = HashMap::new();
    for op in &plan.ops {
        if creates_target(&op.kind) && !op.target_path.is_empty() {
            *target_counts.entry(norm(&op.target_path)).or_default() += 1;
        }
    }

    // Pre-pass 2: paths the plan vacates (a source moved/removed away), so a
    // target landing on a soon-to-be-empty path is not an on-disk collision.
    let vacated: HashSet<String> = plan
        .ops
        .iter()
        .filter(|op| acts_on_source(&op.kind) && !op.source_path.is_empty())
        .map(|op| norm(&op.source_path))
        .collect();

    // Pre-pass 3: cross-volume byte totals per target volume, then the set of
    // volumes whose demand exceeds free space.
    let mut needed_by_volume: HashMap<String, u64> = HashMap::new();
    for op in &plan.ops {
        if is_cross_volume(op) {
            if let Some(vol) = volume_of(&op.target_path) {
                let bytes = op.byte_size.max(0) as u64;
                *needed_by_volume.entry(vol).or_default() += bytes;
            }
        }
    }
    let insufficient_volumes: HashSet<String> = needed_by_volume
        .iter()
        .filter(|(vol, needed)| match env.free_space.available_bytes(vol) {
            Some(avail) => **needed > avail,
            None => false, // unknown free space: mark (warn) but never block.
        })
        .map(|(vol, _)| vol.clone())
        .collect();

    // Main pass: one verdict per op.
    let mut verdicts: Vec<OpVerdict> = plan
        .ops
        .iter()
        .map(|op| verdict_for_op(op, &existing, &target_counts, &vacated, &insufficient_volumes, env))
        .collect();

    // Post-pass: fold a blocked pack member into the pack-shell-quarantine
    // guard (AC-5 at validation time). A shell whose member move is blocked must
    // not be quarantined, or it would strand an un-extracted book.
    apply_pack_shell_guard(&plan.ops, &mut verdicts);

    verdicts
}

fn verdict_for_op(
    op: &PlannedOp,
    existing: &HashSet<String>,
    target_counts: &HashMap<String, u32>,
    vacated: &HashSet<String>,
    insufficient_volumes: &HashSet<String>,
    env: &ValidationEnv,
) -> OpVerdict {
    // A no-op touches nothing on disk; it is always valid (a manual-review no-op
    // is a deliberate hand-off, not an error state).
    if op.kind == "no-op" {
        return OpVerdict::valid();
    }

    // ---- blocking checks, in priority order (first hit wins) ----

    // 1. Snapshot staleness: the source must still exist.
    if acts_on_source(&op.kind)
        && !op.source_path.is_empty()
        && !existing.contains(&norm(&op.source_path))
    {
        return OpVerdict::blocked(ValidationReason::SnapshotStale);
    }

    // 2. Cycle: a move/rename into its own subtree.
    if matches!(op.kind.as_str(), "move" | "rename") {
        let s = norm(&op.source_path);
        let t = norm(&op.target_path);
        if !s.is_empty() && !t.is_empty() && is_descendant(&t, &s) {
            return OpVerdict::blocked(ValidationReason::CycleDetected);
        }
    }

    if creates_target(&op.kind) && !op.target_path.is_empty() {
        let tnorm = norm(&op.target_path);

        // 3. In-plan collision: another creating op produces the same target.
        if target_counts.get(&tnorm).copied().unwrap_or(0) > 1 {
            return OpVerdict::blocked(ValidationReason::CollisionInPlan);
        }

        // 4. On-disk collision: the target already exists and is not vacated.
        if existing.contains(&tnorm) && !vacated.contains(&tnorm) {
            return OpVerdict::blocked(ValidationReason::CollisionOnDisk);
        }

        // 5/6. Reserved / illegal component (F-304 backstop).
        for comp in checkable_components(&op.target_path) {
            if is_reserved_component(&comp) {
                return OpVerdict::blocked(ValidationReason::ReservedName);
            }
            if is_illegal_component(&comp) {
                return OpVerdict::blocked(ValidationReason::IllegalComponent);
            }
        }

        // 7. Path too long (beyond the extended-length allowance).
        if path_len(&op.target_path) > EXTENDED_LENGTH_LIMIT {
            return OpVerdict::blocked(ValidationReason::PathTooLong);
        }
    }

    // 8. Cross-volume space insufficient.
    if is_cross_volume(op) {
        if let Some(vol) = volume_of(&op.target_path) {
            if insufficient_volumes.contains(&vol) {
                return OpVerdict::blocked(ValidationReason::CrossVolumeSpaceInsufficient);
            }
        }
    }

    // ---- warning checks (only reached if nothing blocked) ----

    if creates_target(&op.kind) && !op.target_path.is_empty() {
        let len = path_len(&op.target_path);
        if len > MAX_PATH_LEGACY {
            // Over 260 but within the extended-length allowance: openable via
            // `\\?\`, but interop-risky. When long paths are disabled, carry the
            // how-to link (AC-14); otherwise a plain interop note (AC-13).
            if !env.long_paths_enabled {
                return OpVerdict::warning(
                    ValidationReason::LongPathsDisabled,
                    Some(LONG_PATHS_HOWTO),
                );
            }
            return OpVerdict::warning(ValidationReason::PathLengthNear260, None);
        }
        if len >= NEAR_LIMIT_THRESHOLD {
            // 248..=260: near the legacy limit, interop-risky (AC-13).
            return OpVerdict::warning(ValidationReason::PathLengthNear260, None);
        }
    }

    // Cross-volume move that fits: mark it copy+verify+delete (a warning, since
    // it is not a metadata-only rename).
    if is_cross_volume(op) {
        return OpVerdict::warning(ValidationReason::CrossVolumeCopy, None);
    }

    OpVerdict::valid()
}

/// AC-5 guard at validation time: if any flatten-packs MEMBER move is blocked,
/// the matching pack-shell quarantine is blocked too (it would otherwise strand
/// the un-extracted book). The shell's `source_path` is the pack folder; a
/// member move's `source_path` is a descendant of it.
fn apply_pack_shell_guard(ops: &[PlannedOp], verdicts: &mut [OpVerdict]) {
    // Collect the normalized sources of BLOCKED flatten-packs member moves.
    let blocked_member_sources: Vec<String> = ops
        .iter()
        .zip(verdicts.iter())
        .filter(|(op, v)| {
            op.op_group == "flatten-packs" && op.kind == "move" && v.is_blocked()
        })
        .map(|(op, _)| norm(&op.source_path))
        .collect();
    if blocked_member_sources.is_empty() {
        return;
    }
    for (op, v) in ops.iter().zip(verdicts.iter_mut()) {
        if op.rule_id == "flatten-packs-shell" && op.kind == "quarantine" {
            let shell = norm(&op.source_path);
            let strands = blocked_member_sources
                .iter()
                .any(|m| is_descendant(m, &shell));
            if strands && !v.is_blocked() {
                *v = OpVerdict::blocked(ValidationReason::PackMemberBlocked);
            }
        }
    }
}

// ---- summary ----

/// Aggregate verdict counts for a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ValidationSummary {
    pub valid: u64,
    pub warning: u64,
    pub blocked: u64,
}

impl ValidationSummary {
    /// Whether any op is blocked.
    pub fn has_blocked(self) -> bool {
        self.blocked > 0
    }
}

/// Summarize a verdict list.
pub fn summarize(verdicts: &[OpVerdict]) -> ValidationSummary {
    let mut s = ValidationSummary::default();
    for v in verdicts {
        match v.state {
            ValidationState::Valid => s.valid += 1,
            ValidationState::Warning => s.warning += 1,
            ValidationState::Blocked => s.blocked += 1,
        }
    }
    s
}

// ---- persistence (validate-before-insert; no UPDATE of frozen columns) ----

/// Persist a built plan WITH its real validation verdicts, in one insert.
///
/// This is the F-404 "validation runs before a plan is ever persisted" path
/// (migration 0002's freeze note): each op's `validation_state` /
/// `validation_reason` is written ONCE at insert time from `verdicts`, so no
/// scoped UPDATE of the frozen descriptive columns is ever needed. (Contrast
/// [`crate::plan::builder::persist_plan`], which writes `pending` for callers
/// that validate separately.) `verdicts` must align by index with `plan.ops`.
pub async fn persist_validated_plan(
    pool: &sqlx::SqlitePool,
    scan_id: i64,
    ruleset_id: i64,
    status: &str,
    plan: &BuiltPlan,
    verdicts: &[OpVerdict],
    now: &str,
) -> Result<i64, sqlx::Error> {
    use crate::db::plans::{insert_plan, NewPlan, NewPlanOp};

    assert_eq!(
        plan.ops.len(),
        verdicts.len(),
        "verdicts must align 1:1 with plan ops"
    );

    let stats_json = serde_json::to_string(&plan.stats).expect("stats always serialize");
    let ops: Vec<NewPlanOp> = plan
        .ops
        .iter()
        .zip(verdicts.iter())
        .map(|(op, v)| NewPlanOp {
            op_group: op.op_group.as_str(),
            kind: op.kind.as_str(),
            kind_reason: op.kind_reason.as_deref(),
            source_path: op.source_path.as_str(),
            target_path: op.target_path.as_str(),
            rationale: op.rationale.as_str(),
            rule_id: op.rule_id.as_str(),
            confidence: op.confidence.as_str(),
            byte_size: op.byte_size,
            validation_state: v.state.as_str(),
            validation_reason: v.reason_code(),
            provenance_json: op.provenance_json.as_deref(),
        })
        .collect();

    insert_plan(
        pool,
        &NewPlan {
            scan_id,
            ruleset_id,
            status,
            stats_json: Some(&stats_json),
        },
        &ops,
        now,
    )
    .await
}

// ---- approval state machine (F-405 AC-17) ----

/// The approval state riding beside an op (`plan_ops.approval`). The one
/// mutable pair on an otherwise write-once row (migration 0002 freeze note).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Excluded,
}

impl ApprovalState {
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalState::Pending => "pending",
            ApprovalState::Approved => "approved",
            ApprovalState::Rejected => "rejected",
            ApprovalState::Excluded => "excluded",
        }
    }

    /// Parse a stored `approval` string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ApprovalState::Pending),
            "approved" => Some(ApprovalState::Approved),
            "rejected" => Some(ApprovalState::Rejected),
            "excluded" => Some(ApprovalState::Excluded),
            _ => None,
        }
    }
}

/// A requested approval transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalAction {
    Approve,
    Reject,
    Exclude,
    /// Return an op to `pending` (undo a prior decision).
    Reset,
}

/// Why an approval transition was refused, or an IO failure applying one.
#[derive(Debug)]
pub enum ApprovalError {
    /// AC-17: a `blocked` op cannot be approved; it must be fixed upstream or
    /// excluded.
    BlockedCannotBeApproved,
    /// The database update failed.
    Db(sqlx::Error),
}

impl From<sqlx::Error> for ApprovalError {
    fn from(e: sqlx::Error) -> Self {
        ApprovalError::Db(e)
    }
}

/// The pure AC-17 rule: given the op's validation state and the requested
/// action, the resulting approval state, or an error. The ONE hard rule: a
/// `blocked` op can never become `approved`.
pub fn next_approval(
    validation: ValidationState,
    action: ApprovalAction,
) -> Result<ApprovalState, ApprovalError> {
    match action {
        ApprovalAction::Approve => {
            if validation == ValidationState::Blocked {
                Err(ApprovalError::BlockedCannotBeApproved)
            } else {
                Ok(ApprovalState::Approved)
            }
        }
        ApprovalAction::Reject => Ok(ApprovalState::Rejected),
        ApprovalAction::Exclude => Ok(ApprovalState::Excluded),
        ApprovalAction::Reset => Ok(ApprovalState::Pending),
    }
}

/// Apply an approval transition to ONE op, enforcing AC-17. Reads the op's
/// `validation_state`, computes the next approval via [`next_approval`], and
/// (only if allowed) writes it via the scoped [`crate::db::plans::set_approval`]
/// (which touches only the mutable approval pair). Returns the new state.
pub async fn set_op_approval(
    pool: &sqlx::SqlitePool,
    plan_op_id: i64,
    action: ApprovalAction,
    now: &str,
) -> Result<ApprovalState, ApprovalError> {
    let state_str: String =
        sqlx::query_scalar("SELECT validation_state FROM plan_ops WHERE id = ?")
            .bind(plan_op_id)
            .fetch_one(pool)
            .await?;
    let validation = validation_state_from_str(&state_str);
    let next = next_approval(validation, action)?;
    crate::db::plans::set_approval(pool, plan_op_id, next.as_str(), now).await?;
    Ok(next)
}

/// Apply an approval transition to every op in one campaign group of a plan.
///
/// This is the F-405 per-GROUP approval (a campaign group is the unit of user
/// approval). For [`ApprovalAction::Approve`], blocked ops are SKIPPED (AC-17:
/// they cannot be approved), and every non-blocked op in the group is approved;
/// the count of ops actually transitioned is returned. Reject / Exclude / Reset
/// apply to every op in the group unconditionally.
pub async fn set_group_approval(
    pool: &sqlx::SqlitePool,
    plan_id: i64,
    group: CampaignGroup,
    action: ApprovalAction,
    now: &str,
) -> Result<usize, ApprovalError> {
    let ops = crate::db::plans::get_plan_ops(pool, plan_id).await?;
    let mut applied = 0usize;
    for op in ops {
        if group_for_op_group(&op.op_group) != Some(group) {
            continue;
        }
        let validation = validation_state_from_str(&op.validation_state);
        // Approving a group skips its blocked ops rather than failing the whole
        // group; other actions apply to all.
        if action == ApprovalAction::Approve && validation == ValidationState::Blocked {
            continue;
        }
        let next = next_approval(validation, action)?;
        crate::db::plans::set_approval(pool, op.id, next.as_str(), now).await?;
        applied += 1;
    }
    Ok(applied)
}

/// Parse a stored `validation_state` string; an unknown value is treated as
/// `Blocked` (fail safe: never let an unrecognized state be approved).
fn validation_state_from_str(s: &str) -> ValidationState {
    match s {
        "valid" => ValidationState::Valid,
        "warning" => ValidationState::Warning,
        _ => ValidationState::Blocked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::plan::builder::{GroupCount, PlanStats};

    fn op(kind: &str, group: &str, source: &str, target: &str) -> PlannedOp {
        PlannedOp {
            op_group: group.to_string(),
            kind: kind.to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test op.".to_string(),
            rule_id: "test".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            provenance_json: None,
        }
    }

    fn plan_of(ops: Vec<PlannedOp>) -> BuiltPlan {
        BuiltPlan {
            ops,
            stats: PlanStats {
                total_ops: 0,
                manual_review_ops: 0,
                per_group: vec![GroupCount {
                    group: "loose books".to_string(),
                    ops: 0,
                }],
            },
        }
    }

    fn existing(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn env<'a>(
        existing: &'a HashSet<String>,
        long_paths_enabled: bool,
        fs: &'a dyn FreeSpace,
    ) -> ValidationEnv<'a> {
        ValidationEnv::new(existing, long_paths_enabled, fs)
    }

    // ---- blocking Section 8 codes match AppError ----

    #[test]
    fn blocking_reason_codes_equal_the_apperror_plan_family() {
        // Each blocking ValidationReason's code must equal the matching AppError
        // Section 8 code, so a stored validation_reason and an apply-time error
        // speak one vocabulary.
        let pairs: &[(ValidationReason, AppError)] = &[
            (
                ValidationReason::SnapshotStale,
                AppError::SnapshotStale { path: "x".into() },
            ),
            (
                ValidationReason::CollisionInPlan,
                AppError::CollisionInPlan { path: "x".into() },
            ),
            (
                ValidationReason::CollisionOnDisk,
                AppError::CollisionOnDisk { path: "x".into() },
            ),
            (
                ValidationReason::PathTooLong,
                AppError::PathTooLong {
                    path: "x".into(),
                    length: 1,
                },
            ),
            (
                ValidationReason::IllegalComponent,
                AppError::IllegalComponent {
                    path: "x".into(),
                    component: "x".into(),
                },
            ),
            (
                ValidationReason::ReservedName,
                AppError::ReservedName {
                    path: "x".into(),
                    component: "x".into(),
                },
            ),
            (
                ValidationReason::CrossVolumeSpaceInsufficient,
                AppError::CrossVolumeSpaceInsufficient {
                    volume: "x".into(),
                    needed: 1,
                    available: 0,
                },
            ),
            (
                ValidationReason::CycleDetected,
                AppError::CycleDetected {
                    source_path: "x".into(),
                    target_path: "y".into(),
                },
            ),
        ];
        for (reason, err) in pairs {
            assert_eq!(reason.code(), err.code(), "code mismatch for {reason:?}");
        }
    }

    // ---- individual hazard checks ----

    #[test]
    fn clean_move_is_valid() {
        let ex = existing(&["E:/lib/Book.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "move",
            "loose-root-books",
            "E:/lib/Book.m4b",
            "E:/lib/Author/Title/Book.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].state, ValidationState::Valid);
    }

    #[test]
    fn snapshot_stale_source_is_blocked() {
        let ex = existing(&[]); // source not present
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "move",
            "loose-root-books",
            "E:/lib/Gone.m4b",
            "E:/lib/Author/Title/Gone.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].state, ValidationState::Blocked);
        assert_eq!(v[0].reason_code(), Some("snapshot-stale"));
    }

    #[test]
    fn in_plan_case_only_collision_blocks_both() {
        let ex = existing(&["E:/lib/a.m4b", "E:/lib/b.m4b"]);
        let fs = FixedFreeSpace::new();
        // Two moves whose targets differ only in case: an NTFS collision.
        let plan = plan_of(vec![
            op("move", "loose-root-books", "E:/lib/a.m4b", "E:/lib/Author/Title.m4b"),
            op("move", "loose-root-books", "E:/lib/b.m4b", "E:/lib/author/title.m4b"),
        ]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("collision-in-plan"));
        assert_eq!(v[1].reason_code(), Some("collision-in-plan"));
        assert!(v[0].is_blocked() && v[1].is_blocked());
    }

    #[test]
    fn on_disk_collision_is_blocked_unless_vacated() {
        let ex = existing(&["E:/lib/src.m4b", "E:/lib/dest/Book.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "move",
            "loose-root-books",
            "E:/lib/src.m4b",
            "E:/lib/dest/Book.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("collision-on-disk"));
    }

    #[test]
    fn on_disk_collision_is_allowed_when_target_is_vacated_by_the_plan() {
        // The target already exists, but another op moves it away first.
        let ex = existing(&["E:/lib/src.m4b", "E:/lib/dest.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![
            op("move", "loose-root-books", "E:/lib/dest.m4b", "E:/lib/elsewhere.m4b"),
            op("move", "loose-root-books", "E:/lib/src.m4b", "E:/lib/dest.m4b"),
        ]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[1].state, ValidationState::Valid, "target is vacated first");
    }

    #[test]
    fn cycle_move_into_own_subtree_is_blocked() {
        let ex = existing(&["E:/lib/Series"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "move",
            "flatten-packs",
            "E:/lib/Series",
            "E:/lib/Series/Sub",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("cycle-detected"));
    }

    #[test]
    fn reserved_device_name_is_blocked() {
        let ex = existing(&["E:/lib/x.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "move",
            "loose-root-books",
            "E:/lib/x.m4b",
            "E:/lib/CON/x.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("reserved-name"));
    }

    #[test]
    fn reserved_name_with_extension_is_blocked() {
        let ex = existing(&["E:/lib/x.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "rename",
            "strip-noise",
            "E:/lib/x.m4b",
            "E:/lib/COM1.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("reserved-name"));
    }

    #[test]
    fn illegal_character_component_is_blocked() {
        let ex = existing(&["E:/lib/x.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![op(
            "rename",
            "strip-noise",
            "E:/lib/x.m4b",
            "E:/lib/Bad?Name.m4b",
        )]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("illegal-component"));
    }

    #[test]
    fn trailing_dot_component_is_illegal() {
        assert!(is_illegal_component("Some Book."));
        assert!(is_illegal_component("Some Book "));
        assert!(!is_illegal_component("Some Book"));
    }

    #[test]
    fn path_too_long_beyond_extended_limit_is_blocked() {
        let ex = existing(&["E:/lib/x.m4b"]);
        let fs = FixedFreeSpace::new();
        let long_leaf = "a".repeat(EXTENDED_LENGTH_LIMIT + 10);
        let target = format!("E:/lib/{long_leaf}.m4b");
        let plan = plan_of(vec![op("move", "loose-root-books", "E:/lib/x.m4b", &target)]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("path-too-long"));
        assert!(v[0].is_blocked());
    }

    #[test]
    fn near_260_target_warns_and_over_260_disabled_carries_howto() {
        let ex = existing(&["E:/lib/x.m4b"]);
        let fs = FixedFreeSpace::new();

        // A 255-char target: near-limit interop warning regardless of setting.
        let near = format!("E:/{}", "a".repeat(255 - 3));
        assert_eq!(path_len(&near), 255);
        let plan = plan_of(vec![op("move", "loose-root-books", "E:/lib/x.m4b", &near)]);
        let v_enabled = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v_enabled[0].reason_code(), Some("path-length-near-260"));
        assert!(v_enabled[0].is_warning());

        // A 300-char target with long paths DISABLED: warning + how-to (AC-14).
        let over = format!("E:/{}", "a".repeat(300 - 3));
        assert_eq!(path_len(&over), 300);
        let plan2 = plan_of(vec![op("move", "loose-root-books", "E:/lib/x.m4b", &over)]);
        let v_disabled = validate_plan(&plan2, &env(&ex, false, &fs));
        assert_eq!(v_disabled[0].reason_code(), Some("long-paths-disabled"));
        assert!(v_disabled[0].how_to.is_some(), "carries the how-to link");

        // The same 300-char target with long paths ENABLED: interop note, no link.
        let v_enabled2 = validate_plan(&plan2, &env(&ex, true, &fs));
        assert_eq!(v_enabled2[0].reason_code(), Some("path-length-near-260"));
        assert!(v_enabled2[0].how_to.is_none());
    }

    #[test]
    fn cross_volume_fits_warns_but_over_budget_blocks() {
        let ex = existing(&["E:/lib/a.m4b", "E:/lib/b.m4b"]);
        // D: has room for a but not for a+b together.
        let plan_fits = {
            let mut o = op("move", "loose-root-books", "E:/lib/a.m4b", "D:/lib/Author/a.m4b");
            o.byte_size = 1_000;
            plan_of(vec![o])
        };
        let fs = FixedFreeSpace::new().with("D:", 5_000);
        let v = validate_plan(&plan_fits, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("cross-volume-copy"));
        assert!(v[0].is_warning());

        // Two cross-volume ops summing beyond D:'s free space -> both blocked.
        let plan_over = {
            let mut a = op("move", "loose-root-books", "E:/lib/a.m4b", "D:/lib/Author/a.m4b");
            a.byte_size = 4_000;
            let mut b = op("move", "loose-root-books", "E:/lib/b.m4b", "D:/lib/Author/b.m4b");
            b.byte_size = 4_000;
            plan_of(vec![a, b])
        };
        let v2 = validate_plan(&plan_over, &env(&ex, true, &fs));
        assert_eq!(v2[0].reason_code(), Some("cross-volume-space-insufficient"));
        assert_eq!(v2[1].reason_code(), Some("cross-volume-space-insufficient"));
    }

    #[test]
    fn cross_volume_with_unknown_free_space_warns_never_blocks() {
        let ex = existing(&["E:/lib/a.m4b"]);
        let fs = FixedFreeSpace::new(); // D: unknown
        let mut o = op("move", "loose-root-books", "E:/lib/a.m4b", "D:/lib/a.m4b");
        o.byte_size = 999_999_999;
        let plan = plan_of(vec![o]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].reason_code(), Some("cross-volume-copy"));
    }

    #[test]
    fn no_op_is_always_valid() {
        let ex = existing(&[]);
        let fs = FixedFreeSpace::new();
        let mut o = op("no-op", "staging-separation", "E:/lib/_sort", "");
        o.kind_reason = Some("staging".to_string());
        let plan = plan_of(vec![o]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[0].state, ValidationState::Valid);
    }

    #[test]
    fn pack_shell_quarantine_is_blocked_when_a_member_is_blocked() {
        // A member move whose source vanished (stale) blocks; the shell's
        // quarantine must then also block (it would strand that book).
        let ex = existing(&["E:/lib/Pack", "E:/lib/Pack/Present"]); // "Blocked" member absent
        let fs = FixedFreeSpace::new();
        let mut member_present =
            op("move", "flatten-packs", "E:/lib/Pack/Present", "E:/lib/A/Present");
        member_present.byte_size = 10;
        let member_gone = op("move", "flatten-packs", "E:/lib/Pack/Gone", "E:/lib/B/Gone");
        let mut shell = op("quarantine", "empty-cleanup", "E:/lib/Pack", "E:/lib/_Quarantine/Pack");
        shell.rule_id = "flatten-packs-shell".to_string();
        let plan = plan_of(vec![member_present, member_gone, shell]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        assert_eq!(v[1].reason_code(), Some("snapshot-stale"), "the gone member");
        assert_eq!(
            v[2].reason_code(),
            Some("pack-member-blocked"),
            "the shell must not quarantine over a stranded book"
        );
    }

    #[test]
    fn summary_counts_states() {
        let ex = existing(&["E:/lib/ok.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![
            op("move", "loose-root-books", "E:/lib/ok.m4b", "E:/lib/A/ok.m4b"),
            op("move", "loose-root-books", "E:/lib/gone.m4b", "E:/lib/A/gone.m4b"),
        ]);
        let v = validate_plan(&plan, &env(&ex, true, &fs));
        let s = summarize(&v);
        assert_eq!(s.valid, 1);
        assert_eq!(s.blocked, 1);
        assert!(s.has_blocked());
    }

    // ---- volume / helper unit tests ----

    #[test]
    fn volume_extraction() {
        assert_eq!(volume_of("E:/lib/x"), Some("E:".to_string()));
        assert_eq!(volume_of(r"e:\lib\x"), Some("E:".to_string()));
        assert_eq!(volume_of(r"\\server\share\x"), Some("//server/share".to_string()));
        assert_eq!(volume_of("relative/path"), None);
    }

    #[test]
    fn reserved_and_illegal_helpers() {
        assert!(is_reserved_component("CON"));
        assert!(is_reserved_component("com1.m4b"));
        assert!(is_reserved_component("NUL"));
        assert!(!is_reserved_component("CONSOLE"));
        assert!(!is_reserved_component("Console Wars"));
    }

    // ---- approval state machine (AC-17) ----

    #[test]
    fn blocked_op_cannot_be_approved() {
        assert!(matches!(
            next_approval(ValidationState::Blocked, ApprovalAction::Approve),
            Err(ApprovalError::BlockedCannotBeApproved)
        ));
        // But it CAN be rejected, excluded, or reset.
        assert_eq!(
            next_approval(ValidationState::Blocked, ApprovalAction::Exclude).unwrap(),
            ApprovalState::Excluded
        );
        assert_eq!(
            next_approval(ValidationState::Blocked, ApprovalAction::Reject).unwrap(),
            ApprovalState::Rejected
        );
        assert_eq!(
            next_approval(ValidationState::Blocked, ApprovalAction::Reset).unwrap(),
            ApprovalState::Pending
        );
    }

    #[test]
    fn valid_and_warning_ops_can_be_approved() {
        assert_eq!(
            next_approval(ValidationState::Valid, ApprovalAction::Approve).unwrap(),
            ApprovalState::Approved
        );
        assert_eq!(
            next_approval(ValidationState::Warning, ApprovalAction::Approve).unwrap(),
            ApprovalState::Approved
        );
    }

    #[test]
    fn approval_state_round_trips_through_string() {
        for s in [
            ApprovalState::Pending,
            ApprovalState::Approved,
            ApprovalState::Rejected,
            ApprovalState::Excluded,
        ] {
            assert_eq!(ApprovalState::from_str(s.as_str()), Some(s));
        }
    }

    // ---- approval DB helpers ----

    async fn seed_plan_with_verdicts() -> (tempfile::TempDir, sqlx::SqlitePool, i64) {
        use crate::db::open_db;
        use crate::db::rulesets::{insert_ruleset, NewRuleset};

        let dir = tempfile::TempDir::new().unwrap();
        let (pool, _) = open_db(dir.path()).await.unwrap();
        let scan_id = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) \
             VALUES ('live', 'E:/lib', '2026-07-04T00:00:00Z', 'completed')",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        let ruleset_id = insert_ruleset(
            &pool,
            &NewRuleset {
                name: "Default",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();

        // One valid loose-books move and one blocked (stale) loose-books move.
        let ex = existing(&["E:/lib/ok.m4b"]);
        let fs = FixedFreeSpace::new();
        let plan = plan_of(vec![
            op("move", "loose-root-books", "E:/lib/ok.m4b", "E:/lib/A/ok.m4b"),
            op("move", "loose-root-books", "E:/lib/gone.m4b", "E:/lib/A/gone.m4b"),
        ]);
        let verdicts = validate_plan(&plan, &env(&ex, true, &fs));
        let plan_id = persist_validated_plan(
            &pool, scan_id, ruleset_id, "draft", &plan, &verdicts, "2026-07-04T00:00:00Z",
        )
        .await
        .unwrap();
        (dir, pool, plan_id)
    }

    #[tokio::test]
    async fn persist_writes_real_verdicts_not_pending() {
        let (_d, pool, plan_id) = seed_plan_with_verdicts().await;
        let ops = crate::db::plans::get_plan_ops(&pool, plan_id).await.unwrap();
        assert_eq!(ops[0].validation_state, "valid");
        assert_eq!(ops[1].validation_state, "blocked");
        assert_eq!(ops[1].validation_reason.as_deref(), Some("snapshot-stale"));
    }

    #[tokio::test]
    async fn set_op_approval_refuses_to_approve_a_blocked_op() {
        let (_d, pool, plan_id) = seed_plan_with_verdicts().await;
        let ops = crate::db::plans::get_plan_ops(&pool, plan_id).await.unwrap();
        let blocked_id = ops.iter().find(|o| o.validation_state == "blocked").unwrap().id;

        let result =
            set_op_approval(&pool, blocked_id, ApprovalAction::Approve, "2026-07-04T01:00:00Z")
                .await;
        assert!(matches!(
            result,
            Err(ApprovalError::BlockedCannotBeApproved)
        ));
        // The stored approval is unchanged (still pending).
        let after = crate::db::plans::get_plan_ops(&pool, plan_id).await.unwrap();
        let blocked = after.iter().find(|o| o.id == blocked_id).unwrap();
        assert_eq!(blocked.approval, "pending");

        // Excluding it IS allowed.
        let excluded =
            set_op_approval(&pool, blocked_id, ApprovalAction::Exclude, "2026-07-04T01:00:00Z")
                .await
                .unwrap();
        assert_eq!(excluded, ApprovalState::Excluded);
    }

    #[tokio::test]
    async fn set_group_approval_skips_blocked_and_approves_the_rest() {
        let (_d, pool, plan_id) = seed_plan_with_verdicts().await;
        // Approve the whole "loose books" group: the valid op approves, the
        // blocked op is skipped (AC-17), so exactly one op transitions.
        let applied = set_group_approval(
            &pool,
            plan_id,
            CampaignGroup::LooseBooks,
            ApprovalAction::Approve,
            "2026-07-04T02:00:00Z",
        )
        .await
        .unwrap();
        assert_eq!(applied, 1, "only the non-blocked op is approved");

        let ops = crate::db::plans::get_plan_ops(&pool, plan_id).await.unwrap();
        let valid_op = ops.iter().find(|o| o.validation_state == "valid").unwrap();
        let blocked_op = ops.iter().find(|o| o.validation_state == "blocked").unwrap();
        assert_eq!(valid_op.approval, "approved");
        assert_eq!(blocked_op.approval, "pending", "blocked op stays pending");
    }
}
