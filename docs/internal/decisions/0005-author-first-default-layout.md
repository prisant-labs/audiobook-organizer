---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 5. Author-First Default Layout

## Context and Problem Statement

The strategy brief listed target structure as an open question needing jp's taste-and-browsing judgment: author-first (ABS-native), title-first (jp's documented historical preference), or a genre-preserving hybrid. jp's own prior-work notes (`_local\prior-work\folder-structure.md`) recommend a title-first pattern (`Title - Author (Year)`), reasoning that Audiobookshelf (ABS) surfaces authors and series from metadata regardless of folder naming, and explicitly advise against author as the top-level grouping "unless you personally prefer it."

## Considered Options

- **Title-first** (`Title - Author (Year)`, jp's historical preference per `folder-structure.md`): book title as the primary focus of the folder name, genre as a top-level grouping folder.
- **Author-first** (`{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/`, the `abs-author-first` preset, chosen): author as the top-level grouping, series and sequence nested beneath.
- **Genre-preserving hybrid:** retain genre as a folder tier alongside author or title.

## Decision Outcome

Chosen: **author-first** as the default (D-02 (author-first default layout), 2026-07-03), using the `abs-author-first` preset: `{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/`. Genre and awards become tags/collections, never folders. This supersedes the historical title-first preference recorded in `folder-structure.md`; the tool remains configurable to other presets, but the campaign and the shipped default use author-first.

### Consequences

- Good, because author-first is the layout both discovery passes leaned toward and aligns with how ABS's own author and series browsing views are structured.
- Good, because collapsing genre and awards into tags/collections rather than folders avoids the award-pack nesting problem (Hugo, Nebula, Top 100 megafolders) that the discovery docs identified as a major source of structural mess.
- Bad, because it departs from jp's own previously documented preference, which specifically favored title-first and specifically warned against author-as-top-level; this decision consciously overrides that earlier guidance.
- Neutral, because the ruleset model (F-401 (naming templates), F-402 (structure policies)) keeps title-first and hybrid available as alternate presets, so the historical preference is not lost, only demoted from default.
