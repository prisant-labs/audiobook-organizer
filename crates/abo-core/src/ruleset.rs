//! F-402 (structure policies) + F-801 (ruleset model and persistence): the
//! toggle set with safe defaults that shapes how the plan builder treats
//! packs, sidecars, non-audio clutter, parallel formats, and empties, bundled
//! with the F-401 naming choice into one schema-versioned, persisted
//! [`Ruleset`].
//!
//! A ruleset is the persisted user intent (breakdown E-08). It is stored as a
//! JSON body in the `rulesets` table (migration 0002, CRUD in
//! [`crate::db::rulesets`]); this module is the typed model that body
//! (de)serializes to, the validator that gates a body before it is ever
//! persisted (AC-29), the constructor for the shipped default ruleset (AC-30),
//! and the pure policy helpers the plan builder (F-403, v0.3.0 Phase 4) calls
//! to turn a policy into a per-book decision.
//!
//! # What Phase 3 owns vs. what Phase 4 does
//!
//! This module is pure policy: no I/O beyond [`seed_default_ruleset`] (a thin
//! wrapper over [`crate::db::rulesets::insert_ruleset`]). It decides, for one
//! input, WHAT the policy says (quarantine this pack shell / keep this ebook /
//! route this folder to manual review). Emitting the actual `plan_ops` rows
//! that carry those decisions - with source/target paths, ordering, and
//! rationale - is the F-403 plan builder's job in Phase 4. The helpers here
//! (`pack_shell_disposition`, `clutter_action_for`, `disposition_for_class`,
//! `StructurePolicy::preferred_format`) are the seams it builds on, kept pure
//! and table-testable so the safety-critical policy behavior is proven before
//! any path-manipulation code exists.
//!
//! # Safe defaults (F-402)
//!
//! [`Ruleset::default`] / [`default_ruleset`] encode every F-402 safe default
//! from the spec's "key behaviors":
//!   - one-book-per-folder enforcement ON (split multi-book folders);
//!   - pack shell after full extraction goes to **quarantine** by default, with
//!     a `leave-in-place` toggle (FD-01, the decision-gate default this phase
//!     confirms);
//!   - sidecars (ebook/cover/description) **keep-with-book**;
//!   - non-audio clutter: **keep** ebook + cover, **quarantine**
//!     nfo/sfv/playlist/weblink;
//!   - preferred canonical format **m4b** (the parallel-format loser is
//!     quarantined, never deleted - the never-delete-audio invariant);
//!   - empty-folder removal ON (`rmdir-empty` only ever targets verified-empty
//!     dirs, enforced later by F-404 validation, not here);
//!   - noise stripping ON (the F-302 strip-noise campaign pass).
//!
//! # Manual-review routing (FD-17)
//!
//! Folders the classifier marks [`FolderClass::ManualReview`] - which is where
//! the FD-17 video/course/radio-play/comic cases already funnel (see
//! `classify::engine`'s `fd17-*` rules) - are never auto-planned:
//! [`disposition_for_class`] maps them to [`PlanDisposition::ManualReview`], so
//! the plan builder emits a `no-op(manual-review)` rather than any move/rename
//! (AC-7). This is a fixed safety rule, not a user toggle.

use crate::error::AppError;
use crate::plan::templates::{Preset, DEFAULT_SERIES_INDEX_WIDTH};
use serde::{Deserialize, Serialize};

/// The ruleset JSON schema version this build reads and writes. A persisted
/// body carrying any other version is rejected by [`Ruleset::validate`]
/// (pre-v1 the schema is resettable, matching the sqlx migration policy; after
/// v1.0.0 schema changes become additive-only and this gains
/// backward-compatible reads).
pub const RULESET_SCHEMA_VERSION: i64 = 1;

/// A complete, validated ruleset: the F-401 naming choice plus the F-402
/// structure and cleanup policies, versioned so `abo-core` and any future CLI
/// share one shape (F-801).
///
/// Serialization is strict and complete: every field is written, and on read
/// every field is required (a body missing any section or field is rejected -
/// see [`parse_and_validate`]). This keeps a persisted body unambiguous and
/// makes AC-29 rejection behavior sharp; the writer is always
/// [`Ruleset::to_body_json`], so a round-trip is lossless.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    /// Schema version of this body; must equal [`RULESET_SCHEMA_VERSION`].
    pub schema_version: i64,
    /// F-401 naming template choice.
    pub naming: NamingPolicy,
    /// F-402 structure policies.
    pub structure: StructurePolicy,
    /// F-402/F-302 cleanup toggles.
    pub cleanup: CleanupPolicy,
}

/// F-401 naming: which preset renders target paths, and the series-index
/// zero-pad width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct NamingPolicy {
    /// The shipped preset (default [`Preset::AbsAuthorFirst`], D-02).
    pub preset: Preset,
    /// `{SeriesIndex}` zero-pad width (default [`DEFAULT_SERIES_INDEX_WIDTH`]);
    /// must be at least 1.
    pub series_index_width: usize,
}

/// Where a pack/collection shell goes after its books are fully extracted
/// (FD-01). The decision-gate default is [`PackShellDestination::Quarantine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackShellDestination {
    /// Move the emptied shell to quarantine (never delete). The FD-01 default.
    #[default]
    Quarantine,
    /// Leave the emptied shell exactly where it is (no op emitted for it).
    LeaveInPlace,
}

/// What happens to a book's sidecars (ebook / cover / description) relative to
/// the book when it moves. Default [`SidecarPolicy::KeepWithBook`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidecarPolicy {
    /// Sidecars travel with their book into the canonical target folder.
    #[default]
    KeepWithBook,
    /// Sidecars are quarantined instead of moved with the book.
    Quarantine,
}

/// The action applied to one class of non-audio clutter file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClutterAction {
    /// Leave the file in place with its book (no op).
    Keep,
    /// Move the file into a book subfolder.
    MoveToSubfolder,
    /// Move the file to quarantine (never delete).
    Quarantine,
}

/// The preferred canonical audio format kept when a book holds parallel
/// formats (F-205's `0 M4B` case). Default [`PreferredFormat::M4b`]; the
/// non-preferred copy is quarantined (the never-delete-audio invariant), never
/// silently deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PreferredFormat {
    /// Keep the `.m4b` single-file copy; quarantine the loser. The default.
    #[default]
    M4b,
    /// Keep the `.mp3` chaptered copy; quarantine the loser.
    Mp3,
}

/// Per-file-class non-audio clutter policy (F-402). The fields are policy
/// CATEGORIES named exactly as the spec's AC-6 enumerates them, not raw
/// `scan::typing` file classes: the plan builder maps a concrete file to the
/// matching category. Note a `.txt`/description sidecar is governed by
/// [`StructurePolicy::sidecars`] (keep-with-book), not by this clutter policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClutterPolicy {
    /// Ebook files (epub/pdf/mobi/...). Default [`ClutterAction::Keep`].
    pub ebook: ClutterAction,
    /// Cover images. Default [`ClutterAction::Keep`].
    pub cover: ClutterAction,
    /// `.nfo` release-info files. Default [`ClutterAction::Quarantine`].
    pub nfo: ClutterAction,
    /// `.sfv` checksum files. Default [`ClutterAction::Quarantine`].
    pub sfv: ClutterAction,
    /// Playlist files (m3u/cue). Default [`ClutterAction::Quarantine`].
    pub playlist: ClutterAction,
    /// Weblink files (url/html). Default [`ClutterAction::Quarantine`].
    pub weblink: ClutterAction,
}

/// The non-audio clutter category a concrete file falls into, used to look its
/// policy up via [`ClutterPolicy::action_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClutterKind {
    Ebook,
    Cover,
    Nfo,
    Sfv,
    Playlist,
    Weblink,
}

/// F-402 structure policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct StructurePolicy {
    /// Enforce one book per folder (split multi-book folders). Default `true`.
    pub one_book_per_folder: bool,
    /// Where a fully-extracted pack shell goes (FD-01). Default
    /// [`PackShellDestination::Quarantine`].
    pub pack_shell: PackShellDestination,
    /// How sidecars are handled. Default [`SidecarPolicy::KeepWithBook`].
    pub sidecars: SidecarPolicy,
    /// Per-class non-audio clutter policy.
    pub clutter: ClutterPolicy,
    /// Preferred canonical format on parallel formats. Default
    /// [`PreferredFormat::M4b`].
    pub preferred_format: PreferredFormat,
    /// Remove verified-empty folders. Default `true`.
    pub empty_folder_removal: bool,
}

/// F-402/F-302 cleanup toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CleanupPolicy {
    /// Run the F-302 strip-noise pass (ripper tags, bitrate/size markers,
    /// rank/year prefixes, ...). Default `true`.
    pub strip_noise: bool,
}

/// Whether a folder is eligible for auto-planned operations, or must route to
/// manual review (FD-17). See [`disposition_for_class`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanDisposition {
    /// Eligible for auto-planned move/rename/quarantine/etc. operations.
    AutoPlan,
    /// Never auto-planned: the plan builder emits `no-op(manual-review)`.
    ManualReview,
}

/// Whether a pack shell is quarantined or left where it is, once the guard for
/// partial extraction (AC-5) has been applied. See [`pack_shell_disposition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackShellOutcome {
    /// Emit a quarantine op for the emptied shell.
    Quarantine,
    /// Emit nothing for the shell; it stays exactly where it is.
    LeaveInPlace,
}

impl Default for NamingPolicy {
    fn default() -> Self {
        NamingPolicy {
            preset: Preset::AbsAuthorFirst,
            series_index_width: DEFAULT_SERIES_INDEX_WIDTH,
        }
    }
}

impl Default for ClutterPolicy {
    fn default() -> Self {
        ClutterPolicy {
            ebook: ClutterAction::Keep,
            cover: ClutterAction::Keep,
            nfo: ClutterAction::Quarantine,
            sfv: ClutterAction::Quarantine,
            playlist: ClutterAction::Quarantine,
            weblink: ClutterAction::Quarantine,
        }
    }
}

impl ClutterPolicy {
    /// The action this policy assigns to a given clutter category.
    pub fn action_for(&self, kind: ClutterKind) -> ClutterAction {
        match kind {
            ClutterKind::Ebook => self.ebook,
            ClutterKind::Cover => self.cover,
            ClutterKind::Nfo => self.nfo,
            ClutterKind::Sfv => self.sfv,
            ClutterKind::Playlist => self.playlist,
            ClutterKind::Weblink => self.weblink,
        }
    }
}

impl Default for StructurePolicy {
    fn default() -> Self {
        StructurePolicy {
            one_book_per_folder: true,
            pack_shell: PackShellDestination::Quarantine,
            sidecars: SidecarPolicy::KeepWithBook,
            clutter: ClutterPolicy::default(),
            preferred_format: PreferredFormat::M4b,
            empty_folder_removal: true,
        }
    }
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        CleanupPolicy { strip_noise: true }
    }
}

impl Default for Ruleset {
    fn default() -> Self {
        Ruleset {
            schema_version: RULESET_SCHEMA_VERSION,
            naming: NamingPolicy::default(),
            structure: StructurePolicy::default(),
            cleanup: CleanupPolicy::default(),
        }
    }
}

impl Ruleset {
    /// Validate this ruleset's invariants (independent of how it was
    /// constructed): the schema version must be the one this build supports,
    /// and the series-index width must be at least 1. Returns
    /// [`AppError::RulesetInvalid`] (machine code `ruleset-invalid`) on any
    /// violation.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != RULESET_SCHEMA_VERSION {
            return Err(AppError::RulesetInvalid {
                detail: format!(
                    "unsupported schema_version {} (this build supports {})",
                    self.schema_version, RULESET_SCHEMA_VERSION
                ),
            });
        }
        if self.naming.series_index_width < 1 {
            return Err(AppError::RulesetInvalid {
                detail: "naming.series_index_width must be at least 1".to_string(),
            });
        }
        Ok(())
    }

    /// Serialize this ruleset to its canonical JSON body. Deterministic (serde
    /// writes fields in declaration order); the inverse of
    /// [`parse_and_validate`]. Infallible for this type (no map keys, no
    /// non-string keys, no NaN), so the internal serialize error is unreachable
    /// and surfaced as a panic rather than polluting the signature.
    pub fn to_body_json(&self) -> String {
        serde_json::to_string(self).expect("a Ruleset always serializes to JSON")
    }
}

/// Parse a JSON body into a validated [`Ruleset`], or reject it with
/// [`AppError::RulesetInvalid`] (AC-29). This is the save gate: a caller
/// persisting a user-supplied body validates HERE first, then stores
/// [`Ruleset::to_body_json`] of the result, so a malformed, incomplete, wrong-
/// typed, wrong-version, or out-of-range body is never written.
pub fn parse_and_validate(body_json: &str) -> Result<Ruleset, AppError> {
    let ruleset: Ruleset =
        serde_json::from_str(body_json).map_err(|e| AppError::RulesetInvalid {
            detail: format!("body is not a valid ruleset: {e}"),
        })?;
    ruleset.validate()?;
    Ok(ruleset)
}

/// The shipped default ruleset (AC-30): `abs-author-first` naming (D-02) and
/// pack-shell-to-quarantine (FD-01), with every other F-402 safe default. This
/// is exactly [`Ruleset::default`]; the named function exists so the seed path
/// and its assertions read intently.
pub fn default_ruleset() -> Ruleset {
    Ruleset::default()
}

/// Route a classified folder to auto-planning or to manual review. FD-17
/// non-book media (video/course/radio-play/comic) reach
/// [`FolderClass::ManualReview`] via the classifier's `fd17-*` rules, so this
/// maps that class to [`PlanDisposition::ManualReview`]; the plan builder then
/// emits `no-op(manual-review)` and zero move/rename ops for it (AC-7). Every
/// other class is eligible for auto-planning (the builder decides the concrete
/// per-class treatment).
pub fn disposition_for_class(class: crate::classify::FolderClass) -> PlanDisposition {
    match class {
        crate::classify::FolderClass::ManualReview => PlanDisposition::ManualReview,
        _ => PlanDisposition::AutoPlan,
    }
}

/// Decide what happens to a pack shell after extraction, applying the AC-5
/// partial-extraction guard: quarantining the shell requires that ALL its
/// members were successfully planned. If any member is blocked/unplanned
/// (`all_members_planned == false`), the shell is left in place REGARDLESS of
/// the configured [`PackShellDestination`], because quarantining a shell that
/// still (logically) owns un-extracted books would strand them. When every
/// member is planned, the configured destination applies (AC-4).
pub fn pack_shell_disposition(
    destination: PackShellDestination,
    all_members_planned: bool,
) -> PackShellOutcome {
    if !all_members_planned {
        return PackShellOutcome::LeaveInPlace;
    }
    match destination {
        PackShellDestination::Quarantine => PackShellOutcome::Quarantine,
        PackShellDestination::LeaveInPlace => PackShellOutcome::LeaveInPlace,
    }
}

/// Seed the shipped default ruleset into an empty `rulesets` table, returning
/// its assigned id. Thin wrapper over [`crate::db::rulesets::insert_ruleset`]
/// with a validated, canonical body; keeps the sqlx error type of the db layer
/// (the body is trusted - it comes from [`default_ruleset`] - so no
/// validation-rejection path applies here).
pub async fn seed_default_ruleset(
    pool: &sqlx::SqlitePool,
    name: &str,
    now: &str,
) -> Result<i64, sqlx::Error> {
    let ruleset = default_ruleset();
    let body = ruleset.to_body_json();
    crate::db::rulesets::insert_ruleset(
        pool,
        &crate::db::rulesets::NewRuleset {
            name,
            body_json: &body,
            schema_version: ruleset.schema_version,
        },
        now,
    )
    .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_fields_are_rejected_not_ignored() {
        // AC-29 sharpness: a body carrying keys outside the schema is
        // ruleset-invalid, never silently accepted and dropped on rewrite.
        let mut body: serde_json::Value =
            serde_json::from_str(&Ruleset::default().to_body_json()).unwrap();
        body.as_object_mut()
            .unwrap()
            .insert("future_field".to_string(), serde_json::json!(true));
        let err = parse_and_validate(&body.to_string()).unwrap_err();
        assert_eq!(err.code(), "ruleset-invalid");
    }

    use super::*;

    // ---- AC-30: the shipped default ruleset's values. -------------------

    /// The shipped default ruleset carries `abs-author-first` (D-02) and
    /// pack-shell-to-quarantine (FD-01), plus every other documented F-402
    /// safe default.
    #[test]
    fn default_ruleset_carries_abs_author_first_and_pack_shell_to_quarantine() {
        let rs = default_ruleset();

        // AC-30's two named values.
        assert_eq!(rs.naming.preset, Preset::AbsAuthorFirst);
        assert_eq!(rs.structure.pack_shell, PackShellDestination::Quarantine);

        // The rest of the F-402 safe-default set (spec key behaviors).
        assert_eq!(rs.schema_version, RULESET_SCHEMA_VERSION);
        assert_eq!(rs.naming.series_index_width, DEFAULT_SERIES_INDEX_WIDTH);
        assert!(rs.structure.one_book_per_folder);
        assert_eq!(rs.structure.sidecars, SidecarPolicy::KeepWithBook);
        assert_eq!(rs.structure.preferred_format, PreferredFormat::M4b);
        assert!(rs.structure.empty_folder_removal);
        assert!(rs.cleanup.strip_noise);

        // AC-6 clutter defaults: keep ebook + cover, quarantine the rest.
        assert_eq!(rs.structure.clutter.ebook, ClutterAction::Keep);
        assert_eq!(rs.structure.clutter.cover, ClutterAction::Keep);
        assert_eq!(rs.structure.clutter.nfo, ClutterAction::Quarantine);
        assert_eq!(rs.structure.clutter.sfv, ClutterAction::Quarantine);
        assert_eq!(rs.structure.clutter.playlist, ClutterAction::Quarantine);
        assert_eq!(rs.structure.clutter.weblink, ClutterAction::Quarantine);
    }

    /// The default ruleset's series-index width equals the templates default
    /// (both `1`, matching the ratified prototypes' `Book 1`/`Book 5`
    /// renders), so the two constants can never drift apart.
    #[test]
    fn default_ruleset_width_matches_templates_default() {
        assert_eq!(default_ruleset().naming.series_index_width, 1);
        assert_eq!(DEFAULT_SERIES_INDEX_WIDTH, 1);
    }

    // ---- AC-29: schema-versioned JSON round-trip + reject on save. -------

    /// A default ruleset serializes to a JSON body carrying its
    /// `schema_version` and round-trips back byte-for-byte equal through
    /// [`parse_and_validate`].
    #[test]
    fn default_ruleset_round_trips_through_json_with_schema_version() {
        let rs = default_ruleset();
        let body = rs.to_body_json();

        // The body advertises its schema version explicitly.
        assert!(
            body.contains("\"schema_version\":1"),
            "body must carry schema_version: {body}"
        );
        // And the spec-native preset string, not a Rust variant name.
        assert!(
            body.contains("\"abs-author-first\""),
            "preset must serialize spec-native: {body}"
        );

        let back = parse_and_validate(&body).expect("valid body must round-trip");
        assert_eq!(back, rs);
    }

    /// Serialization is deterministic: the same ruleset always produces the
    /// identical body string (the plan/report determinism story depends on
    /// this).
    #[test]
    fn to_body_json_is_deterministic() {
        assert_eq!(
            default_ruleset().to_body_json(),
            default_ruleset().to_body_json()
        );
    }

    /// Malformed JSON is rejected on save with the `ruleset-invalid` machine
    /// code, never persisted.
    #[test]
    fn malformed_json_is_rejected_with_machine_code() {
        let err = parse_and_validate("{not json").expect_err("must reject");
        assert_eq!(err.code(), "ruleset-invalid");
    }

    /// A body missing a required section (here `naming`) is rejected: the
    /// schema is strict, so an incomplete body never silently fills unknown
    /// intent.
    #[test]
    fn body_missing_a_required_section_is_rejected() {
        let body = r#"{"schema_version":1,"structure":{},"cleanup":{}}"#;
        let err = parse_and_validate(body).expect_err("must reject incomplete body");
        assert_eq!(err.code(), "ruleset-invalid");
    }

    /// A body whose `schema_version` this build does not support is rejected
    /// (pre-v1 the schema is resettable; an unknown version is not silently
    /// coerced).
    #[test]
    fn unsupported_schema_version_is_rejected() {
        let mut rs = default_ruleset();
        rs.schema_version = 999;
        // Serialize via serde directly (to_body_json would carry 999 too).
        let body = serde_json::to_string(&rs).expect("serialize");
        let err = parse_and_validate(&body).expect_err("must reject unknown version");
        assert_eq!(err.code(), "ruleset-invalid");
        assert!(err.to_string().contains("999"));
    }

    /// A field of the wrong type (here `series_index_width` as a string) is
    /// rejected by deserialization, with the machine code.
    #[test]
    fn wrong_field_type_is_rejected() {
        let body = r#"{"schema_version":1,
            "naming":{"preset":"abs-author-first","series_index_width":"two"},
            "structure":{"one_book_per_folder":true,"pack_shell":"quarantine",
                "sidecars":"keep-with-book",
                "clutter":{"ebook":"keep","cover":"keep","nfo":"quarantine",
                    "sfv":"quarantine","playlist":"quarantine","weblink":"quarantine"},
                "preferred_format":"m4b","empty_folder_removal":true},
            "cleanup":{"strip_noise":true}}"#;
        let err = parse_and_validate(body).expect_err("must reject wrong type");
        assert_eq!(err.code(), "ruleset-invalid");
    }

    /// A parseable body with a degenerate value (series-index width 0) is
    /// rejected by [`Ruleset::validate`], not silently accepted.
    #[test]
    fn zero_series_index_width_is_rejected() {
        let mut rs = default_ruleset();
        rs.naming.series_index_width = 0;
        let body = serde_json::to_string(&rs).expect("serialize");
        let err = parse_and_validate(&body).expect_err("must reject width 0");
        assert_eq!(err.code(), "ruleset-invalid");
    }

    /// An unknown preset string is rejected (the preset enum is closed).
    #[test]
    fn unknown_preset_is_rejected() {
        let mut body = default_ruleset().to_body_json();
        body = body.replace("\"abs-author-first\"", "\"made-up-preset\"");
        let err = parse_and_validate(&body).expect_err("must reject unknown preset");
        assert_eq!(err.code(), "ruleset-invalid");
    }

    // ---- AC-4 / AC-5: pack-shell destination + partial-extraction guard. -

    /// AC-4: with full extraction, the default (quarantine) quarantines the
    /// shell and the `leave-in-place` toggle emits no quarantine op.
    #[test]
    fn pack_shell_full_extraction_follows_the_toggle() {
        assert_eq!(
            pack_shell_disposition(PackShellDestination::Quarantine, true),
            PackShellOutcome::Quarantine
        );
        assert_eq!(
            pack_shell_disposition(PackShellDestination::LeaveInPlace, true),
            PackShellOutcome::LeaveInPlace
        );
    }

    /// AC-5: a partially-extracted pack (a blocked/unplanned member) leaves
    /// the shell in place REGARDLESS of the toggle.
    #[test]
    fn pack_shell_partial_extraction_always_leaves_in_place() {
        for destination in [
            PackShellDestination::Quarantine,
            PackShellDestination::LeaveInPlace,
        ] {
            assert_eq!(
                pack_shell_disposition(destination, false),
                PackShellOutcome::LeaveInPlace,
                "destination {destination:?} must not quarantine a partial pack"
            );
        }
    }

    // ---- AC-6: clutter action lookup follows the policy. ----------------

    /// The default clutter policy quarantines nfo/sfv/playlist/weblink and
    /// keeps ebook/cover, looked up by category.
    #[test]
    fn default_clutter_actions_match_ac6() {
        let clutter = ClutterPolicy::default();
        assert_eq!(clutter.action_for(ClutterKind::Ebook), ClutterAction::Keep);
        assert_eq!(clutter.action_for(ClutterKind::Cover), ClutterAction::Keep);
        assert_eq!(
            clutter.action_for(ClutterKind::Nfo),
            ClutterAction::Quarantine
        );
        assert_eq!(
            clutter.action_for(ClutterKind::Sfv),
            ClutterAction::Quarantine
        );
        assert_eq!(
            clutter.action_for(ClutterKind::Playlist),
            ClutterAction::Quarantine
        );
        assert_eq!(
            clutter.action_for(ClutterKind::Weblink),
            ClutterAction::Quarantine
        );
    }

    // ---- AC-7: FD-17 classes route to manual review. --------------------

    /// A folder classified `manual-review` (where FD-17 video/course/
    /// radio-play/comic funnel) routes to [`PlanDisposition::ManualReview`];
    /// every planable class routes to [`PlanDisposition::AutoPlan`].
    #[test]
    fn manual_review_class_routes_to_manual_review_others_auto_plan() {
        use crate::classify::FolderClass;

        assert_eq!(
            disposition_for_class(FolderClass::ManualReview),
            PlanDisposition::ManualReview
        );
        for class in [
            FolderClass::Book,
            FolderClass::SeriesContainer,
            FolderClass::PackContainer,
            FolderClass::Staging,
            FolderClass::Mixed,
            FolderClass::MultiBookSuspect,
            FolderClass::Empty,
            FolderClass::DocsResources,
        ] {
            assert_eq!(
                disposition_for_class(class),
                PlanDisposition::AutoPlan,
                "class {class:?} should be auto-planable"
            );
        }
    }

    // ---- Persistence: seed + reload through the db layer. ---------------

    /// The seeded default ruleset reloads from the db and re-validates,
    /// exercising the F-801 CRUD stack (ruleset.rs validation +
    /// db::rulesets persistence) end to end.
    #[tokio::test]
    async fn seed_default_ruleset_persists_and_reloads_valid() {
        use crate::db::open_db;
        use crate::db::rulesets::get_ruleset;
        use tempfile::TempDir;

        let dir = TempDir::new().expect("db tempdir");
        let (pool, _) = open_db(dir.path()).await.expect("open_db");

        let id = seed_default_ruleset(&pool, "Default", "2026-07-04T00:00:00Z")
            .await
            .expect("seed_default_ruleset");

        let row = get_ruleset(&pool, id)
            .await
            .expect("get_ruleset")
            .expect("row exists");
        assert_eq!(row.name, "Default");
        assert_eq!(row.schema_version, RULESET_SCHEMA_VERSION);

        let reloaded = parse_and_validate(&row.body_json).expect("stored body is valid");
        assert_eq!(reloaded, default_ruleset());
    }
}
