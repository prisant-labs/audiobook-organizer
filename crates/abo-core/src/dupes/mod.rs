//! F-701 duplicate candidate detection.
//!
//! [`detect`] is the pure detector: exact basename+size grouping and
//! normalized-title version candidates, with the GROUP as the canonical unit
//! (FD-08). Persistence of detected groups into the `duplicate_groups` /
//! `duplicate_members` tables lives in [`crate::db::dupes`]. This release is
//! candidate-only (no hashing, no auto-quarantine); see the module doc of
//! [`detect`] for the FD-08 counting and byte-figure rules.

pub mod detect;
pub mod hash;
pub mod verify;

pub use verify::{group_may_auto_resolve, verify_groups, VerifyOutcome};

pub use hash::{group_is_verified_identical, hash_member, ContentSource, MemberHash};

pub use detect::{
    detect_duplicates, detect_exact_duplicates, detect_version_candidates,
    dupe_entries_from_plan_nodes, DupeEntry, DuplicateGroup, DuplicateMember, METHOD_EXACT,
    METHOD_VERSION,
};
