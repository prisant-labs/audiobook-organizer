//! F-303 (field extraction with confidence): merges the per-name matcher
//! output (F-301) UP THE TREE and emits, per entry, merged fields each
//! carrying an explicit confidence tier, plus a per-entry list of conflicts
//! it refused to silently resolve.
//!
//! Like the rest of `crate::parse`, this module is pure logic: no I/O, no
//! database, no filesystem access, nothing `cfg`-gated (the CFG RULE). Its
//! input is a flat slice of [`EntryInput`] carrying only what the merge
//! needs - a stable id, an optional parent id, a raw component name, and
//! whether the entry is a file or a folder. The caller (a snapshot walk in
//! production, the generated fixture library in tests) adapts its own entry
//! records into this shape; this module never sees a `Path`, a `PathBuf`, or
//! a database row.
//!
//! # Folder-first (FD-14)
//!
//! v1 extraction is FOLDER-FIRST by decision FD-14, superseding the
//! discovery doc's `preferSource=tags` default: fields come from folder and
//! file NAMES, and embedded tags play NO role in v1 extraction (F-1101, the
//! tag reader, is deferred). The FD-14 tag-quality probe (release gate G-08)
//! measures whether that assumption holds on the real library and records a
//! verdict; that verdict tunes the confidence weighting through
//! [`ConfidenceWeights`] (see below), it does not reintroduce tag reading
//! here.
//!
//! # The three confidence tiers
//!
//! Each merged field records WHERE its value came from ([`FieldSource`]),
//! and [`ConfidenceWeights`] maps that source to a [`Confidence`]:
//!
//! - **high** = read explicitly from the entry's OWN name (a clean matcher
//!   result on this entry's own folder/file name).
//! - **medium** = INHERITED from the nearest ancestor folder whose own name
//!   explicitly carried the field (a book file inheriting its series folder's
//!   author/series).
//! - **low** = DERIVED / guessed: the entry's own name did not parse cleanly
//!   and the whole string was kept verbatim as a best-effort title (the
//!   F-301 degraded pattern-8 fallback, e.g. the AC-301.3 "FREE SAMPLE_"
//!   outlier). Never a fabricated value: an absent field is [`None`], never a
//!   guess dressed up as data.
//!
//! # What inherits, and what does not
//!
//! Only **author** and **series** inherit (spec AC-303.2: "a file inside a
//! parsed series-container inherits series and author at medium
//! confidence"). Everything else is book-specific and never inherited:
//! `title`, `subtitle`, `series_index`, `year`, and `narrator` each come only
//! from the entry's own name. Inheritance is nearest-ancestor-wins: walking
//! from the entry toward the root, the first ancestor whose OWN name
//! explicitly carried author (resp. series) supplies it.
//!
//! # Precedence and conflict flagging
//!
//! Explicit beats inherited: if an entry's own name carries author (or
//! series), that value is kept at high confidence and is NOT overwritten by
//! an ancestor's value. When the own (explicit) value and the nearest
//! ancestor's (would-be-inherited) value DISAGREE, that disagreement is
//! recorded in [`MergedEntry::conflicts`] and never silently resolved
//! (spec F-303 edge case: "conflicting inherited vs explicit values surface
//! the explicit and flag the conflict"). Downstream, P5 classification and
//! the review UI decide what to do with a conflict; this layer's job is to
//! surface it honestly, not to pick a winner.
//!
//! # A deliberate limitation this layer does not hide
//!
//! Extraction is a pure structural NAME pass; it has no notion of which
//! folders are meaningful author/series containers versus organizational
//! shelves. A shelf folder named like `Genre - Children` parses (via the
//! lowest-specificity F-301 pattern 2) as author `Genre`, title `Children`,
//! so a bare-titled book sitting directly under it INHERITS author `Genre` at
//! MEDIUM, and a book with its own explicit author under it FLAGS a conflict
//! against `Genre`. This is intentional and correct for this layer: the value
//! is genuinely read from an ancestor's name (not fabricated), it is marked
//! only medium (a review tier, never high), and the conflict is surfaced.
//! Distinguishing an organizational shelf from a real series/author container
//! is F-201 classification's job (P5), which consumes these merged fields and
//! conflicts; it is deliberately NOT reimplemented here. (Whether a shelf
//! parses as an author at all depends on its exact name: `Genre - SciFI`, for
//! instance, does NOT, because F-302's release-group stripper treats the
//! mixed-case `SciFI` token as a ripper tag and removes it - another reason
//! this layer flags rather than trusts such inferences.)

use std::collections::HashMap;

use crate::parse::matchers::{MatchOutcome, MatcherId, DEGRADED_FALLBACK_SCORE};
use crate::parse::{parse_preview, strip_known_extension, ParsedFields};

/// Whether an [`EntryInput`] is a file or a folder. File names have their
/// recognized audio extension stripped before matching (so patterns that key
/// on the whole string are not thrown off by `.m4b`); folder names never do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Folder,
}

/// One entry the merge operates over. `id` is any caller-stable identifier
/// (the caller's row id, or a positional index); `parent` is the `id` of the
/// containing folder, or [`None`] for a top-level entry. `name` is the single
/// raw path component (not a full path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInput {
    pub id: usize,
    pub parent: Option<usize>,
    pub name: String,
    pub kind: NodeKind,
}

/// A per-field confidence tier (F-303).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Where a merged field's value came from. This is the stable, tuning-
/// independent fact; [`ConfidenceWeights`] turns it into a [`Confidence`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSource {
    /// Read explicitly from the entry's own name.
    OwnName,
    /// Inherited from the nearest ancestor whose own name carried the field.
    Inherited,
    /// Derived / guessed (the F-301 degraded fallback kept a noisy name
    /// verbatim as a title).
    Derived,
}

/// Which structured field a value or conflict is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldName {
    Title,
    Author,
    Series,
    SeriesIndex,
    Year,
    Narrator,
    Subtitle,
}

/// The FD-14 tunable hook: maps each [`FieldSource`] to the [`Confidence`]
/// tier it earns. The default is the v1 folder-first weighting (own name =>
/// high, inherited => medium, derived => low). The FD-14 tag-quality probe
/// (release gate G-08) can hand P7 a different mapping - for example demoting
/// inherited fields to low if folder names prove materially less reliable
/// than the probe hoped - WITHOUT any change to the merge logic itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfidenceWeights {
    pub own_name: Confidence,
    pub inherited: Confidence,
    pub derived: Confidence,
}

impl Default for ConfidenceWeights {
    /// The documented v1 folder-first default: own name => high, inherited
    /// => medium, derived => low (FD-14).
    fn default() -> Self {
        ConfidenceWeights {
            own_name: Confidence::High,
            inherited: Confidence::Medium,
            derived: Confidence::Low,
        }
    }
}

impl ConfidenceWeights {
    fn confidence_for(&self, source: FieldSource) -> Confidence {
        match source {
            FieldSource::OwnName => self.own_name,
            FieldSource::Inherited => self.inherited,
            FieldSource::Derived => self.derived,
        }
    }
}

/// A single merged field: its value, where it came from, and the resolved
/// confidence tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<T> {
    pub value: T,
    pub source: FieldSource,
    pub confidence: Confidence,
}

/// The merged, confidence-annotated fields for one entry. Every field is
/// optional: an absent field is [`None`] (fabricate nothing), never a guessed
/// value dressed as data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedFields {
    pub title: Option<Field<String>>,
    pub author: Option<Field<String>>,
    pub series: Option<Field<String>>,
    pub series_index: Option<Field<String>>,
    pub year: Option<Field<u16>>,
    pub narrator: Option<Field<String>>,
    pub subtitle: Option<Field<String>>,
}

/// A disagreement between an entry's own explicit value and the value it
/// would otherwise have inherited from an ancestor. The explicit `own_value`
/// is kept (at high confidence); this record surfaces the discarded
/// `ancestor_value` for review rather than silently resolving it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldConflict {
    pub field: FieldName,
    pub own_value: String,
    pub ancestor_value: String,
    /// The `id` of the ancestor whose name carried the conflicting value.
    pub ancestor_id: usize,
}

/// The merge result for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedEntry {
    pub id: usize,
    /// Which matcher parsed this entry's OWN name, if one produced a
    /// confident result. [`None`] when the own name was ambiguous or matched
    /// nothing. A degraded pattern-8 fallback still records
    /// [`MatcherId::Pattern8BareTitle`] here.
    pub own_matcher: Option<MatcherId>,
    /// Whether this entry's own name resolved to an F-301 tie
    /// ([`MatchOutcome::Ambiguous`]); when true, no own fields are trusted
    /// (they may still be inherited) and this is surfaced for review.
    pub own_ambiguous: bool,
    pub fields: MergedFields,
    pub conflicts: Vec<FieldConflict>,
}

/// The confident own-name parse of a single entry, precomputed once so the
/// ancestor walk can consult any entry's own fields without re-parsing.
struct OwnParse {
    /// Fields read cleanly from the own name. Empty for an ambiguous, no-
    /// match, or degraded-fallback name (a degraded title is carried
    /// separately in `derived_title`, not here, so it is never treated as an
    /// inheritable or explicit-high value).
    fields: ParsedFields,
    matcher: Option<MatcherId>,
    ambiguous: bool,
    /// A best-effort title kept verbatim from a degraded pattern-8 fallback
    /// (low confidence / derived). [`None`] for every non-degraded outcome.
    derived_title: Option<String>,
}

/// Merge the entry tree with the default (v1 folder-first) confidence
/// weighting (FD-14). See [`extract_with_weights`].
pub fn extract(entries: &[EntryInput]) -> Vec<MergedEntry> {
    extract_with_weights(entries, &ConfidenceWeights::default())
}

/// Merge the entry tree, assigning each field a confidence tier via
/// `weights` (the FD-14 hook). Output is one [`MergedEntry`] per input entry,
/// in input order.
pub fn extract_with_weights(
    entries: &[EntryInput],
    weights: &ConfidenceWeights,
) -> Vec<MergedEntry> {
    // Map each caller id to its position in `entries`, so parent ids resolve
    // to array indices for the ancestor walk. A parent id that does not
    // resolve (dangling) is treated as "no parent" rather than panicking:
    // this layer never fabricates structure it was not given.
    let index_of: HashMap<usize, usize> =
        entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let parent_index: Vec<Option<usize>> = entries
        .iter()
        .map(|e| e.parent.and_then(|pid| index_of.get(&pid).copied()))
        .collect();

    // Precompute every entry's own-name parse once.
    let own: Vec<OwnParse> = entries.iter().map(parse_own).collect();

    entries
        .iter()
        .enumerate()
        .map(|(i, e)| merge_entry(i, e, &own, &parent_index, entries, weights))
        .collect()
}

/// Parse one entry's own name into an [`OwnParse`].
fn parse_own(entry: &EntryInput) -> OwnParse {
    let outcome = own_name_outcome(&entry.name, entry.kind);
    match outcome {
        MatchOutcome::Matched(m) => {
            let degraded =
                m.id == MatcherId::Pattern8BareTitle && m.score == DEGRADED_FALLBACK_SCORE;
            if degraded {
                OwnParse {
                    fields: ParsedFields::default(),
                    matcher: Some(m.id),
                    ambiguous: false,
                    derived_title: m.fields.title,
                }
            } else {
                OwnParse {
                    fields: m.fields,
                    matcher: Some(m.id),
                    ambiguous: false,
                    derived_title: None,
                }
            }
        }
        MatchOutcome::Ambiguous(_) => OwnParse {
            fields: ParsedFields::default(),
            matcher: None,
            ambiguous: true,
            derived_title: None,
        },
        MatchOutcome::NoMatch => OwnParse {
            fields: ParsedFields::default(),
            matcher: None,
            ambiguous: false,
            derived_title: None,
        },
    }
}

/// Run the F-301 matcher stack against an entry's own name, choosing the
/// name form appropriate to its kind.
///
/// Folders are matched as-is. Files are the subtle case, because the P4a
/// matcher stack strips a recognized audio extension only inside patterns 1
/// and 8 (the two that are extension-aware by design); patterns 2-7 and 9
/// would otherwise fold a trailing `.m4b` into their last captured field. So
/// a file is first matched WITH its extension: if that yields a clean
/// loose-root `Title by Author` (pattern 1) or the protected degraded
/// pattern-8 fallback (which carries the AC-301.3 outlier guard), that
/// extension-aware result is kept. Otherwise the file is re-matched on its
/// extension-stripped stem, so an in-folder file like `14.0 - Cold Days.m4b`
/// resolves through pattern 6 to `series_index=14.0, title=Cold Days` rather
/// than a `.m4b`-polluted title.
fn own_name_outcome(name: &str, kind: NodeKind) -> MatchOutcome {
    match kind {
        NodeKind::Folder => parse_preview(name),
        NodeKind::File => {
            let with_ext = parse_preview(name);
            match &with_ext {
                MatchOutcome::Matched(m) if m.id == MatcherId::Pattern1TitleByAuthorLooseRoot => {
                    with_ext
                }
                MatchOutcome::Matched(m)
                    if m.id == MatcherId::Pattern8BareTitle
                        && m.score == DEGRADED_FALLBACK_SCORE =>
                {
                    with_ext
                }
                _ => parse_preview(strip_known_extension(name)),
            }
        }
    }
}

/// Build the [`MergedEntry`] for the entry at position `i`.
fn merge_entry(
    i: usize,
    entry: &EntryInput,
    own: &[OwnParse],
    parent_index: &[Option<usize>],
    entries: &[EntryInput],
    weights: &ConfidenceWeights,
) -> MergedEntry {
    let mine = &own[i];
    let mut fields = MergedFields::default();
    let mut conflicts = Vec::new();

    // ---- book-specific fields: own name only, never inherited ----
    fields.title = match (&mine.fields.title, &mine.derived_title) {
        (Some(t), _) => Some(own_field(t.clone(), weights)),
        (None, Some(d)) => Some(derived_field(d.clone(), weights)),
        (None, None) => None,
    };
    fields.subtitle = mine.fields.subtitle.clone().map(|v| own_field(v, weights));
    fields.series_index = mine
        .fields
        .series_index
        .clone()
        .map(|v| own_field(v, weights));
    fields.year = mine.fields.year.map(|v| own_field(v, weights));
    fields.narrator = mine.fields.narrator.clone().map(|v| own_field(v, weights));

    // ---- inheritable fields: author and series ----
    fields.author = resolve_inheritable(
        i,
        FieldName::Author,
        mine.fields.author.as_deref(),
        own,
        parent_index,
        entries,
        weights,
        &mut conflicts,
        |o| o.fields.author.as_deref(),
    );
    fields.series = resolve_inheritable(
        i,
        FieldName::Series,
        mine.fields.series.as_deref(),
        own,
        parent_index,
        entries,
        weights,
        &mut conflicts,
        |o| o.fields.series.as_deref(),
    );

    MergedEntry {
        id: entry.id,
        own_matcher: mine.matcher,
        own_ambiguous: mine.ambiguous,
        fields,
        conflicts,
    }
}

/// Resolve one inheritable field (author or series) for entry `i`, applying
/// explicit-over-inherited precedence and flagging a conflict when the
/// explicit own value disagrees with the nearest ancestor's value.
#[allow(clippy::too_many_arguments)]
fn resolve_inheritable(
    i: usize,
    field: FieldName,
    own_value: Option<&str>,
    own: &[OwnParse],
    parent_index: &[Option<usize>],
    entries: &[EntryInput],
    weights: &ConfidenceWeights,
    conflicts: &mut Vec<FieldConflict>,
    get: impl Fn(&OwnParse) -> Option<&str>,
) -> Option<Field<String>> {
    let ancestor = nearest_ancestor_value(i, parent_index, own, entries, &get);
    match own_value {
        Some(mine) => {
            // Explicit wins and stays high. If an ancestor disagrees, record
            // the conflict but keep the explicit value.
            if let Some((anc_value, anc_id)) = &ancestor {
                if anc_value != mine {
                    conflicts.push(FieldConflict {
                        field,
                        own_value: mine.to_string(),
                        ancestor_value: anc_value.clone(),
                        ancestor_id: *anc_id,
                    });
                }
            }
            Some(own_field(mine.to_string(), weights))
        }
        None => ancestor.map(|(value, _)| inherited_field(value, weights)),
    }
}

/// Walk from entry `i` toward the root, returning the value and id of the
/// nearest ancestor whose OWN name explicitly carried the field (via `get`),
/// or [`None`] if no ancestor has it. The walk is bounded by the entry count
/// so a malformed (cyclic) parent chain can never loop forever.
fn nearest_ancestor_value(
    i: usize,
    parent_index: &[Option<usize>],
    own: &[OwnParse],
    entries: &[EntryInput],
    get: &impl Fn(&OwnParse) -> Option<&str>,
) -> Option<(String, usize)> {
    let mut steps = 0;
    let mut cur = parent_index[i];
    while let Some(idx) = cur {
        if steps > entries.len() {
            break;
        }
        steps += 1;
        if let Some(value) = get(&own[idx]) {
            return Some((value.to_string(), entries[idx].id));
        }
        cur = parent_index[idx];
    }
    None
}

fn own_field<T>(value: T, weights: &ConfidenceWeights) -> Field<T> {
    Field {
        value,
        source: FieldSource::OwnName,
        confidence: weights.confidence_for(FieldSource::OwnName),
    }
}

fn inherited_field(value: String, weights: &ConfidenceWeights) -> Field<String> {
    Field {
        value,
        source: FieldSource::Inherited,
        confidence: weights.confidence_for(FieldSource::Inherited),
    }
}

fn derived_field(value: String, weights: &ConfidenceWeights) -> Field<String> {
    Field {
        value,
        source: FieldSource::Derived,
        confidence: weights.confidence_for(FieldSource::Derived),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: usize, parent: Option<usize>, name: &str) -> EntryInput {
        EntryInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::Folder,
        }
    }

    fn file(id: usize, parent: Option<usize>, name: &str) -> EntryInput {
        EntryInput {
            id,
            parent,
            name: name.to_string(),
            kind: NodeKind::File,
        }
    }

    fn by_id(merged: &[MergedEntry], id: usize) -> &MergedEntry {
        merged.iter().find(|m| m.id == id).expect("id present")
    }

    /// AC-303.2: a file inside a parsed series-container inherits series and
    /// author at MEDIUM; an explicit in-name field is HIGH and overrides.
    #[test]
    fn inherits_author_and_series_at_medium_and_own_title_is_high() {
        // Frank Herbert-Dune container (author + series) with a member file
        // that has only its own title.
        let entries = vec![
            folder(0, None, "Frank Herbert-Dune-#1-Chronicles[1-8}"),
            file(1, Some(0), "The Battle of Corrin.m4b"),
        ];
        let merged = extract(&entries);

        let container = by_id(&merged, 0);
        assert_eq!(
            container
                .fields
                .author
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence)),
            Some(("Frank Herbert", Confidence::High))
        );
        assert_eq!(
            container
                .fields
                .series
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence)),
            Some(("Dune", Confidence::High))
        );

        let member = by_id(&merged, 1);
        // Own title, high.
        assert_eq!(
            member
                .fields
                .title
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence, f.source)),
            Some((
                "The Battle of Corrin",
                Confidence::High,
                FieldSource::OwnName
            ))
        );
        // Inherited author + series, medium.
        assert_eq!(
            member
                .fields
                .author
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence, f.source)),
            Some(("Frank Herbert", Confidence::Medium, FieldSource::Inherited))
        );
        assert_eq!(
            member
                .fields
                .series
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence, f.source)),
            Some(("Dune", Confidence::Medium, FieldSource::Inherited))
        );
        assert!(member.conflicts.is_empty());
    }

    /// Explicit-over-inherited: the member's own author overrides, and the
    /// disagreement with the ancestor is flagged, not silently resolved.
    #[test]
    fn explicit_author_overrides_inherited_and_flags_conflict() {
        let entries = vec![
            folder(0, None, "Frank Herbert-Dune-#1-Chronicles[1-8}"),
            // A misfiled member whose own name parses to a different author.
            file(1, Some(0), "Brian Herbert - Sandworms of Dune.m4b"),
        ];
        let merged = extract(&entries);
        let member = by_id(&merged, 1);

        // Own explicit author kept at high.
        assert_eq!(
            member
                .fields
                .author
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence)),
            Some(("Brian Herbert", Confidence::High))
        );
        // Conflict recorded against the ancestor, explicit preserved.
        assert_eq!(member.conflicts.len(), 1);
        let c = &member.conflicts[0];
        assert_eq!(c.field, FieldName::Author);
        assert_eq!(c.own_value, "Brian Herbert");
        assert_eq!(c.ancestor_value, "Frank Herbert");
        assert_eq!(c.ancestor_id, 0);
    }

    /// Nearest-ancestor-wins across multiple levels: the author comes from
    /// the closest ancestor that has one, not a further shelf.
    #[test]
    fn nearest_ancestor_wins_across_levels() {
        let entries = vec![
            folder(0, None, "Genre - SciFI"),
            folder(1, Some(0), "Jim Butcher - Dresden Files 22GB"),
            folder(2, Some(1), "14.0 - Cold Days [320k 18;48;03 2.52GB]"),
            file(3, Some(2), "14.0 - Cold Days.m4b"),
        ];
        let merged = extract(&entries);

        // The deepest file: own index + title (high), inherited author from
        // the nearest ancestor that has one (Jim Butcher, NOT Genre).
        let f = by_id(&merged, 3);
        assert_eq!(
            f.fields
                .series_index
                .as_ref()
                .map(|x| (x.value.as_str(), x.confidence)),
            Some(("14.0", Confidence::High))
        );
        assert_eq!(
            f.fields.title.as_ref().map(|x| x.value.as_str()),
            Some("Cold Days")
        );
        assert_eq!(
            f.fields
                .author
                .as_ref()
                .map(|x| (x.value.as_str(), x.confidence, x.source)),
            Some(("Jim Butcher", Confidence::Medium, FieldSource::Inherited))
        );
        // No conflict: the file has no own author, so nothing to disagree.
        assert!(f.conflicts.is_empty());
    }

    /// The AC-303.1 degraded/low tier and fabricate-nothing rule: the
    /// "FREE SAMPLE_" outlier keeps its noisy string as a LOW-confidence
    /// derived title and invents no author.
    #[test]
    fn degraded_fallback_title_is_low_and_no_author_is_fabricated() {
        let entries = vec![file(0, None, "FREE SAMPLE_ The Martian by Andy Weir.m4b")];
        let merged = extract(&entries);
        let e = by_id(&merged, 0);

        assert_eq!(
            e.fields
                .title
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence, f.source)),
            Some((
                "FREE SAMPLE_ The Martian by Andy Weir",
                Confidence::Low,
                FieldSource::Derived
            ))
        );
        assert_eq!(e.own_matcher, Some(MatcherId::Pattern8BareTitle));
        // Nothing fabricated.
        assert!(e.fields.author.is_none());
        assert!(e.fields.series.is_none());
    }

    /// Fabricate-nothing at the root: an entry with no parseable field and no
    /// inheritable ancestor yields empty fields, never a guess.
    #[test]
    fn no_context_yields_empty_not_fabricated() {
        // A clean bare-title folder at the root: title only, everything else
        // absent.
        let entries = vec![folder(0, None, "Rework")];
        let merged = extract(&entries);
        let e = by_id(&merged, 0);
        assert_eq!(
            e.fields
                .title
                .as_ref()
                .map(|f| (f.value.as_str(), f.confidence)),
            Some(("Rework", Confidence::High))
        );
        assert!(e.fields.author.is_none());
        assert!(e.fields.series.is_none());
        assert!(e.fields.year.is_none());
        assert!(e.conflicts.is_empty());
    }

    /// The FD-14 hook actually changes the tiers: a non-default weighting
    /// (inherited demoted to low) flows through without touching the merge.
    #[test]
    fn confidence_weights_hook_retunes_the_tiers() {
        let entries = vec![
            folder(0, None, "Frank Herbert-Dune-#1-Chronicles[1-8}"),
            file(1, Some(0), "The Battle of Corrin.m4b"),
        ];
        let weights = ConfidenceWeights {
            own_name: Confidence::High,
            inherited: Confidence::Low, // FD-14 probe verdict demotes inheritance
            derived: Confidence::Low,
        };
        let merged = extract_with_weights(&entries, &weights);
        let member = by_id(&merged, 1);
        assert_eq!(
            member.fields.author.as_ref().map(|f| f.confidence),
            Some(Confidence::Low)
        );
        assert_eq!(
            member.fields.title.as_ref().map(|f| f.confidence),
            Some(Confidence::High)
        );
    }

    /// A dangling parent id is treated as "no parent", never a panic: this
    /// layer does not fabricate structure it was not handed.
    #[test]
    fn dangling_parent_is_treated_as_root() {
        let entries = vec![file(5, Some(999), "The Fifth Season.m4b")];
        let merged = extract(&entries);
        let e = by_id(&merged, 5);
        assert_eq!(
            e.fields.title.as_ref().map(|f| f.value.as_str()),
            Some("The Fifth Season")
        );
        assert!(e.fields.author.is_none());
    }
}
