//! F-704 resolution policies (v0.6.0 P3, AC-23, AC-24, AC-26): given a
//! duplicate group, which copy would be kept.
//!
//! # This module proposes. It never decides, and it never acts.
//!
//! [`propose`] returns a keeper and the copies that would be archived. Nothing
//! here emits an operation, consults a hash, or asks whether the group is
//! allowed to be resolved. That separation is deliberate: permission already
//! has one source of truth in
//! [`group_may_auto_resolve`](super::verify::group_may_auto_resolve) and the
//! `AC-12` gate, and a second opinion living here would drift from it. The
//! caller asks the gate; this answers a different question.
//!
//! `AC-24` is what makes that safe: a non-flag-only policy proposes a keeper and
//! the user confirms before any operation is generated. There is no silent
//! auto-resolution, so a proposal is a suggestion in the literal sense.
//!
//! # Written against BOOKS as well as files (FD-44)
//!
//! The same policy means different things at the two levels, which is exactly
//! why `P2b` was sequenced before this:
//!
//! | Policy | Against files | Against books |
//! |---|---|---|
//! | keep-larger | the bigger file | the bigger copy, sidecars included |
//! | keep-m4b | the `.m4b` | the copy that is one file over the copy that is twelve |
//!
//! # What this module found, and did not paper over
//!
//! **The specced policies discriminate almost nowhere that resolution is
//! actually permitted.** Working through the populations:
//!
//! - An **exact** group is keyed on `(basename, size)`. Equal size means
//!   keep-larger cannot rank it, and equal basename means equal extension, so
//!   keep-m4b cannot either. Both tie, always, by construction of the key.
//! - A **fingerprint** book group requires agreement on audio count and total
//!   audio bytes (`AC-51`), so keep-m4b ties on it by construction too.
//! - The groups where both policies DO discriminate are title-only ones, and
//!   `AC-55` says those never auto-resolve, because choosing between one file
//!   and twelve is a preference rather than a mechanical ranking.
//!
//! So the tie-break is not an edge case; on proven-identical copies it is the
//! de facto policy. It is therefore a first-class, documented, tested rule here
//! rather than an afterthought, and [`KeeperReason::Equivalent`] exists so the
//! confirm surface can say plainly that the policy found nothing to choose
//! between.
//!
//! # Why keep-larger ranks total bytes rather than audio bytes
//!
//! A folder member's `size` is every descendant file, sidecars included
//! ([`dupe_entries_from_plan_nodes`](super::detect::dupe_entries_from_plan_nodes)
//! sums all `File` nodes without filtering by class), while
//! [`BookFolder::audio_bytes`](super::BookFolder) is audio only. Ranking audio
//! bytes would tie on every fingerprint group, since agreeing on them is part of
//! what makes it one. Ranking total bytes instead lets keep-larger prefer the
//! copy that also has the cover art and the chapter file, which is much closer
//! to what someone choosing "keep the larger copy" means.

use super::books::BookFolder;
use super::detect::DuplicateGroup;

/// The three `F-704` policies (`AC-23`). `keep-higher-bitrate` was cut as
/// `F-1108`: file size is a free proxy for it that cannot be missing, and it has
/// no defined value for a book split across N files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolutionPolicy {
    /// The default (`AC-23`). Suggests a keeper and emits nothing (`AC-26`).
    #[default]
    FlagOnly,
    /// Keep the copy with the most bytes.
    KeepLarger,
    /// Prefer the `.m4b`; against books, prefer the copy that is fewer files.
    KeepM4b,
}

/// Why a particular copy was proposed as the keeper.
///
/// An enum rather than a sentence, deliberately. `AC-24` means this reaches a
/// confirm surface, and any sentence written here would be user-facing copy
/// produced by the engine: a fifth producer needing its own vocabulary sweep,
/// in a crate whose sweeps are per-surface. Keeping it an enum leaves the words
/// in `strings.ts`, where the existing sweep already gates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeeperReason {
    /// It has the most bytes, and that was decisive.
    LargerCopy,
    /// It is a `.m4b` and the others are not.
    PreferredFormat,
    /// It is fewer files than the others (`FD-44`'s book-level keep-m4b).
    FewerFiles,
    /// The policy found nothing to choose between: every copy ranked equally
    /// under it, so the keeper is simply the first by path. The common case on
    /// exact groups, and the honest thing to tell a reader.
    Equivalent,
}

/// A proposed resolution: which copy to keep, which to archive, and why.
///
/// Note what is NOT here: any notion of whether this may be acted on. That
/// belongs to the `AC-12` gate and the match tier, and duplicating it here would
/// give permission two sources of truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// `entries.id` of the copy to keep.
    pub keeper: usize,
    /// `entries.id` of every other copy, in the group's own path order.
    pub losers: Vec<usize>,
    /// Why `keeper` won.
    pub reason: KeeperReason,
}

/// Propose a keeper for `group` under `policy`.
///
/// `books` is the book-folder view of the same snapshot, used only to answer
/// "how many audio files is this copy" for the book-level keep-m4b rule. An
/// empty slice is fine and simply means no member resolves to a book.
///
/// Returns `None` for a group of fewer than two members: that is not a duplicate
/// group, and proposing to archive the only copy of a book is the one outcome
/// this product must never produce.
///
/// `FlagOnly` still returns a proposal, because `AC-26` requires the group and
/// a keeper SUGGESTION to be recorded for later review; it simply must not be
/// acted on. It names no criterion, so it reuses keep-larger's ranking as the
/// default heuristic: the bigger copy is the most likely to be the complete one.
pub fn propose(
    policy: ResolutionPolicy,
    group: &DuplicateGroup,
    books: &[BookFolder],
) -> Option<Resolution> {
    if group.members.len() < 2 {
        return None;
    }

    // Members arrive sorted by path, and that order is the tie-break: stable
    // across runs, independent of directory iteration order, and already the
    // order every other surface shows them in.
    let ranked: Vec<(usize, Rank)> = group
        .members
        .iter()
        .map(|m| (m.entry_id, rank(policy, m.entry_id, &m.path, m.size, books)))
        .collect();

    let best = ranked
        .iter()
        .map(|(_, r)| *r)
        .max()
        .expect("len >= 2 checked above");

    // `position`, not `max_by_key`: on a tie this takes the FIRST member at the
    // best rank, which is the first by path. `max_by_key` returns the last.
    let winner = ranked
        .iter()
        .position(|(_, r)| *r == best)
        .expect("best came from this list");

    let decisive = ranked.iter().filter(|(_, r)| *r == best).count() == 1;

    Some(Resolution {
        keeper: ranked[winner].0,
        losers: ranked
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != winner)
            .map(|(_, (id, _))| *id)
            .collect(),
        reason: if decisive {
            best.reason()
        } else {
            KeeperReason::Equivalent
        },
    })
}

/// One member's rank under a policy. Higher wins.
///
/// Comparison is derived, so `Format`'s first field is a TIER and its second is
/// the within-tier score. Ties are detectable rather than merely resolved,
/// because the caller needs to know not just who won but whether anyone actually
/// beat anyone: "these were equivalent" is a different thing to tell a reader
/// than "this one was bigger".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// keep-larger and flag-only: total bytes.
    Bytes(u64),
    /// keep-m4b, as (tier, score within tier), both higher-is-better:
    ///
    /// - tier 2, a `.m4b` file. The literal reading of the policy.
    /// - tier 1, a book folder, scored by INVERTED audio count so fewer files
    ///   ranks higher (`FD-44`).
    /// - tier 0, any other file, scored by size so the tier still has an
    ///   internal order rather than collapsing into one big tie.
    ///
    /// A `.m4b` outranking a book folder is deliberate and consistent: under
    /// keep-m4b, "prefer the .m4b" and "prefer one file over twelve" point the
    /// same way, because a single `.m4b` IS the one-file copy.
    Format(u8, u64),
}

impl Rank {
    fn reason(self) -> KeeperReason {
        match self {
            Rank::Format(2, _) => KeeperReason::PreferredFormat,
            Rank::Format(1, _) => KeeperReason::FewerFiles,
            // Tier 0 means no `.m4b` and no book anywhere in the group, so what
            // actually decided it was size. Saying "fewer files" there would be
            // a reason the reader could check and find false.
            _ => KeeperReason::LargerCopy,
        }
    }
}

fn rank(
    policy: ResolutionPolicy,
    entry_id: usize,
    path: &str,
    size: u64,
    books: &[BookFolder],
) -> Rank {
    match policy {
        // flag-only reuses keep-larger's ranking as its suggestion heuristic.
        ResolutionPolicy::FlagOnly | ResolutionPolicy::KeepLarger => Rank::Bytes(size),
        ResolutionPolicy::KeepM4b => {
            if is_preferred_format(path) {
                Rank::Format(2, size)
            } else if let Some(book) = books.iter().find(|b| b.id == entry_id) {
                Rank::Format(1, u64::MAX - book.audio_count as u64)
            } else {
                Rank::Format(0, size)
            }
        }
    }
}

/// Whether a member's path is the preferred single-file format.
///
/// Free function rather than inline, because "what counts as an m4b" is the kind
/// of thing that grows a second extension later and should have one home.
fn is_preferred_format(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".m4b")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dupes::detect::{DuplicateMember, METHOD_EXACT, METHOD_VERSION};

    fn member(entry_id: usize, path: &str, size: u64) -> DuplicateMember {
        DuplicateMember {
            entry_id,
            path: path.to_string(),
            size,
        }
    }

    fn group(method: &'static str, members: Vec<DuplicateMember>) -> DuplicateGroup {
        DuplicateGroup {
            method,
            group_key: "k".to_string(),
            total_bytes: members.iter().map(|m| m.size).sum(),
            members,
            book_match: None,
            subsumed_by_book_group: false,
        }
    }

    fn book(id: usize, audio_count: usize, audio_bytes: u64) -> BookFolder {
        BookFolder {
            id,
            path: format!("E:\\lib\\book{id}"),
            title_norm: "dune".to_string(),
            audio_count,
            audio_bytes,
            audio_entry_ids: Vec::new(),
            audio_sizes: Vec::new(),
        }
    }

    #[test]
    fn a_group_of_one_has_no_resolution() {
        let g = group(METHOD_EXACT, vec![member(1, "a.m4b", 10)]);
        assert_eq!(propose(ResolutionPolicy::KeepLarger, &g, &[]), None);
    }

    #[test]
    fn keep_larger_prefers_the_bigger_copy() {
        let g = group(
            METHOD_VERSION,
            vec![member(1, "a.mp3", 10), member(2, "b.mp3", 99)],
        );
        let r = propose(ResolutionPolicy::KeepLarger, &g, &[]).unwrap();
        assert_eq!(r.keeper, 2);
        assert_eq!(r.losers, vec![1]);
        assert_eq!(r.reason, KeeperReason::LargerCopy);
    }

    /// The finding this module documents, asserted rather than asserted-about.
    /// An exact group is keyed on (basename, size), so its members ALWAYS agree
    /// on size and extension and neither policy can rank them.
    #[test]
    fn an_exact_group_ties_under_both_policies() {
        let g = group(
            METHOD_EXACT,
            vec![
                member(1, "E:\\a\\Dune.m4b", 500),
                member(2, "E:\\b\\Dune.m4b", 500),
            ],
        );
        for policy in [ResolutionPolicy::KeepLarger, ResolutionPolicy::KeepM4b] {
            let r = propose(policy, &g, &[]).unwrap();
            assert_eq!(r.reason, KeeperReason::Equivalent, "policy {policy:?}");
            // First by path, which is the order members already arrive in.
            assert_eq!(r.keeper, 1, "policy {policy:?}");
        }
    }

    #[test]
    fn keep_m4b_prefers_the_m4b_over_another_format() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(1, "E:\\a\\Dune.mp3", 900),
                member(2, "E:\\b\\Dune.m4b", 100),
            ],
        );
        let r = propose(ResolutionPolicy::KeepM4b, &g, &[]).unwrap();
        assert_eq!(r.keeper, 2, "the .m4b wins even though it is smaller");
        assert_eq!(r.reason, KeeperReason::PreferredFormat);
    }

    /// FD-44's book-level reading of keep-m4b: one file beats twelve. Note this
    /// is the group AC-55 says never auto-resolves; the policy still has an
    /// opinion, and the AC-12 gate is what stops it being acted on silently.
    #[test]
    fn keep_m4b_prefers_one_file_over_twelve_for_books() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(10, "E:\\a\\Dune", 700),
                member(20, "E:\\b\\Dune", 700),
            ],
        );
        let books = vec![book(10, 12, 700), book(20, 1, 700)];
        let r = propose(ResolutionPolicy::KeepM4b, &g, &books).unwrap();
        assert_eq!(r.keeper, 20);
        assert_eq!(r.reason, KeeperReason::FewerFiles);
    }

    /// AC-51's fingerprint requires agreeing audio COUNT, so keep-m4b cannot
    /// rank a fingerprint group. The other half of the module's finding.
    #[test]
    fn keep_m4b_ties_on_a_fingerprint_matched_book_group() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(10, "E:\\a\\Dune", 700),
                member(20, "E:\\b\\Dune", 700),
            ],
        );
        let books = vec![book(10, 12, 700), book(20, 12, 700)];
        let r = propose(ResolutionPolicy::KeepM4b, &g, &books).unwrap();
        assert_eq!(r.reason, KeeperReason::Equivalent);
        assert_eq!(r.keeper, 10, "first by path");
    }

    /// AC-26: flag-only still produces a suggestion. Emitting nothing is the
    /// caller's job, not this module's, so what is asserted here is that the
    /// suggestion exists and matches the documented heuristic.
    #[test]
    fn flag_only_still_suggests_a_keeper() {
        let g = group(
            METHOD_VERSION,
            vec![member(1, "a.mp3", 10), member(2, "b.mp3", 99)],
        );
        let flagged = propose(ResolutionPolicy::FlagOnly, &g, &[]).unwrap();
        let larger = propose(ResolutionPolicy::KeepLarger, &g, &[]).unwrap();
        assert_eq!(flagged, larger);
    }

    #[test]
    fn flag_only_is_the_default_policy() {
        assert_eq!(ResolutionPolicy::default(), ResolutionPolicy::FlagOnly);
    }

    /// Every copy is accounted for exactly once. A policy that dropped a member
    /// would silently leave a duplicate behind; one that listed the keeper among
    /// the losers would archive every copy of the book.
    #[test]
    fn the_keeper_is_never_also_a_loser_and_nothing_is_dropped() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(1, "a.mp3", 10),
                member(2, "b.mp3", 99),
                member(3, "c.mp3", 50),
            ],
        );
        for policy in [
            ResolutionPolicy::FlagOnly,
            ResolutionPolicy::KeepLarger,
            ResolutionPolicy::KeepM4b,
        ] {
            let r = propose(policy, &g, &[]).unwrap();
            assert!(!r.losers.contains(&r.keeper), "policy {policy:?}");
            assert_eq!(r.losers.len(), 2, "policy {policy:?}");
            let mut all = r.losers.clone();
            all.push(r.keeper);
            all.sort_unstable();
            assert_eq!(all, vec![1, 2, 3], "policy {policy:?}");
        }
    }

    /// Tier 0: no `.m4b` and no book folder in the group, so keep-m4b has
    /// nothing of its own to go on and size decides. The reason must say so
    /// rather than claiming a format or a file count that did not apply, since
    /// a reader can check the paths and find it false.
    #[test]
    fn keep_m4b_falls_back_to_size_when_no_copy_is_preferred() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(1, "E:\\a\\Dune.mp3", 10),
                member(2, "E:\\b\\Dune.mp3", 99),
            ],
        );
        let r = propose(ResolutionPolicy::KeepM4b, &g, &[]).unwrap();
        assert_eq!(r.keeper, 2);
        assert_eq!(r.reason, KeeperReason::LargerCopy);
    }

    /// The tier order stated in `Rank`'s docs, asserted. Under keep-m4b a single
    /// `.m4b` IS the one-file copy, so it must outrank a twelve-file book folder
    /// rather than losing to it on some other axis.
    #[test]
    fn keep_m4b_prefers_an_m4b_file_over_a_multi_file_book() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(10, "E:\\a\\Dune", 700),
                member(2, "E:\\b\\Dune.m4b", 100),
            ],
        );
        let books = vec![book(10, 12, 700)];
        let r = propose(ResolutionPolicy::KeepM4b, &g, &books).unwrap();
        assert_eq!(r.keeper, 2);
        assert_eq!(r.reason, KeeperReason::PreferredFormat);
    }

    #[test]
    fn the_preferred_format_check_is_case_insensitive() {
        assert!(is_preferred_format("E:\\lib\\Dune.M4B"));
        assert!(is_preferred_format("E:\\lib\\Dune.m4b"));
        assert!(!is_preferred_format("E:\\lib\\Dune.mp3"));
        assert!(!is_preferred_format("E:\\lib\\m4b.mp3"));
    }
}
