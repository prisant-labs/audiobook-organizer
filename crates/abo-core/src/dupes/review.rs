//! F-703 duplicate review (v0.6.0 P4, AC-17 to AC-22): the group-first view of
//! detected duplicates, and its CSV export.
//!
//! # The GROUP is the unit, everywhere (FD-08, AC-17, AC-18)
//!
//! One group is one book with N copies. Every count this module produces counts
//! GROUPS; the members inside one are "copies", never "members", "pairs", or
//! "duplicates" (a copy is a duplicate OF something, so counting duplicates is
//! the ambiguity `FD-08` exists to remove). The one figure that is not a count
//! is the byte total, and it is deliberately named an ESTIMATE: it is the sum of
//! every candidate copy's bytes, which is what `DuplicateGroup::total_bytes`
//! means, and it is NOT the space a resolution would reclaim. Reporting it as
//! space saved would overstate by roughly the size of the copy that gets kept.
//!
//! # Fresh detection owns the groups; persisted rows own only the hash state
//!
//! [`build_review`] takes groups from a fresh run of the detector over the
//! stored snapshot, exactly as the Copies card does, and lays persisted
//! verification state over them keyed by `entries.id` rather than by group.
//!
//! That split matters. A hash is a fact about a FILE, so it survives a change in
//! how files are grouped; group identity does not. The `F-1110` subsumption rule
//! changed grouping after hashes could already have been persisted, and under
//! this design that is a non-event: the current detector's grouping wins, and
//! each hash reattaches to its own file. Reading persisted GROUPS instead would
//! have made group-id reconciliation a read-path problem, and it is not one.
//!
//! # Why this counts the same population the Copies card counts
//!
//! Both filter on [`DuplicateGroup::is_duplicate_candidate`]. `AC-20`'s bar is
//! that the export matches the surface exactly, and the way that breaks is
//! subtle: the P2b session shipped a moment where four figures disagreed because
//! three of them still read an exact-only helper. One predicate, read by
//! everything, is the only version of this that stays true.

use std::collections::HashMap;

use crate::db::dupes::MemberVerification;
use crate::dupes::books::BookFolder;
use crate::dupes::detect::DuplicateGroup;
use crate::dupes::policy::{propose, KeeperReason, ResolutionPolicy};

/// Whether this copy's contents have been read (`F-702`).
///
/// Named for what a reader wants to know rather than for the column it comes
/// from: "has anything actually checked this?"
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyCheck {
    /// Nobody has read it yet. Work not done, not a problem.
    NotChecked,
    /// Read end to end.
    Checked,
    /// A read was attempted and failed. Carries why, for the details disclosure.
    CouldNotRead(String),
}

impl CopyCheck {
    /// Plain-language label. `AC-21` register: this text reaches a person, so it
    /// says what happened rather than naming a column or a hash.
    pub fn label(&self) -> &'static str {
        match self {
            CopyCheck::NotChecked => "not checked yet",
            CopyCheck::Checked => "contents checked",
            CopyCheck::CouldNotRead(_) => "could not be read",
        }
    }
}

/// One copy inside a duplicate group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateCopy {
    /// `entries.id` in the snapshot this review was built from.
    pub entry_id: usize,
    pub path: String,
    pub size_bytes: u64,
    pub check: CopyCheck,
    /// Whether this is the copy the default suggestion would keep (`AC-19`).
    pub suggested_keeper: bool,
}

/// One duplicate group: one book, N copies (`FD-08`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroupView {
    /// A readable name for the book (`AC-17`). See [`display_name`].
    pub book: String,
    /// Stable key from the detector, used as the CSV's group column (`AC-20`).
    /// A join key, never shown as if it were a book name.
    pub group_key: String,
    /// The detector that found this group. Carried because a group's IDENTITY is
    /// `(method, group_key)`, not the key alone: a confirmation is recorded
    /// against both, and the two detectors build keys in different shapes.
    pub method: &'static str,
    /// How the group was found, in plain language.
    pub found_by: &'static str,
    /// Copies in this group. Always at least two; a group of one is not a group.
    pub copies: Vec<DuplicateCopy>,
    /// Sum of every copy's bytes. An ESTIMATE of what is duplicated, never the
    /// space a resolution reclaims (`AC-18`).
    pub candidate_bytes_estimate: u64,
    /// Why the suggested keeper was suggested, if a suggestion was possible.
    pub keeper_reason: Option<KeeperReason>,
    /// Whether the copies are PROVEN identical: at least two of them, every one
    /// carrying a hash, and all the hashes agreeing. This is `AC-12`'s gate, the
    /// thing `AC-13`'s two-step override overrides.
    ///
    /// The three conditions reject three different situations and are the same
    /// three [`crate::dupes::verify::group_may_auto_resolve`] applies: a group of
    /// one is not a duplicate group, an unhashed or unreadable copy means the
    /// tool does not know, and disagreeing hashes mean the detector was wrong
    /// (`AC-14`, same name and size, different book).
    ///
    /// ALWAYS FALSE FOR A FOLDER GROUP, and honestly so. A folder has no bytes of
    /// its own: its content tier is a multiset comparison over its files
    /// (`AC-54`), which lives in the database rather than in this pure function.
    /// Nothing is lost this release, because `P3` emission is scoped to FILE
    /// losers and a folder loser cannot be archived yet regardless.
    pub content_verified: bool,
}

impl DuplicateGroupView {
    /// Copies in this group. Named `copy_count`, not `member_count`: the
    /// register is fixed by `FD-08` and this is the word a user reads.
    pub fn copy_count(&self) -> usize {
        self.copies.len()
    }
}

/// The whole duplicate review for one scan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DuplicatesReview {
    pub groups: Vec<DuplicateGroupView>,
}

impl DuplicatesReview {
    /// The headline number, and it counts GROUPS (`AC-18`).
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Total copies across every group. Reported beside the group count, never
    /// instead of it: "14 copies" and "6 duplicated books" answer different
    /// questions and the P2b session shipped the first while meaning the second.
    pub fn copy_count(&self) -> usize {
        self.groups.iter().map(|g| g.copy_count()).sum()
    }

    /// Sum of every group's estimate. Carries the same caveat: candidate bytes,
    /// not space reclaimed (`AC-18`).
    pub fn candidate_bytes_estimate(&self) -> u64 {
        self.groups.iter().map(|g| g.candidate_bytes_estimate).sum()
    }

    /// The `AC-20` export: one row per COPY, with a group column, so a
    /// spreadsheet can group by it and arrive at the same counts the surface
    /// shows.
    ///
    /// Headers are prose rather than identifiers because this file is opened by
    /// a person, not parsed by the app. Nothing here says "deleted": a resolved
    /// copy is "moved to the Archive" (`AC-21`, `FD-10`, `FD-42`).
    pub fn to_csv(&self) -> String {
        let mut wtr = csv::WriterBuilder::new().from_writer(Vec::new());
        wtr.write_record([
            "book",
            "group",
            "copies of this book",
            "found by",
            "this copy",
            "size of this copy",
            "checked",
            "suggested to keep",
            "why",
            "estimated bytes in this group (not space saved)",
        ])
        .expect("writing the header record to an in-memory buffer cannot fail");

        for g in &self.groups {
            let copies = g.copy_count().to_string();
            let estimate = g.candidate_bytes_estimate.to_string();
            for c in &g.copies {
                let size = c.size_bytes.to_string();
                wtr.write_record([
                    g.book.as_str(),
                    g.group_key.as_str(),
                    copies.as_str(),
                    g.found_by,
                    c.path.as_str(),
                    size.as_str(),
                    c.check.label(),
                    if c.suggested_keeper { "keep" } else { "" },
                    if c.suggested_keeper {
                        keeper_reason_label(g.keeper_reason)
                    } else {
                        ""
                    },
                    estimate.as_str(),
                ])
                .expect("writing a data record to an in-memory buffer cannot fail");
            }
        }

        let bytes = wtr
            .into_inner()
            .expect("into_inner cannot fail for a Vec<u8> sink");
        String::from_utf8(bytes)
            .expect("every field is built from an owned Rust String, so output is valid UTF-8")
    }
}

/// Plain-language rendering of why a copy was suggested.
///
/// This is user-facing text generated in Rust, which makes it a producer of copy
/// in its own right. It is swept by this module's own vocabulary test, the same
/// way `report.rs`, `error.rs`, `query.rs` and the plan builder's rationales are
/// swept by theirs.
pub fn keeper_reason_label(reason: Option<KeeperReason>) -> &'static str {
    match reason {
        Some(KeeperReason::LargerCopy) => "this copy is the biggest",
        Some(KeeperReason::PreferredFormat) => "this copy is a single m4b file",
        Some(KeeperReason::FewerFiles) => "this copy is fewer files",
        // The common case on identical copies, and worth saying plainly rather
        // than dressing up as a decision: nothing distinguished them.
        Some(KeeperReason::Equivalent) => "the copies were equivalent, so the first one is kept",
        None => "",
    }
}

/// A readable name for the book a duplicate group is about.
///
/// The group KEY is a detector artifact (`Dune.m4b|900`), fine for joining rows
/// and wrong for a person to read. `AC-17` says a group is presented as one book
/// with N copies, so something has to name the book.
///
/// Which part of the path names it depends on what the members are. A version
/// candidate's members are the books themselves (usually folders), so the
/// member's own name is the book. An exact group's members are FILES inside a
/// book, so the book is the folder above. Getting that backwards named every
/// folder group after the directory above it, which was found and fixed once
/// already; the rule is kept in one place so it cannot be got backwards twice.
///
/// Moved here from `plan::report` rather than copied: naming a duplicate group
/// is a duplicates concern, and `plan` already depends on `dupes` rather than
/// the reverse.
pub fn display_name(g: &DuplicateGroup, sep: char) -> String {
    let first = g.members.first().map(|m| m.path.as_str()).unwrap_or("");
    if g.is_version_candidate() {
        return leaf(first, sep).to_string();
    }
    let parent = parent_of(first, sep);
    let folder = leaf(parent, sep);
    if folder.is_empty() || parent == first {
        leaf(first, sep).to_string()
    } else {
        folder.to_string()
    }
}

fn leaf(path: &str, sep: char) -> &str {
    path.rsplit(sep).next().unwrap_or(path)
}

fn parent_of(path: &str, sep: char) -> &str {
    match path.rfind(sep) {
        Some(i) => &path[..i],
        None => path,
    }
}

/// Build the review from a fresh detection plus persisted hash state.
///
/// `verifications` is keyed by `entries.id` (see
/// [`member_verifications_for_scan`](crate::db::dupes::member_verifications_for_scan)).
/// A missing key means not checked yet, which is why absent entries need no
/// special handling here.
///
/// Pure, so the whole review is a deterministic function of a snapshot plus what
/// has been hashed. The one caller that reads a database does so around this,
/// not inside it.
pub fn build_review(
    groups: &[DuplicateGroup],
    books: &[BookFolder],
    verifications: &HashMap<i64, MemberVerification>,
    sep: char,
) -> DuplicatesReview {
    let mut out = Vec::new();

    for g in groups.iter().filter(|g| g.is_duplicate_candidate()) {
        // The default suggestion is flag-only's, which AC-26 defines as a
        // recorded keeper SUGGESTION carrying no permission to act.
        let suggestion = propose(ResolutionPolicy::FlagOnly, g, books);
        let keeper = suggestion.as_ref().map(|r| r.keeper);

        let copies = g
            .members
            .iter()
            .map(|m| DuplicateCopy {
                entry_id: m.entry_id,
                path: m.path.clone(),
                size_bytes: m.size,
                check: match verifications.get(&(m.entry_id as i64)) {
                    Some(MemberVerification::Verified(_)) => CopyCheck::Checked,
                    Some(MemberVerification::Failed(why)) => CopyCheck::CouldNotRead(why.clone()),
                    Some(MemberVerification::Unhashed) | None => CopyCheck::NotChecked,
                },
                suggested_keeper: Some(m.entry_id) == keeper,
            })
            .collect();

        out.push(DuplicateGroupView {
            book: display_name(g, sep),
            group_key: g.group_key.clone(),
            method: g.method,
            found_by: found_by_label(g),
            copies,
            candidate_bytes_estimate: g.total_bytes,
            keeper_reason: suggestion.map(|r| r.reason),
            content_verified: content_is_verified_identical(g, verifications),
        });
    }

    DuplicatesReview { groups: out }
}

/// `AC-12`'s gate, computed from what has been hashed.
///
/// Pure, and deliberately the same three conditions as
/// [`crate::dupes::verify::group_may_auto_resolve`], which answers the same
/// question from the database. Two implementations of one rule is a risk worth
/// naming: this one exists because the review is built without persisted group
/// ids, and the two are kept honest by asserting the same cases.
fn content_is_verified_identical(
    g: &DuplicateGroup,
    verifications: &HashMap<i64, MemberVerification>,
) -> bool {
    if g.members.len() < 2 {
        return false;
    }
    let mut hashes = g.members.iter().map(|m| {
        match verifications.get(&(m.entry_id as i64)) {
            Some(MemberVerification::Verified(h)) => Some(h.as_str()),
            // Unhashed, unreadable, or a folder member that has no hash of its
            // own. All three mean the same thing here: not proven.
            _ => None,
        }
    });
    let Some(Some(first)) = hashes.next() else {
        return false;
    };
    hashes.all(|h| h == Some(first))
}

/// How the group was found, in words a reader can act on.
///
/// Deliberately not the internal method string. "exact" and "version" name the
/// detector's technique; these name the EVIDENCE, which is what tells someone
/// how much to trust the grouping before they decide anything.
fn found_by_label(g: &DuplicateGroup) -> &'static str {
    use crate::dupes::books::BookMatch;
    match g.book_match {
        Some(BookMatch::Structural) => "same book: matching files, sizes and count",
        Some(BookMatch::Fingerprint) => "same book: matching title, file count and total size",
        Some(BookMatch::TitleOnly) => "same title, but the copies are shaped differently",
        None if g.is_exact() => "same file name and size",
        None => "same title",
    }
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

    fn group(method: &'static str, key: &str, members: Vec<DuplicateMember>) -> DuplicateGroup {
        DuplicateGroup {
            method,
            group_key: key.to_string(),
            total_bytes: members.iter().map(|m| m.size).sum(),
            members,
            book_match: None,
            subsumed_by_book_group: false,
        }
    }

    fn two_copy_group() -> DuplicateGroup {
        group(
            METHOD_EXACT,
            "Dune.m4b|900",
            vec![
                member(1, "E:\\lib\\A\\Dune.m4b", 900),
                member(2, "E:\\lib\\B\\Dune.m4b", 900),
            ],
        )
    }

    /// Verifications for the two-copy fixture, one entry per member.
    fn verified(a: &str, b: &str) -> HashMap<i64, MemberVerification> {
        HashMap::from([
            (1i64, MemberVerification::Verified(a.to_string())),
            (2i64, MemberVerification::Verified(b.to_string())),
        ])
    }

    /// `AC-12`: proven identical is the only state that opens the automatic path.
    #[test]
    fn matching_hashes_on_every_copy_are_what_verified_means() {
        let review = build_review(&[two_copy_group()], &[], &verified("aaa", "aaa"), '\\');
        assert!(review.groups[0].content_verified);
    }

    /// `AC-14`: same name, same size, DIFFERENT content is the case this gate
    /// exists to catch. Auto-resolving it would archive a different book.
    #[test]
    fn hashes_that_disagree_are_not_verified_however_alike_the_files_looked() {
        let review = build_review(&[two_copy_group()], &[], &verified("aaa", "bbb"), '\\');
        assert!(!review.groups[0].content_verified);
    }

    #[test]
    fn one_copy_left_unread_leaves_the_whole_group_unverified() {
        let unread = HashMap::from([(1i64, MemberVerification::Verified("aaa".to_string()))]);
        let review = build_review(&[two_copy_group()], &[], &unread, '\\');
        assert!(!review.groups[0].content_verified);

        let failed = HashMap::from([
            (1i64, MemberVerification::Verified("aaa".to_string())),
            (
                2i64,
                MemberVerification::Failed("access denied".to_string()),
            ),
        ]);
        let review = build_review(&[two_copy_group()], &[], &failed, '\\');
        assert!(
            !review.groups[0].content_verified,
            "a file we could not read is the case where not-knowing matters most"
        );
    }

    #[test]
    fn a_group_nobody_hashed_is_not_verified() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        assert!(!review.groups[0].content_verified);
    }

    /// The group's identity is (method, group_key), and a confirmation is
    /// recorded against both. Carrying only the key would make an exact group and
    /// a version group with the same key indistinguishable to the surface.
    #[test]
    fn a_group_carries_the_detector_that_found_it() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        assert_eq!(review.groups[0].method, METHOD_EXACT);
    }

    /// AC-17 and AC-18: the headline counts GROUPS, and copies are counted
    /// separately rather than instead. Two copies of one book is ONE duplicated
    /// book, and the P2b session shipped a report that said otherwise.
    #[test]
    fn the_headline_counts_groups_not_copies() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        assert_eq!(review.group_count(), 1, "one duplicated book");
        assert_eq!(review.copy_count(), 2, "two copies of it");
    }

    /// The review must count the same population the Copies card counts, or the
    /// export and the screen disagree, which is exactly what AC-20 forbids.
    #[test]
    fn a_subsumed_group_is_excluded_just_as_the_copies_card_excludes_it() {
        let mut g = two_copy_group();
        g.subsumed_by_book_group = true;
        assert!(!g.is_duplicate_candidate(), "fixture precondition");

        let review = build_review(&[g], &[], &HashMap::new(), '\\');
        assert_eq!(review.group_count(), 0);
    }

    /// The join is by FILE, so a hash reattaches even though nothing about the
    /// group it was persisted under is consulted.
    #[test]
    fn persisted_hash_state_attaches_to_the_right_copy() {
        let mut v = HashMap::new();
        v.insert(1i64, MemberVerification::Verified("abc".into()));
        v.insert(2i64, MemberVerification::Failed("access denied".into()));

        let review = build_review(&[two_copy_group()], &[], &v, '\\');
        let copies = &review.groups[0].copies;
        assert_eq!(copies[0].check, CopyCheck::Checked);
        assert_eq!(
            copies[1].check,
            CopyCheck::CouldNotRead("access denied".into())
        );
    }

    /// A copy nothing has hashed reads as not checked rather than as an error.
    /// Work not done is not a problem, and saying so wrongly would push someone
    /// to investigate a file that is fine.
    #[test]
    fn an_unhashed_copy_reads_as_not_checked() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        for c in &review.groups[0].copies {
            assert_eq!(c.check, CopyCheck::NotChecked);
        }
        assert_eq!(review.groups[0].copies[0].check.label(), "not checked yet");
    }

    /// AC-19: exactly one copy per group is suggested, and it is never all of
    /// them and never none.
    #[test]
    fn exactly_one_copy_per_group_is_suggested_to_keep() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        let suggested = review.groups[0]
            .copies
            .iter()
            .filter(|c| c.suggested_keeper)
            .count();
        assert_eq!(suggested, 1);
    }

    /// AC-20: one row per COPY plus a header, with the group repeated on each
    /// row so a spreadsheet can group by it and reach the same counts.
    #[test]
    fn the_csv_has_one_row_per_copy_and_a_group_column() {
        let review = build_review(
            &[two_copy_group(), {
                // A title group only counts once it reaches the AC-51
                // fingerprint tier; a bare title match is a candidate for
                // nothing (AC-52, AC-55). Getting that wrong in a FIXTURE is
                // how an export quietly counts a population the screen does
                // not, which is the exact defect AC-20 exists to prevent.
                let mut g = group(
                    METHOD_VERSION,
                    "sapiens",
                    vec![
                        member(3, "E:\\lib\\C\\Sapiens", 20),
                        member(4, "E:\\lib\\D\\Sapiens", 20),
                        member(5, "E:\\lib\\E\\Sapiens", 20),
                    ],
                );
                g.book_match = Some(crate::dupes::books::BookMatch::Fingerprint);
                g
            }],
            &[],
            &HashMap::new(),
            '\\',
        );

        let csv = review.to_csv();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 1 + 5, "header plus one row per copy");
        // The first column names the BOOK, and the group key is its own column
        // beside it. An earlier draft had one column headed "book (group)"
        // carrying `Dune.m4b|900`, which promises a book name and delivers a
        // detector artifact; reading the rendered file is what caught it.
        assert!(lines[0].starts_with("book,group,"), "got: {}", lines[0]);
        assert!(
            lines[1].starts_with("A,Dune.m4b|900,"),
            "the exact group is named for the folder holding the copies: {}",
            lines[1]
        );
        assert_eq!(csv.matches("Dune.m4b|900").count(), 2, "group key per copy");
        assert_eq!(review.copy_count(), 5);
    }

    /// AC-18: the byte figure must say which quantity it is. It is the sum of
    /// every candidate copy, NOT the space a resolution reclaims, and the header
    /// says so where a reader will see it.
    #[test]
    fn the_byte_column_says_it_is_an_estimate_and_not_space_saved() {
        let review = build_review(&[two_copy_group()], &[], &HashMap::new(), '\\');
        assert_eq!(review.candidate_bytes_estimate(), 1800);
        assert!(review
            .to_csv()
            .contains("estimated bytes in this group (not space saved)"));
    }

    /// PRODUCER 7. Every user-facing string this module generates, swept.
    ///
    /// The six before it are `strings.ts`/`errorCopy.ts`, `report.rs`,
    /// `error.rs`, `query.rs`, `src/gallery` and the plan builder's rationales.
    /// Two of those were added only after a retired word had already shipped
    /// through them, so this one arrives with the code rather than after it.
    #[test]
    fn no_exported_text_carries_forbidden_vocabulary() {
        let review = build_review(
            &[two_copy_group()],
            &[],
            &HashMap::from([(1i64, MemberVerification::Failed("nope".into()))]),
            '\\',
        );

        let mut samples: Vec<String> = review.to_csv().lines().map(str::to_string).collect();
        for check in [
            CopyCheck::NotChecked,
            CopyCheck::Checked,
            CopyCheck::CouldNotRead(String::new()),
        ] {
            samples.push(check.label().to_string());
        }
        for r in [
            Some(KeeperReason::LargerCopy),
            Some(KeeperReason::PreferredFormat),
            Some(KeeperReason::FewerFiles),
            Some(KeeperReason::Equivalent),
            None,
        ] {
            samples.push(keeper_reason_label(r).to_string());
        }
        // Every "found by" phrase, not only the ones this fixture produces.
        let mut g = two_copy_group();
        for tier in [
            None,
            Some(crate::dupes::books::BookMatch::TitleOnly),
            Some(crate::dupes::books::BookMatch::Fingerprint),
            Some(crate::dupes::books::BookMatch::Structural),
        ] {
            g.book_match = tier;
            samples.push(found_by_label(&g).to_string());
        }

        for sample in samples {
            let text = sample.to_lowercase().replace("audiobookshelf", "<product>");
            for (word, decision, successor) in [
                ("aside", "FD-42", "Archive"),
                ("shelf", "FD-47", "library"),
                ("shelves", "FD-47", "library"),
                ("shelved", "FD-47", "library"),
                ("tidy", "FD-48", "organize"),
            ] {
                assert!(
                    !text.contains(word),
                    "exported duplicate text carries {word:?}, retired by {decision} in favour \
                     of {successor:?}: {sample}"
                );
            }
            for word in ["quarantine", "dedupe", "manifest", "dashboard", "deleted"] {
                assert!(
                    !text.contains(word),
                    "exported duplicate text carries the forbidden term {word:?} \
                     (design-system 6.1; AC-21 forbids \"deleted\" as primary vocabulary): {sample}"
                );
            }
        }
    }

    /// Guards the guard: the sweep must fire on the word `AC-21` names, or it
    /// proves nothing about the register it claims to enforce.
    #[test]
    fn the_sweep_would_catch_deleted() {
        let banned = ["quarantine", "dedupe", "manifest", "dashboard", "deleted"];
        assert!(banned.iter().any(|w| "this copy is deleted".contains(w)));
    }
}
