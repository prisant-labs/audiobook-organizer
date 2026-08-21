//! F-701 duplicate candidate detection.
//!
//! [`detect`] is the pure detector: exact basename+size grouping and
//! normalized-title version candidates, with the GROUP as the canonical unit
//! (FD-08). Persistence of detected groups into the `duplicate_groups` /
//! `duplicate_members` tables lives in [`crate::db::dupes`]. This release is
//! candidate-only (no hashing, no auto-quarantine); see the module doc of
//! [`detect`] for the FD-08 counting and byte-figure rules.
//!
//! [`books`] adds the F-1110 book-level comparison on top: a folder group
//! carries a MATCH TIER saying whether its members are two copies of one book or
//! merely two folders with the same name. It raises the tier of a group
//! [`detect`] already found rather than emitting groups of its own, which is
//! what keeps the FD-08 group count honest.

pub mod books;
pub mod detect;
pub mod hash;
pub mod job;
pub mod policy;
pub mod review;
pub mod verify;

pub use verify::{
    book_group_content_matches, group_may_auto_resolve, verify_book_group, verify_groups,
    VerifyOutcome,
};

pub use hash::{
    group_is_verified_identical, hash_member, ContentSource, FsContentSource, MemberHash,
    READ_BUFFER_BYTES,
};

pub use books::{book_folders_from_plan_nodes, match_tier, BookFolder, BookMatch};

pub use policy::{propose, ConfirmedResolution, KeeperReason, Resolution, ResolutionPolicy};

pub use review::{
    build_review, build_review_with_policy, CopyCheck, DuplicateCopy, DuplicateGroupView,
    DuplicatesReview,
};

pub use job::{
    confirm_resolution_gated, ensure_duplicate_groups, review_for_scan, review_view_for_scan,
    verify_scan_duplicates, PersistedDuplicates,
};

pub use job::{
    confirm_resolution_gated, ensure_duplicate_groups, review_for_scan, review_view_for_scan,
    verify_scan_duplicates, PersistedDuplicates,
};

pub use detect::{
    detect_duplicates, detect_exact_duplicates, detect_version_candidates,
    dupe_entries_from_plan_nodes, DupeEntry, DuplicateGroup, DuplicateMember, METHOD_EXACT,
    METHOD_VERSION,
};
