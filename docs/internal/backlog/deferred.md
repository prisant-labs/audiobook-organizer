---
title: "Backlog: deferred work"
type: backlog
updated: 2026-08-05
---

# Deferred

Work that was in scope and got cut. Newest first. See [`README.md`](README.md)
for what belongs here and how items leave.

---

## Keep-higher-bitrate resolution policy

- **Deferred:** 2026-08-05, UI round 2 crit pass.
- **Feature id:** `F-1108` in the PRD's E-11 section.
- **What:** one of four duplicate-resolution policies, ranking copies by declared
  audio bitrate to choose a keeper.
- **Why not now, and the reason matters because my first one was wrong.** I said
  it needed a media dependency the project does not have. It has one: `lofty` is a
  normal dependency, compiles into the default build, and already opens these exact
  files to read cover art for `F-907`. Bitrate is genuinely available.
  The real reason to cut it: **file size is a free proxy.** For the same book, a
  higher-bitrate copy *is* a larger file, so `keep-larger` already captures it
  using a number that cannot be missing, cannot be wrong, and needs no decode.
  It also has no well-defined value for a book split across N files: "the bitrate"
  of twelve mp3s requires an arbitrary rule.
- **What would make it worth doing:** evidence that `keep-larger` picks the wrong
  copy often enough to matter, which would mean copies of the same book differing
  in size for reasons other than encoding quality.
- **Side effect:** closes `OQ-1` (per-copy bitrate source) by making it moot.

## Folder-change notice while browsing

- **Deferred:** 2026-08-05, UI round 2 crit pass.
- **Feature id:** `F-1109` in the PRD's E-11 section.
- **What:** a count of what changed in the library since the last scan, surfaced
  passively while browsing ("4 new files appeared").
- **Why not now:** it was drawn in a round 1 mockup as though it existed. It never
  did. Deferred rather than built because the **actual risk** it appeared to cover
  is better covered by the on-entry rescan check (`F-609`), which fires when the
  user is about to act rather than while they are reading. A passive notice needs
  either a filesystem watcher running for the life of the app, or a scan on every
  screen visit. Both cost more than the problem.
- **Do not confuse with:** the `snapshot-stale` refusal, which is a safety check
  at apply time. That protects you; it does not inform you.
- **What would make it worth doing:** users reporting surprise at stale library
  counts *despite* the on-entry check.

---

## Standing note

Anything cut from a release belongs here on the day it is cut, with the reason
written while it is fresh. A reason reconstructed six weeks later is a guess.
