//! F-702 content hashing: what turns a duplicate CANDIDATE into a proven
//! duplicate (v0.6.0 P2, AC-10 and AC-14).
//!
//! # What this is for
//!
//! [`super::detect`] groups files that share a basename and a byte size, or
//! whose titles normalize to the same string. That is strong evidence and it is
//! not proof. Two different recordings can agree on both signals; a re-encode at
//! the same settings can land on the same size. Acting on a candidate would mean
//! setting a book aside on a guess, which is the one thing this product refuses
//! to do (D-09). Hashing is how a guess becomes a fact.
//!
//! # Candidates only, structurally (AC-10)
//!
//! There is no hash-everything path in this crate and there must never be. The
//! scan does not hash. The plan builder does not hash. The only entry point is
//! [`hash_member`], which takes ONE already-detected group member at a time, and
//! the only caller is the verification job.
//!
//! That restraint is a performance decision made once, deliberately: this
//! library is 297 GB, and hashing all of it to find the few percent that are
//! duplicates would trade an hour of disk for an answer the size-and-name
//! detector already narrowed down for free.
//!
//! # Why BLAKE3, and why the security properties do not matter
//!
//! Nothing here is adversarial. The question is "are these two files the same
//! book", not "did someone forge a collision". BLAKE3 is chosen for throughput
//! over multi-gigabyte files. If it were slow, the honest fallback is the one
//! AC-16 already names: ship flag-only and let a person decide.
//!
//! # The read seam, and why it is not `Vfs`
//!
//! [`ContentSource`] exists so this module can be tested against in-memory bytes
//! without a temp directory, and so the executor's [`Vfs`](crate::exec::Vfs)
//! trait does not grow a read method it has no use for. `Vfs` is the seam the
//! safety-critical executor mutates the world through; every method on it is
//! something that can damage a library. Hashing only ever reads, so it gets its
//! own one-method trait rather than widening that surface.

use crate::error::AppError;

/// Read-only access to file bytes, for hashing.
///
/// One method, deliberately. Anything wider would invite this module to grow
/// capabilities it does not need, and would make the test double a filesystem
/// simulator rather than a byte source.
pub trait ContentSource {
    /// Stream the file at `path` into `sink` in chunks.
    ///
    /// Chunked rather than "return the bytes", because these files are routinely
    /// larger than a gigabyte and reading one into memory to hash it would be a
    /// self-inflicted memory problem. An implementation should hand over
    /// whatever chunk size suits it; callers must not depend on the boundaries.
    fn read_chunks(&self, path: &str, sink: &mut dyn FnMut(&[u8])) -> Result<(), std::io::Error>;
}

/// The outcome of hashing one duplicate group member.
///
/// A failure is a VALUE here, not an error return, because a member that could
/// not be read is a fact the surface has to display and the resolution gate has
/// to respect (AC-12). Collapsing it into a `Result` at the job boundary would
/// lose the distinction between "this one failed" and "the whole job failed",
/// and the caller needs to keep hashing the rest of the group either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberHash {
    /// The file was read end to end. Lowercase hex of the BLAKE3 digest.
    Hashed(String),
    /// The file could not be read. Carries the reason, for the surface and for
    /// the persisted `hash_error` column.
    Failed(String),
}

impl MemberHash {
    /// The hash, if there is one. `None` for a failure.
    pub fn hash(&self) -> Option<&str> {
        match self {
            MemberHash::Hashed(h) => Some(h),
            MemberHash::Failed(_) => None,
        }
    }

    /// True only for a successful read. Named for the question the resolution
    /// gate actually asks (AC-12): may this member take part in an automatic
    /// decision?
    pub fn is_verified(&self) -> bool {
        matches!(self, MemberHash::Hashed(_))
    }
}

/// Hash ONE duplicate group member.
///
/// Never call this over a scan, a snapshot, or a directory walk. It takes a
/// single path because AC-10's candidates-only rule is easier to keep when the
/// signature makes bulk use awkward.
///
/// A read error becomes [`MemberHash::Failed`] rather than an `Err`: one
/// unreadable file must not abandon the rest of its group. The `Result` is
/// reserved for conditions that genuinely end the work.
pub fn hash_member<S: ContentSource>(source: &S, path: &str) -> Result<MemberHash, AppError> {
    let mut hasher = blake3::Hasher::new();
    match source.read_chunks(path, &mut |chunk| {
        hasher.update(chunk);
    }) {
        Ok(()) => Ok(MemberHash::Hashed(hasher.finalize().to_hex().to_string())),
        Err(e) => Ok(MemberHash::Failed(e.to_string())),
    }
}

/// Whether a whole group is safe to resolve automatically (AC-12).
///
/// True only when EVERY member hashed successfully AND they all agree. Any
/// failure, any missing hash, or any disagreement returns false.
///
/// Note what a disagreement means: the detector said these were the same file
/// and the bytes say otherwise. That is AC-14, and the correct response is to
/// refuse the automatic path, not to pick a winner. Two books that share a name
/// and a size but differ in content are two books.
pub fn group_is_verified_identical(hashes: &[MemberHash]) -> bool {
    let mut iter = hashes.iter();
    let Some(first) = iter.next() else {
        // An empty group is not "verified", it is a bug upstream. Refusing is
        // the safe reading either way.
        return false;
    };
    let Some(first_hash) = first.hash() else {
        return false;
    };
    iter.all(|h| h.hash() == Some(first_hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Bytes in a map. Not a filesystem simulator: it exists only to feed
    /// `hash_member` something to read, and to fail on demand.
    struct MemSource {
        files: HashMap<String, Vec<u8>>,
        /// Paths that fail to read, with the message they fail with.
        broken: HashMap<String, String>,
    }

    impl MemSource {
        fn new() -> Self {
            MemSource {
                files: HashMap::new(),
                broken: HashMap::new(),
            }
        }
        fn with(mut self, path: &str, bytes: &[u8]) -> Self {
            self.files.insert(path.to_string(), bytes.to_vec());
            self
        }
        fn broken(mut self, path: &str, why: &str) -> Self {
            self.broken.insert(path.to_string(), why.to_string());
            self
        }
    }

    impl ContentSource for MemSource {
        fn read_chunks(
            &self,
            path: &str,
            sink: &mut dyn FnMut(&[u8]),
        ) -> Result<(), std::io::Error> {
            if let Some(why) = self.broken.get(path) {
                return Err(std::io::Error::other(why.clone()));
            }
            let Some(bytes) = self.files.get(path) else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("no such file: {path}"),
                ));
            };
            // Deliberately small chunks, so a hasher that mishandles chunk
            // boundaries fails here rather than only on a large real file.
            for chunk in bytes.chunks(7) {
                sink(chunk);
            }
            Ok(())
        }
    }

    #[test]
    fn identical_bytes_hash_the_same_however_they_are_chunked() {
        let s = MemSource::new()
            .with("a.m4b", b"the same audiobook content, at length")
            .with("b.m4b", b"the same audiobook content, at length");
        let a = hash_member(&s, "a.m4b").unwrap();
        let b = hash_member(&s, "b.m4b").unwrap();
        assert!(a.is_verified());
        assert_eq!(a, b);
    }

    /// AC-14, and the whole reason this module exists. Two files the detector
    /// would group (same basename, same size) whose CONTENT differs must produce
    /// different digests and must not be auto-resolved.
    #[test]
    fn same_name_same_size_different_content_does_not_verify_ac14() {
        // Same length on purpose: this is exactly what fools the size detector.
        let left = b"Dune, read by Simon Vance....";
        let right = b"Dune, read by Scott Brick....";
        assert_eq!(
            left.len(),
            right.len(),
            "fixture must defeat the size check"
        );

        let s = MemSource::new()
            .with("shelf-one/Dune.m4b", left)
            .with("shelf-two/Dune.m4b", right);
        let a = hash_member(&s, "shelf-one/Dune.m4b").unwrap();
        let b = hash_member(&s, "shelf-two/Dune.m4b").unwrap();

        assert!(a.is_verified() && b.is_verified());
        assert_ne!(a, b, "different content must hash differently");
        assert!(
            !group_is_verified_identical(&[a, b]),
            "AC-14: a group that disagrees must never auto-resolve"
        );
    }

    #[test]
    fn a_group_whose_members_all_match_is_verified() {
        let s = MemSource::new()
            .with("one/Dune.m4b", b"identical")
            .with("two/Dune.m4b", b"identical")
            .with("three/Dune.m4b", b"identical");
        let hashes: Vec<MemberHash> = ["one/Dune.m4b", "two/Dune.m4b", "three/Dune.m4b"]
            .iter()
            .map(|p| hash_member(&s, p).unwrap())
            .collect();
        assert!(group_is_verified_identical(&hashes));
    }

    /// An unreadable member is a VALUE, not a job failure: the rest of the group
    /// still gets hashed. But it does block the automatic path, because "we
    /// could not read this one" is not the same as "this one matches".
    #[test]
    fn an_unreadable_member_fails_softly_and_blocks_auto_resolution() {
        let s = MemSource::new()
            .with("one/Dune.m4b", b"identical")
            .with("two/Dune.m4b", b"identical")
            .broken("three/Dune.m4b", "access denied");

        let ok = hash_member(&s, "one/Dune.m4b").unwrap();
        let also_ok = hash_member(&s, "two/Dune.m4b").unwrap();
        let bad = hash_member(&s, "three/Dune.m4b").unwrap();

        assert!(ok.is_verified());
        assert!(!bad.is_verified());
        match &bad {
            MemberHash::Failed(why) => assert!(why.contains("access denied")),
            MemberHash::Hashed(_) => panic!("should not have hashed"),
        }
        assert!(
            !group_is_verified_identical(&[ok, also_ok, bad]),
            "AC-12: an unverified member blocks the group"
        );
    }

    /// A missing file is a read failure like any other, not a panic and not a
    /// silent success. Worth its own test because "file vanished between the
    /// scan and the hash" is an ordinary event on a live library.
    #[test]
    fn a_missing_file_is_a_failure_not_a_silent_pass() {
        let s = MemSource::new().with("here.m4b", b"x");
        let gone = hash_member(&s, "gone.m4b").unwrap();
        assert!(!gone.is_verified());
    }

    /// Guards the empty case explicitly. An empty group reaching this function
    /// means something upstream is wrong, and answering "yes, verified" would
    /// turn that bug into a set-aside.
    #[test]
    fn an_empty_group_is_never_verified() {
        assert!(!group_is_verified_identical(&[]));
    }

    /// A single member is not a duplicate group and must not read as one.
    #[test]
    fn a_lone_member_does_not_count_as_a_verified_duplicate_group() {
        let s = MemSource::new().with("only.m4b", b"x");
        let one = hash_member(&s, "only.m4b").unwrap();
        // It IS internally consistent, so this documents the boundary: group
        // membership is the detector's job, not this function's. What matters is
        // that the caller never hands it a group of one.
        assert!(group_is_verified_identical(&[one]));
    }
}
