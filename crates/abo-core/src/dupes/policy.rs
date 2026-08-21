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
/// Crosses IPC so the surface can offer the choice (`AC-28`), deriving the same
/// way [`crate::exec::ApplyMode`] does rather than growing a parallel wire enum
/// that could drift from this one.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, specta::Type,
)]
#[serde(rename_all = "kebab-case")]
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
    /// The policy found nothing to choose between at the top: the LEADING
    /// copies ranked equally under it, so the keeper is simply the first of them
    /// by path. Says nothing about copies further down, which may well have
    /// ranked lower. The common case on exact groups, and the honest thing to
    /// tell a reader.
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

/// A resolution the USER has confirmed (`AC-24`), and the only thing that ever
/// causes an Archive operation to be emitted for a duplicate group.
///
/// Separate from [`Resolution`] on purpose. A `Resolution` is what a policy
/// proposed; this is what a person agreed to. `AC-24` forbids silent
/// auto-resolution, so the plan builder accepts only this type, and flag-only
/// satisfies `AC-26` by producing none of them rather than by being special-cased
/// anywhere in the builder.
///
/// # `entry_id`s are per-scan, and that has a consequence for `P5`
///
/// These ids index one snapshot. `FD-39` re-plans from a FRESH scan after an
/// interruption rather than replaying, so ids do not survive a re-scan. Whatever
/// persists confirmations must key them to a `scan_id` and drop them when the
/// scan is superseded; a confirmation carried across a re-scan would archive
/// whatever file happens to hold that id next, which is the worst failure this
/// product can have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedResolution {
    /// `entries.id` of the copy to keep. Carried rather than dropped because the
    /// builder validates against it and the rationale names it.
    pub keeper: usize,
    /// `entries.id` of each copy to move to the Archive.
    pub losers: Vec<usize>,
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

    // The nearest rival: the best rank among everyone else. `Equivalent` when it
    // equals `best`, otherwise it is what the winner actually had to beat, and
    // therefore what the reason must name.
    let rival = ranked
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != winner)
        .map(|(_, (_, r))| *r)
        .max()
        .expect("len >= 2 checked above");

    Some(Resolution {
        keeper: ranked[winner].0,
        losers: ranked
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != winner)
            .map(|(_, (id, _))| *id)
            .collect(),
        reason: if rival == best {
            KeeperReason::Equivalent
        } else {
            discriminating_reason(best, rival)
        },
    })
}

/// One member's rank under a policy. Higher wins, and `Ord` is derived, so
/// field order IS precedence.
///
/// Ties are detectable rather than merely resolved, because the caller needs to
/// know not just who won but whether anyone actually beat anyone: "these were
/// equivalent" is a different thing to tell a reader than "this one was bigger".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// keep-larger and flag-only: total bytes.
    Bytes(u64),
    /// keep-m4b, in precedence order, all higher-is-better:
    ///
    /// 0. `1` for a `.m4b`, `0` otherwise. The literal reading of the policy.
    /// 1. INVERTED file count, so fewer files ranks higher (`FD-44`).
    /// 2. Total bytes, so two copies alike on both still have an order.
    ///
    /// **A plain file counts as ONE file, and that is load-bearing.** An earlier
    /// version ranked "is a book folder" above "is any other file", which made a
    /// twelve-part book beat a single `.mp3` and inverted the exact comparison
    /// `FD-44` asks keep-m4b to make. Being a book is not itself a merit; the
    /// merit is being fewer files, and a lone file is the fewest there is.
    Format(u8, u64, u64),
}

/// Which axis actually decided it, by comparing the winner against its nearest
/// rival.
///
/// Derived from the comparison rather than from the winner's own category. The
/// difference is not cosmetic: two `.m4b` copies of different sizes would
/// otherwise be reported as won on format, and `AC-24` puts that sentence in
/// front of someone who can look at the two paths and see that both end in
/// `.m4b`. A reason a reader can falsify is worse than no reason.
fn discriminating_reason(winner: Rank, rival: Rank) -> KeeperReason {
    match (winner, rival) {
        (Rank::Format(p1, f1, _), Rank::Format(p2, f2, _)) => {
            if p1 != p2 {
                KeeperReason::PreferredFormat
            } else if f1 != f2 {
                KeeperReason::FewerFiles
            } else {
                KeeperReason::LargerCopy
            }
        }
        // keep-larger and flag-only have exactly one axis. Mixed variants cannot
        // occur: one policy ranks every member of a call.
        _ => KeeperReason::LargerCopy,
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
            let preferred = u8::from(is_preferred_format(path));
            // A member that is not a known book folder is a single file.
            let files = books
                .iter()
                .find(|b| b.id == entry_id)
                .map(|b| b.audio_count as u64)
                .unwrap_or(1);
            Rank::Format(preferred, u64::MAX - files, size)
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

    /// The reason must name the axis that ACTUALLY decided it, not the winner's
    /// category. Two `.m4b` copies of different sizes: `PreferredFormat` would
    /// read "it is a .m4b and the others are not", which the reader can check
    /// against the paths and find false. Size decided, so size is the reason.
    #[test]
    fn two_m4bs_of_different_sizes_are_decided_by_size_not_by_format() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(1, "E:\\a\\Dune.m4b", 900),
                member(2, "E:\\b\\Dune.m4b", 100),
            ],
        );
        let r = propose(ResolutionPolicy::KeepM4b, &g, &[]).unwrap();
        assert_eq!(r.keeper, 1);
        assert_eq!(r.reason, KeeperReason::LargerCopy);
    }

    /// A lone file IS a one-file copy, so `FD-44`'s "prefer the copy that is one
    /// file over the copy that is twelve" must prefer it over a twelve-file
    /// book. Ranking books above plain files would have inverted exactly the
    /// comparison the policy exists to make.
    #[test]
    fn keep_m4b_prefers_a_single_file_over_a_twelve_file_book() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(10, "E:\\a\\Dune", 700),
                member(2, "E:\\b\\Dune.mp3", 100),
            ],
        );
        let books = vec![book(10, 12, 700)];
        let r = propose(ResolutionPolicy::KeepM4b, &g, &books).unwrap();
        assert_eq!(r.keeper, 2, "one file beats twelve, even as an .mp3");
        assert_eq!(r.reason, KeeperReason::FewerFiles);
    }

    /// `Equivalent` means the LEADING copies tied, not that every copy did. A
    /// third, smaller copy losing does not make the top two distinguishable.
    #[test]
    fn a_tie_at_the_top_is_equivalent_even_when_a_third_copy_loses() {
        let g = group(
            METHOD_VERSION,
            vec![
                member(1, "E:\\a\\Dune.mp3", 99),
                member(2, "E:\\b\\Dune.mp3", 99),
                member(3, "E:\\c\\Dune.mp3", 10),
            ],
        );
        let r = propose(ResolutionPolicy::KeepLarger, &g, &[]).unwrap();
        assert_eq!(r.reason, KeeperReason::Equivalent);
        assert_eq!(r.keeper, 1, "first by path among the tied leaders");
        assert_eq!(r.losers, vec![2, 3]);
    }

    #[test]
    fn the_preferred_format_check_is_case_insensitive() {
        assert!(is_preferred_format("E:\\lib\\Dune.M4B"));
        assert!(is_preferred_format("E:\\lib\\Dune.m4b"));
        assert!(!is_preferred_format("E:\\lib\\Dune.mp3"));
        assert!(!is_preferred_format("E:\\lib\\m4b.mp3"));
    }
}
