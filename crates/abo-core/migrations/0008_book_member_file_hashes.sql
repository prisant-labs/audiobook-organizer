-- F-1110 AC-54 (content match tier for BOOK-level duplicate groups): per-file
-- hashes beneath a duplicate group member that is a FOLDER.
--
-- Why a child table rather than more columns on duplicate_members.
--
-- Migration 0007 gave duplicate_members a content_hash / hash_error pair, which
-- works because an exact basename+size group's member IS one file. A book-level
-- group's member is a FOLDER holding one to fifty audio files, and a folder has
-- no hash. Squeezing it into the existing pair would mean inventing a
-- folder-digest, which is a second thing that can be wrong (over what set, in
-- what order, including which sidecars) and which nothing else in the product
-- needs. Storing the per-file facts instead keeps the comparison honest: two
-- folders hold the same audio when their multisets of file hashes agree.
--
-- The member-level pair is deliberately LEFT NULL for folder members. That is
-- what keeps the AC-12 auto-resolve gate closed for book groups by
-- construction rather than by a rule someone has to remember: the gate reads
-- duplicate_members.content_hash, finds nothing, and refuses. AC-52 keeps
-- book-level candidates recorded, counted, and never acted on; resolution opens
-- at P3, not here.
--
-- Same three-state encoding as 0007, for the same reason: no state column that
-- can disagree with the data it describes.
--
--   content_hash IS NULL AND hash_error IS NULL  -> never hashed
--   content_hash IS NOT NULL                     -> hashed, and this is it
--   hash_error   IS NOT NULL                     -> tried, failed, and this is why
CREATE TABLE duplicate_member_files (
    id           INTEGER PRIMARY KEY,
    member_id    INTEGER NOT NULL REFERENCES duplicate_members(id) ON DELETE CASCADE,
    entry_id     INTEGER NOT NULL,
    path         TEXT    NOT NULL,
    size         INTEGER NOT NULL,
    content_hash TEXT,
    hash_error   TEXT
);

-- One row per file per member. A second verification pass over the same group
-- must update rows rather than accumulate a duplicate set of them, and this is
-- what makes that an INSERT OR IGNORE instead of a read-then-branch.
CREATE UNIQUE INDEX idx_duplicate_member_files_unique
    ON duplicate_member_files (member_id, entry_id);

-- The one query the verification job runs: the files of a member that still
-- need a hash. Mirrors 0007's partial index for the same access pattern.
CREATE INDEX idx_duplicate_member_files_unhashed
    ON duplicate_member_files (member_id)
    WHERE content_hash IS NULL AND hash_error IS NULL;
