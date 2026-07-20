//! F-602 (undo manifest): the self-contained, reverse-executable undo file.
//!
//! On a completed apply the executor exports a JSON manifest - the user-facing
//! "undo file" - into the Reports folder (AC-11). It is SELF-CONTAINED: every
//! operation's id, source, target, kind, order, and F-507 provenance live in the
//! file itself, so a later undo (v0.6.0) can reconstruct the reverse operation
//! list from the JSON ALONE, without the app database being present or healthy.
//!
//! # Plain-language register
//!
//! "Manifest" and "journal" are engine terms. The exported file's base name is
//! [`MANIFEST_JSON_BASENAME`] (`undo-file.json`), and any user surface calls it an
//! "undo file", never a manifest.
//!
//! # Schema versioning (OQ-1, resolved)
//!
//! The JSON carries [`MANIFEST_SCHEMA_VERSION`] as its first field. A reader
//! rejects a HIGHER version with a clear [`ManifestError::SchemaTooNew`] rather
//! than silently misreading a newer shape, so an undo file written by a future
//! build is never half-understood by an older one.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::db::plans::PlanOpRow;
use crate::error::AppError;

use super::{ApplyMode, APPROVED};

/// The serde default for a manifest's `mode` when a legacy undo file carries no
/// `mode` field. SAFETY CHOICE: an unmarked file defaults to
/// [`ApplyMode::DryRun`], the conservative reading. A dry-run manifest's
/// [`Manifest::reverse_ops`] is REFUSED, so treating an ambiguous file as a
/// rehearsal guarantees a reconciliation flow can never offer to undo moves that
/// never happened. The opposite default ("real") could turn an unmarked dry-run
/// file into a false undo of nonexistent moves. No undo file predates this field
/// (migration 0005 is the first to create manifests), so the default only ever
/// fires defensively, and it fails safe.
fn default_mode() -> ApplyMode {
    ApplyMode::DryRun
}

/// The manifest JSON schema version (OQ-1). Bumped only on a breaking shape
/// change; a reader rejects anything higher than the version it was built with.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// The stable base name for the exported undo file. Plain-language register:
/// "undo file", never "manifest".
pub const MANIFEST_JSON_BASENAME: &str = "undo-file.json";

/// One operation in the manifest, in the shape an undo needs and no more: the
/// op's identity, its order, what it did, and where from/to, plus the F-507
/// provenance carried verbatim so nothing about a book's origin is lost (AC-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestOp {
    /// The `plan_ops.id` this operation is.
    pub op_id: i64,
    /// The operation's position in the plan (`plan_ops.seq`): forward walk order,
    /// and the order an undo reverses.
    pub seq: i64,
    /// What the operation did (`move`/`rename`/`mkdir`/...).
    pub kind: String,
    /// Where the operation moved FROM (the undo target).
    pub source_path: String,
    /// Where the operation moved TO (the undo source, the current location).
    pub target_path: String,
    /// The op's F-507 pack/award provenance JSON, carried VERBATIM from plan time
    /// (`pack_path`, `pack_name`, optional `award_marker`), so the undo file alone
    /// records where an extracted book came from. `None` for non-pack ops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_json: Option<String>,
    /// For a `quarantine` (set-aside) op, WHY the item was set aside (the op's
    /// plain-language rationale, which encodes duplicate-of / non-preferred-format
    /// / clutter). Together with `source_path` (the original location) and
    /// `target_path` (the `<set-aside-root>\<job-id>\...` destination), this makes
    /// the reason and the original path recoverable from the undo file alone
    /// (AC-22). `None` for every non-set-aside op. Additive and optional, so an
    /// older reader ignores it and an undo file without it still parses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_aside_reason: Option<String>,
}

/// The whole undo file: a schema version, the job/plan identity, whether every op
/// is reversible, and the ordered operation list. Self-contained by construction
/// (AC-11) - recovery reads this and needs nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The schema version (OQ-1); a reader rejects a higher value.
    pub manifest_schema_version: u32,
    /// Whether this records a `dry-run` rehearsal or a `real` apply. A dry-run
    /// manifest describes moves that never happened, so [`Manifest::reverse_ops`]
    /// refuses to produce an undo for it. Serde-defaulted (see [`default_mode`])
    /// so a future field addition or a legacy file reads safely without a schema
    /// version bump.
    #[serde(default = "default_mode")]
    pub mode: ApplyMode,
    /// The apply `jobs.id` this manifest belongs to.
    pub job_id: i64,
    /// The `plans.id` that was applied.
    pub plan_id: i64,
    /// True when every operation in this manifest can be reversed (FD-10).
    pub reversible: bool,
    /// The applied operations, in plan (`seq`) order.
    pub ops: Vec<ManifestOp>,
}

/// One reverse operation, reconstructed from the manifest without the database
/// (AC-11): move `from` (where the op left the book) back `to` (where it started).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverseOp {
    /// The `plan_ops.id` of the forward operation being reversed.
    pub op_id: i64,
    /// The forward op's kind (so an undo knows what it is reversing).
    pub kind: String,
    /// Where the item is NOW (the forward op's target).
    pub from: String,
    /// Where to put it back (the forward op's source).
    pub to: String,
}

/// A failure reading an undo file back (the v0.6.0 undo path and the round-trip
/// test). Deliberately NOT an [`AppError`]: reading an undo file is not an IPC
/// surface this phase, and a self-contained recovery tool wants a precise,
/// self-describing error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    /// The JSON could not be parsed into a manifest.
    #[error("undo file could not be read: {0}")]
    Parse(String),
    /// The undo file was written by a newer build than this reader understands.
    #[error(
        "undo file was written by a newer version of the app (undo-file version {found}; \
         this app understands up to version {supported}); update the app to read it"
    )]
    SchemaTooNew { found: u32, supported: u32 },
    /// The undo file records a dry-run rehearsal, not a real apply: no file
    /// actually moved, so there is nothing to reverse. Refusing here is the
    /// safe semantic - a reconciliation flow physically cannot enumerate reverse
    /// moves for a rehearsal, so it can never move a file based on one.
    #[error(
        "this undo file records a dry-run rehearsal, not a real apply; there is nothing to undo"
    )]
    DryRunNotReversible,
}

/// The kinds an undo can reverse. A `move`/`rename` reverses by moving back; a
/// `mkdir` reverses by removing the created (empty) directory; a `quarantine`
/// (set-aside) reverses by moving the item back; an `rmdir-empty` reverses by
/// recreating the folder; a `no-op` did nothing to reverse. FD-10 guarantees no
/// audiobook is ever deleted, so the current op set is fully reversible; an unknown
/// future kind flips [`Manifest::reversible`] to `false` honestly rather than
/// claiming an undo it cannot perform.
///
/// The accepted list is EXACTLY [`crate::exec::DISPATCH_OP_KINDS`], the kinds the
/// executor's dispatch understands (which is exactly what the plan builder emits).
/// A test pins the two lists together so they can never drift. NOTE: `quarantine`
/// is the STORED op kind; `set-aside` is only its plain-language DISPLAY term (the
/// F-505 export scrub), never a value in `plan_ops.kind`, so it is deliberately not
/// listed here - the manifest reads raw stored kinds.
fn is_reversible_kind(kind: &str) -> bool {
    matches!(
        kind,
        "move" | "rename" | "mkdir" | "quarantine" | "rmdir-empty" | "no-op"
    )
}

/// Build the manifest for a completed apply from the plan's operation rows. Only
/// APPROVED operations are included - they are exactly the ones the executor
/// walked, so the undo file reverses precisely what was done and nothing else.
/// `mode` records whether this was a `dry-run` rehearsal or a `real` apply.
pub fn build_manifest(mode: ApplyMode, job_id: i64, plan_id: i64, ops: &[PlanOpRow]) -> Manifest {
    let ops: Vec<ManifestOp> = ops
        .iter()
        .filter(|o| o.approval == APPROVED)
        .map(|o| ManifestOp {
            op_id: o.id,
            seq: o.seq,
            kind: o.kind.clone(),
            source_path: o.source_path.clone(),
            target_path: o.target_path.clone(),
            provenance_json: o.provenance_json.clone(),
            // AC-22: the set-aside reason rides the undo file for a set-aside op
            // (the reason is the op's plain-language rationale); `None` otherwise.
            set_aside_reason: (o.kind == "quarantine").then(|| o.rationale.clone()),
        })
        .collect();
    let reversible = ops.iter().all(|o| is_reversible_kind(&o.kind));
    Manifest {
        manifest_schema_version: MANIFEST_SCHEMA_VERSION,
        mode,
        job_id,
        plan_id,
        reversible,
        ops,
    }
}

impl Manifest {
    /// Serialize to pretty JSON (the exported undo file). Deterministic: field
    /// order is fixed and the op list is already in `seq` order.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a Manifest always serializes to JSON")
    }

    /// Read an undo file back from its JSON, WITHOUT the app database (AC-11). The
    /// schema version is checked FIRST: a higher version is rejected with
    /// [`ManifestError::SchemaTooNew`] before any field is trusted, so a newer
    /// shape is never half-parsed.
    pub fn from_json(s: &str) -> Result<Self, ManifestError> {
        // Read only the version first, so a future shape is rejected cleanly
        // rather than failing deeper with a confusing field error.
        let value: serde_json::Value =
            serde_json::from_str(s).map_err(|e| ManifestError::Parse(e.to_string()))?;
        let found = value
            .get("manifest_schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ManifestError::Parse("missing manifest_schema_version".to_string()))?
            as u32;
        if found > MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::SchemaTooNew {
                found,
                supported: MANIFEST_SCHEMA_VERSION,
            });
        }
        serde_json::from_value(value).map_err(|e| ManifestError::Parse(e.to_string()))
    }

    /// Reconstruct the reverse operation list from the manifest alone (AC-11): the
    /// ops in REVERSE `seq` order, each moving its target back to its source. No
    /// database read - everything needed is in the manifest.
    ///
    /// REFUSES a dry-run manifest with [`ManifestError::DryRunNotReversible`]: a
    /// rehearsal moved nothing, so there is nothing to reverse, and refusing means
    /// no undo flow can ever act on a rehearsal. Only a `real` apply reverses.
    pub fn reverse_ops(&self) -> Result<Vec<ReverseOp>, ManifestError> {
        if self.mode == ApplyMode::DryRun {
            return Err(ManifestError::DryRunNotReversible);
        }
        let mut ops = self.ops.clone();
        ops.sort_by_key(|o| std::cmp::Reverse(o.seq));
        Ok(ops
            .into_iter()
            .map(|o| ReverseOp {
                op_id: o.op_id,
                kind: o.kind,
                from: o.target_path,
                to: o.source_path,
            })
            .collect())
    }
}

/// Write the undo file JSON into `dir` (creating it and its ancestors if missing)
/// under [`MANIFEST_JSON_BASENAME`], returning its path. Cross-platform `std::fs`,
/// like [`crate::reports`]; it writes only into the Reports folder, never the
/// library.
pub fn write_manifest_json(dir: &Path, manifest: &Manifest) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(MANIFEST_JSON_BASENAME);
    std::fs::write(&path, manifest.to_json())?;
    Ok(path)
}

/// Append the manifest's index row to the `manifests` table (migration 0005).
/// Append-only: this is an `INSERT`, never an update. `mode` records dry-run vs
/// real so a DB-side reader distinguishes a rehearsal without opening the file.
/// Returns the new row id.
pub async fn insert_manifest_row(
    pool: &SqlitePool,
    mode: ApplyMode,
    job_id: i64,
    plan_id: i64,
    json_path: &str,
    reversible: bool,
) -> Result<i64, AppError> {
    let result = sqlx::query(
        "INSERT INTO manifests (job_id, plan_id, json_path, reversible, mode) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(job_id)
    .bind(plan_id)
    .bind(json_path)
    .bind(reversible as i64)
    .bind(mode.as_str())
    .execute(pool)
    .await
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not record the undo file: {e}"),
    })?;
    Ok(result.last_insert_rowid())
}

/// One `manifests` index row (migration 0005), read back so the undo path
/// (F-604) can find the exported undo file and the plan/job it belongs to. The
/// self-contained recovery data lives in the JSON at `json_path`, not here; this
/// row is only the pointer plus the dry-run/real marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestRow {
    pub id: i64,
    pub job_id: i64,
    pub plan_id: i64,
    pub json_path: String,
    pub reversible: bool,
    pub mode: String,
}

/// Read one `manifests` index row by id, or `None` if it does not exist. Used by
/// the undo path ([`crate::exec::rollback`]) to locate the exported undo file for
/// a completed apply. A read-only `SELECT`; the append-only contract is untouched.
pub async fn get_manifest_row(
    pool: &SqlitePool,
    manifest_id: i64,
) -> Result<Option<ManifestRow>, AppError> {
    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT id, job_id, plan_id, json_path, reversible, mode FROM manifests WHERE id = ?",
    )
    .bind(manifest_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::RollbackPrepareFailed {
        detail: format!("could not read the undo file record: {e}"),
    })?;
    Ok(row.map(|r| ManifestRow {
        id: r.get("id"),
        job_id: r.get("job_id"),
        plan_id: r.get("plan_id"),
        json_path: r.get("json_path"),
        reversible: r.get::<i64, _>("reversible") != 0,
        mode: r.get("mode"),
    }))
}

/// What [`export_after_apply`] produced, so a caller (or a test) can find every
/// artifact without re-deriving paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestExport {
    /// The `manifests.id` row inserted.
    pub manifest_id: i64,
    /// The exported undo-file JSON path.
    pub json_path: PathBuf,
    /// The re-emitted provenance report paths (JSON, Markdown).
    pub provenance_json: PathBuf,
    pub provenance_markdown: PathBuf,
}

/// Export a completed apply's artifacts into `reports_dir` (AC-11, AC-12): write
/// the self-contained undo file, record its index row, and re-emit the F-507
/// provenance report reflecting the applied (final) locations. `mode` marks the
/// undo file and the index row as a dry-run rehearsal or a real apply.
///
/// The re-emit is scoped to the APPROVED (walked) subset of `ops`, matching the
/// undo file: only the ops that were actually applied have a "final location", so
/// the post-apply report reflects exactly what moved. It is written under the
/// [`crate::plan::provenance::PROVENANCE_AFTER_APPLY_JSON_BASENAME`] names so it
/// sits BESIDE the plan-time provenance report rather than overwriting it, keeping
/// plan-time vs post-apply separately auditable.
pub async fn export_after_apply(
    pool: &SqlitePool,
    reports_dir: &Path,
    mode: ApplyMode,
    job_id: i64,
    plan_id: i64,
    ops: &[PlanOpRow],
) -> Result<ManifestExport, AppError> {
    let manifest = build_manifest(mode, job_id, plan_id, ops);
    let json_path =
        write_manifest_json(reports_dir, &manifest).map_err(|e| AppError::ApplyFailed {
            detail: format!("could not write the undo file: {e}"),
        })?;
    let manifest_id = insert_manifest_row(
        pool,
        mode,
        job_id,
        plan_id,
        &json_path.to_string_lossy(),
        manifest.reversible,
    )
    .await?;

    // Re-emit the F-507 provenance report over the APPROVED (walked) ops only, so
    // it reflects the final locations of exactly what was applied (matching the
    // undo file). Reuses the v0.3.0 generator over the persisted rows.
    let approved: Vec<PlanOpRow> = ops
        .iter()
        .filter(|o| o.approval == APPROVED)
        .cloned()
        .collect();
    let provenance = crate::plan::provenance::build_provenance_report_from_rows(&approved);
    let (provenance_json, provenance_markdown) = crate::reports::write_provenance_report(
        reports_dir,
        &provenance,
        crate::plan::provenance::PROVENANCE_AFTER_APPLY_JSON_BASENAME,
        crate::plan::provenance::PROVENANCE_AFTER_APPLY_MARKDOWN_BASENAME,
    )
    .map_err(|e| AppError::ApplyFailed {
        detail: format!("could not re-emit the provenance report: {e}"),
    })?;

    Ok(ManifestExport {
        manifest_id,
        json_path,
        provenance_json,
        provenance_markdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_db;
    use tempfile::TempDir;

    fn op(
        id: i64,
        seq: i64,
        kind: &str,
        source: &str,
        target: &str,
        prov: Option<&str>,
    ) -> PlanOpRow {
        PlanOpRow {
            id,
            plan_id: 1,
            seq,
            op_group: "flatten-packs".to_string(),
            kind: kind.to_string(),
            kind_reason: None,
            source_path: source.to_string(),
            target_path: target.to_string(),
            rationale: "test.".to_string(),
            rule_id: "flatten-packs".to_string(),
            confidence: "high".to_string(),
            byte_size: 0,
            validation_state: "valid".to_string(),
            validation_reason: None,
            provenance_json: prov.map(|p| p.to_string()),
            approval: "approved".to_string(),
            approval_updated_at: None,
        }
    }

    /// AC-11: a manifest read back from its JSON WITHOUT the app database
    /// reconstructs the reverse operation list. The manifest is built, serialized,
    /// dropped, and reparsed - the reverse ops come only from the JSON.
    #[test]
    fn manifest_round_trips_and_reconstructs_reverse_ops_without_the_db() {
        let ops = vec![
            op(
                10,
                0,
                "move",
                "E:\\Books\\Hugo Pack\\The Fifth Season",
                "E:\\Books\\N.K. Jemisin\\The Fifth Season",
                Some(
                    r#"{"pack_path":"E:\\Books\\Hugo Pack","pack_name":"Hugo Pack","award_marker":"^"}"#,
                ),
            ),
            op(
                11,
                1,
                "move",
                "E:\\Books\\Hugo Pack\\Neptune's Brood",
                "E:\\Books\\Charles Stross\\Neptune's Brood",
                Some(r#"{"pack_path":"E:\\Books\\Hugo Pack","pack_name":"Hugo Pack"}"#),
            ),
        ];
        // A REAL apply is the one an undo reverses.
        let manifest = build_manifest(ApplyMode::Real, 42, 1, &ops);
        assert_eq!(manifest.manifest_schema_version, MANIFEST_SCHEMA_VERSION);
        assert_eq!(manifest.mode, ApplyMode::Real);
        assert!(manifest.reversible, "two moves are reversible");
        assert_eq!(manifest.ops.len(), 2);

        // Serialize, then reparse from the STRING alone (no db, no source rows).
        let json = manifest.to_json();
        let reread = Manifest::from_json(&json).expect("round-trip parse");
        assert_eq!(reread, manifest, "the manifest round-trips losslessly");

        // AC-12: provenance rides each op verbatim through the round trip.
        assert_eq!(
            reread.ops[0].provenance_json.as_deref(),
            Some(
                r#"{"pack_path":"E:\\Books\\Hugo Pack","pack_name":"Hugo Pack","award_marker":"^"}"#
            )
        );

        // The reverse ops come purely from the reparsed manifest: reverse seq
        // order, target -> source.
        let reverse = reread.reverse_ops().expect("a real apply reverses");
        assert_eq!(reverse.len(), 2);
        assert_eq!(reverse[0].op_id, 11, "reverse walks highest seq first");
        assert_eq!(
            reverse[0].from,
            "E:\\Books\\Charles Stross\\Neptune's Brood"
        );
        assert_eq!(reverse[0].to, "E:\\Books\\Hugo Pack\\Neptune's Brood");
        assert_eq!(reverse[1].op_id, 10);
        assert_eq!(reverse[1].from, "E:\\Books\\N.K. Jemisin\\The Fifth Season");
        assert_eq!(reverse[1].to, "E:\\Books\\Hugo Pack\\The Fifth Season");
    }

    /// The manifest is self-contained: only the approved (walked) ops are in it,
    /// and every field an undo needs is present.
    #[test]
    fn build_manifest_includes_only_approved_ops() {
        let mut pending = op(2, 1, "move", "E:\\a", "E:\\b", None);
        pending.approval = "pending".to_string();
        let ops = vec![op(1, 0, "move", "E:\\x", "E:\\y", None), pending];

        let manifest = build_manifest(ApplyMode::Real, 7, 3, &ops);
        assert_eq!(
            manifest.ops.len(),
            1,
            "only the approved op is in the undo file"
        );
        assert_eq!(manifest.ops[0].op_id, 1);
    }

    /// AC-22: a set-aside (`quarantine`) op in the undo file carries the reason it
    /// was set aside AND its original path (source_path), so the reason and the
    /// original relative path are recoverable from the undo file alone. A rollback
    /// of a real apply then restores it to `source_path` (its original location).
    #[test]
    fn set_aside_op_records_reason_and_original_path_in_the_undo_file() {
        let mut set_aside = op(
            5,
            0,
            "quarantine",
            "E:\\Books - Audio\\Some Book\\track01.mp3",
            "E:\\Set Aside\\42\\Some Book\\track01.mp3",
            None,
        );
        set_aside.rationale =
            "This book keeps a preferred m4b copy, so the extra \"track01.mp3\" copy is set aside (never deleted).".to_string();
        // A non-set-aside op alongside carries no set-aside reason.
        let move_op = op(
            6,
            1,
            "move",
            "E:\\Books - Audio\\A.m4b",
            "E:\\Books - Audio\\Author\\A.m4b",
            None,
        );

        let manifest = build_manifest(ApplyMode::Real, 42, 1, &[set_aside, move_op]);
        // Round-trip through the JSON alone (the undo file is self-contained).
        let reread = Manifest::from_json(&manifest.to_json()).expect("round-trip");
        let sa = reread
            .ops
            .iter()
            .find(|o| o.kind == "quarantine")
            .expect("the set-aside op is in the undo file");
        assert_eq!(
            sa.set_aside_reason.as_deref(),
            Some(
                "This book keeps a preferred m4b copy, so the extra \"track01.mp3\" copy is set aside (never deleted)."
            ),
            "the reason is recoverable from the undo file"
        );
        assert_eq!(
            sa.source_path, "E:\\Books - Audio\\Some Book\\track01.mp3",
            "the original path is recoverable (relative path = source minus library root)"
        );
        assert!(
            sa.target_path.starts_with("E:\\Set Aside\\42\\"),
            "the set-aside location carries the job id (FD-34)"
        );
        // A non-set-aside op carries no set-aside reason.
        let mv = reread.ops.iter().find(|o| o.kind == "move").unwrap();
        assert_eq!(mv.set_aside_reason, None);
    }

    /// A dry-run manifest records a rehearsal (no file moved), so `reverse_ops`
    /// REFUSES rather than offering to undo moves that never happened - the safe
    /// semantic. Its `mode` still round-trips through the JSON so a reader can see
    /// what it was.
    #[test]
    fn a_dry_run_manifest_refuses_to_reverse() {
        let ops = vec![op(1, 0, "move", "E:\\x", "E:\\y", None)];
        let manifest = build_manifest(ApplyMode::DryRun, 7, 3, &ops);
        assert_eq!(manifest.mode, ApplyMode::DryRun);

        let reread = Manifest::from_json(&manifest.to_json()).expect("round-trip");
        assert_eq!(
            reread.mode,
            ApplyMode::DryRun,
            "the dry-run marker survives"
        );
        assert_eq!(
            reread.reverse_ops(),
            Err(ManifestError::DryRunNotReversible),
            "a rehearsal must never yield reverse ops"
        );
    }

    /// The mode marker is serialized in the undo file JSON as the kebab-case tag,
    /// and a legacy file with NO mode field defaults to `dry-run` (the fail-safe
    /// reading, so an unmarked file never yields a false undo offer).
    #[test]
    fn mode_serializes_and_an_unmarked_file_defaults_to_dry_run() {
        let json = build_manifest(ApplyMode::DryRun, 1, 1, &[]).to_json();
        assert!(json.contains("\"mode\": \"dry-run\""), "json: {json}");

        // A legacy undo file predating the mode field reads back as dry-run.
        let legacy = r#"{"manifest_schema_version": 1, "job_id": 1, "plan_id": 1, "reversible": true, "ops": []}"#;
        let parsed = Manifest::from_json(legacy).expect("legacy parse");
        assert_eq!(
            parsed.mode,
            ApplyMode::DryRun,
            "unmarked defaults to dry-run"
        );
        assert_eq!(
            parsed.reverse_ops(),
            Err(ManifestError::DryRunNotReversible),
            "an unmarked legacy file fails safe: no reverse ops"
        );
    }

    /// P2 obligation: the manifest's reversible-kind list is pinned to the
    /// executor's dispatch op-kind vocabulary ([`crate::exec::DISPATCH_OP_KINDS`]),
    /// so the two can never drift. Every kind the executor dispatches is classified
    /// reversible-or-not here (all six current kinds are reversible, FD-10), and
    /// nothing outside that set is silently accepted as reversible - the display-only
    /// `set-aside` term is proven NOT to be a stored kind.
    #[test]
    fn reversible_kinds_are_pinned_to_the_dispatch_op_kinds() {
        // Every op kind the executor dispatches is classified here (all reversible).
        for kind in crate::exec::DISPATCH_OP_KINDS {
            assert!(
                is_reversible_kind(kind),
                "dispatch kind '{kind}' must be classified by is_reversible_kind"
            );
        }
        // The reversible set is EXACTLY the dispatch set (no extras), so a kind the
        // dispatch does not emit is never claimed reversible.
        let extras = ["set-aside", "copy", "delete", "hardlink", ""];
        for kind in extras {
            assert!(
                !crate::exec::DISPATCH_OP_KINDS.contains(&kind),
                "'{kind}' is not a dispatch op kind"
            );
            assert!(
                !is_reversible_kind(kind),
                "'{kind}' is not a stored op kind and must not be reversible \
                 (set-aside is display-only, never a plan_ops.kind)"
            );
        }
    }

    /// OQ-1: a reader rejects an undo file whose schema version is higher than it
    /// understands, with a clear, self-describing error.
    #[test]
    fn a_higher_schema_version_is_rejected_with_a_clear_error() {
        let future = format!(
            r#"{{"manifest_schema_version": {}, "job_id": 1, "plan_id": 1, "reversible": true, "ops": []}}"#,
            MANIFEST_SCHEMA_VERSION + 1
        );
        let err = Manifest::from_json(&future).expect_err("a newer version must be rejected");
        assert_eq!(
            err,
            ManifestError::SchemaTooNew {
                found: MANIFEST_SCHEMA_VERSION + 1,
                supported: MANIFEST_SCHEMA_VERSION,
            }
        );
        // The message names the mismatch without leaking a raw code or path.
        assert!(err.to_string().contains("newer version"));
    }

    /// The manifest export lands the undo file and re-emits the provenance report
    /// into the Reports folder, and records the index row (AC-11, AC-12).
    #[tokio::test]
    async fn export_after_apply_writes_undo_file_and_reemits_provenance() {
        let db = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(db.path()).await.expect("open_db");
        // Foreign keys for the manifests row: a jobs row and a plans row.
        let job =
            sqlx::query("INSERT INTO jobs (kind, state, started_at) VALUES ('apply','running',?)")
                .bind("2026-07-18T00:00:00Z")
                .execute(&pool)
                .await
                .expect("jobs row")
                .last_insert_rowid();
        // A scan + ruleset + plan so the plan_id FK resolves.
        let scan = sqlx::query(
            "INSERT INTO scans (source, root_path, started_at, status) VALUES ('live','E:\\Books','2026-07-18T00:00:00Z','completed')",
        )
        .execute(&pool)
        .await
        .expect("scan")
        .last_insert_rowid();
        let ruleset = crate::db::rulesets::insert_ruleset(
            &pool,
            &crate::db::rulesets::NewRuleset {
                name: "d",
                body_json: "{}",
                schema_version: 1,
            },
            "2026-07-18T00:00:00Z",
        )
        .await
        .expect("ruleset");
        let plan = crate::db::plans::insert_plan(
            &pool,
            &crate::db::plans::NewPlan {
                scan_id: scan,
                ruleset_id: ruleset,
                status: "draft",
                stats_json: None,
            },
            &[],
            "2026-07-18T00:00:00Z",
        )
        .await
        .expect("plan");

        let reports = TempDir::new().expect("reports tempdir");
        let ops = vec![op(
            1,
            0,
            "move",
            "E:\\Books\\Hugo Pack\\Fifth Season",
            "E:\\Books\\Jemisin\\Fifth Season",
            Some(r#"{"pack_path":"E:\\Books\\Hugo Pack","pack_name":"Hugo Pack"}"#),
        )];

        let export = export_after_apply(&pool, reports.path(), ApplyMode::DryRun, job, plan, &ops)
            .await
            .expect("export");

        assert!(export.json_path.exists(), "the undo file was written");
        assert!(
            export.provenance_json.exists(),
            "provenance re-emitted (json)"
        );
        assert!(
            export.provenance_markdown.exists(),
            "provenance re-emitted (md)"
        );
        // The post-apply re-emit uses its own name so it does NOT overwrite a
        // plan-time provenance-report.json in the same folder.
        assert_eq!(
            export.provenance_json.file_name().and_then(|n| n.to_str()),
            Some("provenance-report-after-apply.json")
        );

        // The undo file on disk round-trips, is self-contained, and carries the
        // dry-run marker (AC-11); a dry-run manifest refuses to reverse.
        let text = std::fs::read_to_string(&export.json_path).expect("read undo file");
        let reread = Manifest::from_json(&text).expect("parse undo file");
        assert_eq!(reread.job_id, job);
        assert_eq!(reread.plan_id, plan);
        assert_eq!(reread.ops.len(), 1);
        assert_eq!(reread.mode, ApplyMode::DryRun, "undo file carries dry-run");
        assert_eq!(
            reread.reverse_ops(),
            Err(ManifestError::DryRunNotReversible)
        );

        // The index row was recorded (append-only) and carries the same marker.
        let (count, mode): (i64, String) = {
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manifests WHERE job_id = ?")
                .bind(job)
                .fetch_one(&pool)
                .await
                .expect("count manifests");
            let mode: String = sqlx::query_scalar("SELECT mode FROM manifests WHERE job_id = ?")
                .bind(job)
                .fetch_one(&pool)
                .await
                .expect("manifest mode");
            (count, mode)
        };
        assert_eq!(count, 1);
        assert_eq!(mode, "dry-run", "the manifests row agrees with the file");
    }
}
