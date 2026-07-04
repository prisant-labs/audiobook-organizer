---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 12. Cover Extraction Commits to v0.4.0 with a Designed Fallback

## Context and Problem Statement

The prototyped cover-forward "library home" surface (F-902, renamed from "dashboard" per FD-07 (F-902 renamed library home)) depends on reading book cover art. The feature that reads that art, however, was defined as part of a deferred F-1101 subset (tag and metadata enrichment, deferred to v1.1+). Building a cover-forward UI in v0.4.0 (seeing) against a feature that does not exist until v1.1 is a scope contradiction the audit flagged.

## Considered Options

- Leave cover reading deferred to F-1101 (v1.1+) and redesign the v0.4.0 library home surface without cover art, delaying the cover-forward look.
- **Pull a minimal, read-only cover-extraction subset forward into v0.4.0, with a designed no-cover fallback tile as an acceptance criterion (chosen).**

## Decision Outcome

Chosen: **F-907 (cover extraction and fallback tiles)**, epic E-09, P0, v0.4.0 (seeing) (D-15 (cover extraction v0.4.0) and FD-03 (F-907 cover extraction), 2026-07-03). The `lofty` subset reads embedded art and `cover.jpg` sidecars, read-only. Covers render square (1:1), never 2:3 portrait, per D-06 (anti-reference). The fallback tile, required whenever no cover art is found, is title text on a deterministic colored tile (a hash of the title), so the shelf degrades gracefully rather than showing broken images or empty boxes.

### Consequences

- Good, because it resolves the audit's Stream 2, finding 9 (IMPORTANT): the cover-forward library home no longer depends on a feature that will not exist until v1.1.
- Good, because the fallback tile is an explicit acceptance criterion, not an afterthought: every book, with or without embedded art, renders a coherent square tile in v0.4.0 (seeing).
- Good, because scoping this subset as read-only, extraction-only (no writing, no online lookup, no full tag enrichment) keeps it narrow and avoids reopening the metadata-editor scope creep the strategy brief explicitly warned against.
- Bad, because it pulls forward filesystem and library work (embedded-art parsing) that was originally planned for v1.1, adding scope to v0.4.0 (seeing).
- Neutral, because the deterministic-hash fallback tile design is reusable as-is if full F-1101 metadata enrichment ships later; nothing here needs to be rebuilt.
