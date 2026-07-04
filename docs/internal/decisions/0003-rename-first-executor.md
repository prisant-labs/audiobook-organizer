---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 3. Rename-First Executor

## Context and Problem Statement

The library is 297 GB on a drive with 1.3 TB free (18 TB drive, 16.7 TB used, per the WizTree telemetry). A full copy-to-new-tree apply strategy fits within free space, but the apply mechanism must be chosen deliberately rather than defaulting to "always copy," since the same 297 GB of headroom is also needed for a pre-campaign safety backup.

## Considered Options

- **Copy-to-new-tree for every operation:** safest against in-place corruption, but costs hours of I/O and temporarily consumes roughly a quarter of the remaining headroom on a drive already 93 percent full, for every apply, including trivial same-volume renames.
- **Rename-first, same-volume primary; copy+verify+delete only cross-volume (chosen):** same-volume moves are metadata-only renames, near-instant, and move zero bytes; a full copy is reserved for genuine cross-volume operations.

## Decision Outcome

Chosen: **rename-first executor** (D-08 (rename-first executor), 2026-07-02, corrected rationale). Same-volume rename is the primary apply strategy; copy+verify+delete is used only for cross-volume moves. The corrected rationale: a full 297 GB copy is feasible on the free-space budget, but that capacity is better spent once, as the pre-campaign backup (D-17 (backup posture user-defined)), than as the routine apply mechanism for a library that is not changing volumes.

### Consequences

- Good, because same-volume renames are near-instant and do not compete with the pre-campaign backup for I/O time or free-space headroom.
- Good, because it correctly separates two different jobs: "make the library safe to touch" (the backup) from "reorganize the tree" (the apply), instead of conflating them by copying on every operation.
- Good, because it still requires plan validation to check summed byte estimates against free space for any copy-based (cross-volume) operations.
- Bad, because it means same-volume renames on NTFS are metadata operations without the extra safety margin a full copy-then-verify would give per operation; this is why journal-before-act, quarantine, and rollback (0004 (safety invariants)) carry the safety weight instead.
- Neutral, because the strategy is unaffected if the target ever needs to be cross-volume (a second drive, a NAS); the executor already branches on that case.
