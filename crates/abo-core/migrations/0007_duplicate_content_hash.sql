-- F-702 (hash verification, v0.6.0 P2): persist the content hash of a duplicate
-- group member so re-opening the duplicates surface never re-reads the bytes
-- (AC-15).
--
-- Two nullable columns rather than a hash plus a separate state enum, on
-- purpose. A state column is a third source of truth that can disagree with the
-- data it describes, and this project has already been bitten once by a probe
-- that could not tell "definitely absent" from "could not tell". Here the three
-- states are read directly off the data and cannot drift:
--
--   content_hash IS NULL AND hash_error IS NULL  -> never hashed
--   content_hash IS NOT NULL                     -> hashed, and this is it
--   hash_error   IS NOT NULL                     -> tried, failed, and this is why
--
-- The failure case is stored rather than discarded because "we could not read
-- this file" is a fact the surface has to show. A member whose hash failed is
-- NOT the same as one nobody has hashed yet: the first is a reason to refuse an
-- automatic resolution (AC-12), the second is merely work not yet done.
--
-- Additive: both columns are nullable with no default, so every existing row
-- reads as never-hashed, which is exactly what it is.
ALTER TABLE duplicate_members ADD COLUMN content_hash TEXT;
ALTER TABLE duplicate_members ADD COLUMN hash_error TEXT;

-- Hashing is candidates-only (AC-10), so this index serves the one query the
-- verification job runs: find the members of a group that still need a hash.
CREATE INDEX idx_duplicate_members_unhashed
    ON duplicate_members (group_id)
    WHERE content_hash IS NULL AND hash_error IS NULL;
