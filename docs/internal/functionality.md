---
title: "Audiobook Organizer - Functionality Summary and Detailed Breakdown"
date: 2026-07-05
status: living document
owner: jprisant
produced-by: AUTHOR agent (functionality)
sources:
  - docs/internal/product-requirements.md (Section 5, feature registry)
  - docs/internal/program-roadmap.md (release ladder, tracking statuses)
  - docs/internal/architecture.md (workspace, pipeline, IPC, data model)
  - docs/internal/releases/v0.1.0-spine/spec.md
  - docs/internal/releases/v0.2.0-understanding/spec.md
  - docs/internal/releases/v0.2.0-understanding/baseline-delta-2026-07-04.md
  - docs/internal/releases/v0.2.0-understanding/fd14-tag-probe-2026-07-04.md
  - docs/internal/releases/v0.3.0-planning/spec.md
  - docs/internal/releases/v0.3.0-planning/F-506-report-spec.md
  - docs/internal/decision-ledger.md
---

# Audiobook Organizer: functionality summary and detailed breakdown

This document answers one question: what does the software actually do, today, in detail, and what lands next. It is written for jp and for future contributors who need the real state of the product without re-reading the whole planning suite. Every reference ID below carries a short handle on first use in each section. Where a claim is "proven", it points at the release spec's acceptance criteria (AC) or gate (G) that proved it, or at a named evidence document.

## 1. Executive summary

Audiobook Organizer is a Windows desktop tool (Tauri v2, Rust, React, TypeScript, SQLite) that reorganizes a messy audiobook library into an Audiobookshelf-native (ABS-native) folder structure, without ever deleting audio and without ever touching the network. It is an analyzer and planner first, a mover second: everything before the executor stage is read-only, and the executor itself does not exist yet.

**What works today, proven on the real library.** Three releases are built and gate-walked: v0.1.0 (spine, scaffold and tracer bullet), v0.2.0 (understanding, scan/classify/parse), and v0.3.0 (planning, plan/validate/export/report). Run end to end against jp's real 297 GB, 14,799-entry library at `E:\Books - Audio`:

- A read-only scan completes in under 1 second (warm cache), two orders of magnitude inside the 60-second soft target, covering 14,799 entries and 320.8 GB with zero skipped entries and zero permission or junction failures [S: docs/internal/releases/v0.2.0-understanding/baseline-delta-2026-07-04.md, gate G-03].
- The classifier and parser turn that scan into a full library health picture: 582 book-like folders, 406 duplicate-candidate groups (856 copy files), 237 of 238 loose root files parsing cleanly, and a full per-class folder census, all cross-validated against the rescued 2026-03-25 WizTree baseline [S: baseline-delta-2026-07-04.md; v0.3.0 spec.md gate G-8].
- The plan builder turns that understanding into a validated, deterministic, 1,817-change plan, exported as CSV, JSON, and Markdown plus a single self-contained HTML dry-run report that opens with networking disabled and requires no app to read [S: docs/internal/releases/v0.3.0-planning/spec.md, gate G-5].

**What is next.** v0.4.0 (seeing, the first real GUI), v0.5.0 (acting, the executor and rollback), and v0.6.0 (hardening, interruption safety and dedupe resolution) are spec-ready but not yet built (see Section 3 and Section 8). Nothing moves, renames, or deletes a single file on the real library until v0.5.0 lands and jp explicitly approves a Real (non-dry-run) apply, which is a human-only gate under every circumstance [S: docs/internal/decision-ledger.md D-10].

## 2. Data-source statement

**No third-party book databases. No online lookups. No metadata services. Zero network requests, enforced by CI, not just promised.**

This was an explicit operator requirement and is architecturally load-bearing, not a placeholder for a future integration:

- All parsing is **folder- and file-name based**. The engine reads what the operator already named things and matches against nine catalogued naming patterns discovered by auditing the real library (see Section 3, Parse stage) [S: docs/internal/product-requirements.md Section 5, F-301].
- Embedded audio tags (ID3-style title/artist/album/composer frames) are read **exactly once**, read-only, as a bounded 300-file measurement probe for the FD-14 (tag-quality probe) gate item, never as a live extraction path. The probe **CONFIRMED folder-first**: folder names carry a usable title and author for 100% of the sampled files, versus 85% (title) and 96.3% (author) for embedded tags, and tag-vs-folder agreement on author is only 31.8% where both exist, meaning the two sources are not even interchangeable [S: docs/internal/releases/v0.2.0-understanding/fd14-tag-probe-2026-07-04.md]. The probe code lives behind an opt-in Cargo feature (`probe`) that is absent from every shipped build.
- Cover art extraction (F-907, spec-ready for v0.4.0) reads embedded art and `cover.jpg` sidecars that are **already on disk**, read-only, for display only. It is not a metadata lookup.
- A CI grep gate greps the report template and the app bundle for external hosts (`http://`, `https://`, remote `<link>`, remote `<script src>`) and fails the build on any hit; this enforced the same posture from v0.1.0 onward [S: docs/internal/architecture.md Section 5, FD-11 (zero-network invariant)].
- No ABS API calls, no tag writes, no downloader, no cloud sync exist anywhere in scope. See Section 7 (non-goals) for the full list.

The practical consequence: everything the tool "knows" about a book (title, author, series, index, year, narrator) it inferred from how the file or folder is named, weighted by where in the tree that name sits. This is why the parse stage's nine-pattern matcher set and its confidence tiering (Section 3) are the actual product; the file-move mechanics are commodity by comparison [S: product-requirements.md Section 2, "classification is the product"].

## 3. The pipeline, stage by stage

Everything the product does is one strict pipeline; no stage may be skipped, and later stages refuse input that failed an earlier one [S: architecture.md Section 2].

```
scan -> classify -> parse -> plan -> validate -> preview/approve -> apply -> verify
  |                                       |               |
  +-- WizTree CSV import (alt entry)      +-- export      +-- journal -> rollback
```

### 3.1 Scan (F-101 live tree scanner, F-102 WizTree CSV import, F-105 snapshot persistence)

**What it does.** Recursively walks a chosen root (or imports an existing WizTree CSV export) and writes an immutable snapshot into SQLite: one `scans` row plus one `entries` row per file or folder, each with path, kind, size, mtime, depth, and parent linkage.

**Key behaviors.** Extended-length (`\\?\`) path semantics from day one, so paths past the legacy 260-character limit still open. Permission-denied entries are recorded and skipped, never aborting the whole scan. Junctions and reparse points are recorded but never followed, so a junction loop terminates cleanly instead of recursing forever. The WizTree CSV path (F-102) produces a snapshot flagged `source = csv` that is indistinguishable downstream from a live scan, letting planning start from an existing export without touching the drive.

**Edge handling.** A root that does not exist or is not a directory fails with a taxonomy error, never a panic. A malformed CSV row raises `csv-parse(row)` naming the row without aborting the whole import.

**Proven by.** AC-9 through AC-14 (v0.1.0 spec) and AC-101.1 through AC-102.3 (v0.2.0 spec); real-library evidence in baseline-delta-2026-07-04.md: 14,799 entries, 320.8 GB, 0 skipped, 113 near-260-character interop warnings (all inside deep Hugo/Nebula collection paths), 0 permission-denied, 0 junction loops, under 1 second wall time. The rescued 2026-03-25 CSV also imported cleanly with 0 malformed rows across 14,689 data rows.

**Status: BUILT** (v0.1.0 minimal walker; hardened in v0.2.0 against the full Windows edge list).

**File typing (F-103), the companion to scan.** Every scanned file gets an extension-based class, established at v0.1.0:

| Class | Extensions | Notes |
|---|---|---|
| `audio` | m4b, mp3, m4a, opus, wma, flac | The books themselves |
| `ebook` | epub, pdf, mobi, azw3, lit, pdb, docx | Sidecars that travel with a book by default |
| `image` | jpg, jpeg, png, gif, webp, bmp | Covers and art |
| `playlist` | m3u, m3u8, cue | Non-audio clutter, set aside by default |
| `release-info` | nfo, sfv, txt | Non-audio clutter, set aside by default |
| `weblink` | url, html, htm | Non-audio clutter, set aside by default |
| `comic` | cbr, cbz | Routed to `manual-review` at the folder level (FD-17) |
| `video` | mp4, mkv, avi, mov, wmv, m4v | New per FD-17; folders dominated by this class route to `manual-review`, never auto-planned |
| `other` | anything uncatalogued | Never silently dropped |

`.mp4` types as `video` unconditionally because extension alone cannot distinguish an audio-in-mp4 audiobook from an actual video file; this is a documented, conservative default, not an oversight (see Section 8).

### 3.2 Classify (F-201 folder classification engine, F-202 library health metrics, F-203 multi-book detection, F-204 disc-structure detection, F-205 parallel-format detection)

**What it does.** Assigns exactly one `FolderClass` to every folder, evaluated bottom-up (children before parents): `book`, `series-container`, `pack-container`, `staging`, `mixed`, `multi-book-suspect`, `empty`, `docs-resources`, or `manual-review`. Every classification records a rule ID and evidence so the product can explain itself later.

**Key behaviors.** `manual-review` is a first-class outcome, not an error. Folders dominated by video or course content (the "52 Sales Lessons" mp4 case) and radio plays route to `manual-review` and are never auto-planned (FD-17, video/course routing). Box-set-style folders holding several complete books as sibling files (Harry Potter, Narnia, Wings of Fire) are flagged `multi-book-suspect` rather than misread as one book. Disc-based books (`Disc NN`/`CD NN`/`Disk NN`) are recognized as ABS-conformant; nonconforming disc names get a proposed rename. Books with both mp3 chapters and an m4b sibling are flagged so the preferred-format policy can pick a keeper.

**Edge handling.** A legitimate single book split into `Disc NN` subfolders is explicitly NOT flagged multi-book. A folder with one book plus a bonus file is not falsely flagged. Folders with no audio anywhere beneath (`empty`) are distinguished from folders holding only ebooks/nfo/images (`docs-resources`).

**Proven by.** AC-201.1 through AC-205 (v0.2.0 spec, disc/parallel-format land as F-204/F-205 in v0.3.0); real-library census in baseline-delta-2026-07-04.md: book 312, series-container 26, pack-container 41, staging 2, mixed 10, multi-book-suspect 270, empty 21, docs-resources 31, manual-review 6 (live, 2026-07-04).

**Status: BUILT** (v0.2.0 core engine; F-204/F-205 detection added in v0.3.0).

### 3.3 Parse (F-301 pattern matcher set, F-302 noise strippers, F-303 field extraction with confidence, F-304 name normalizer)

**What it does.** Runs nine ordered, pure-function pattern matchers (lifted from an audit of the real library) against every folder and file name, strips ripper-tag noise, extracts title/author/series/index/year/narrator with a per-field confidence tier, and normalizes the resulting path components for Windows legality.

**Key behaviors.** The nine patterns, run in specificity order so a name matchable by two patterns resolves to the more specific one; equal-score ties surface as `ambiguous`, never a guess:

| # | Pattern | Real-library example shape |
|---|---|---|
| 1 | `Title by Author` | The 238 root files; 237 parse cleanly |
| 2 | `Author - Title` | Standard two-field folder name |
| 3 | `Title by Author` (folder variant) | Same as #1 but naming a folder, not a file |
| 4 | `Year - Author - Title (Series #N) [noise]`, `^` award marker | Hugo/Nebula award-collection entries |
| 5 | `N - Title - Author - Year` | Top 100-style ranked packs |
| 6 | `NN.N - Title [noise]` | Series entries with a decimal sequence number |
| 7 | `Author_Name_-_Title` | Underscored ripper output |
| 8 | Bare `Title` | No author signal in the name at all |
| 9 | `Author - Series` with irregular separators | e.g. `Frank Herbert-Dune-#1-Chronicles[1-8}` |

Noise strippers (bracket tags, bitrate/size markers, rank/year prefixes, release-group suffixes, underscore-to-space) are individually toggleable and hold the hard property `strip(strip(x)) == strip(x)`. Field confidence is high (explicit in the name), medium (inherited from a parsed parent), or low (guessed); an explicit in-name value always overrides an inherited one. The normalizer strips Windows-illegal characters, forbids reserved device names and trailing dots/spaces, truncates at a word boundary (default 120 chars), and applies Unicode NFC normalization so visually identical NFC/NFD names compare equal.

**Edge handling.** The one root file (of 238) that does not parse cleanly falls through to `manual-review`/`ambiguous`, never a wrong parse. A name that is entirely noise is preserved or flagged, never stripped to empty. An all-illegal-character name normalizes to a safe placeholder or is flagged, never emitted empty.

**Proven by.** AC-301.1 through AC-304.5 (v0.2.0 spec). Parser fixture coverage measured at 95.2% (20 of 21 fixture cases), above the 90% freeze threshold, so the pattern set stayed open rather than freezing. On the real library, loose-root parses reproduced the discovery baseline exactly: 237 of 238 both on the rescued CSV and the fresh live scan [S: baseline-delta-2026-07-04.md]. Idempotence proven by property test (proptest) across composed-realistic and arbitrary-Unicode generators, 4096 cases, 5 repeated runs clean [S: v0.2.0 spec.md gate G-02].

**Status: BUILT.**

### 3.4 Plan (F-401 naming templates, F-402 structure policies, F-403 plan builder, F-405 plan persistence, F-507 pack provenance capture)

**What it does.** Combines classifications, parsed fields, and a named ruleset into an ordered, immutable list of proposed changes (move, rename, mkdir, rmdir-empty, quarantine/set-aside, or no-op), grouped into the seven user-facing campaign groups. Internally the builder runs eight passes; series-index normalization folds into "messy names" for every user-facing surface while remaining a distinct internal pass (FD-26, campaign group canon) [S: product-requirements.md Section 3.1].

| User-facing group (what the reviewer sees) | What it does | Internal plan pass(es) |
|---|---|---|
| Staging | Moves intake/processing folders (`_sort`, `_process`) out of the scanned library | `staging-separation` |
| Loose books | Gives each loose root audiobook its own folder | `loose-root-books` |
| Messy names | Strips ripper tags, normalizes names, fixes series numbering | `strip-noise`, `normalize-series` |
| Box sets | Splits folders holding several complete books | `split-multi-book` |
| Bundles | Extracts books from award/source packs into canonical folders | `flatten-packs` |
| Copies | Sets aside duplicate copies of a book | `dedupe-quarantine` |
| Empty folders | Removes verified-empty folders | `empty-cleanup` |

**Key behaviors.** Three shipped naming presets: `abs-author-first` (the default, `{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/`), `title-first`, and `hybrid-genre`. Genre and awards never become folders under the default; they become provenance data instead (D-02, author-first default). Missing-field fallbacks are explicit (a missing year omits the `(Year)` segment rather than emitting `()`; a missing series index routes the book to `manual-review` rather than guessing). Structure policies with safe defaults: one-book-per-folder, pack shells go to set-aside after successful extraction (leave-in-place is a toggle), sidecars travel with their book, non-audio clutter (nfo/sfv/playlist/weblink) is set aside by default, the preferred format on a parallel-format conflict is m4b with the loser set aside. For every book pulled out of a pack or award collection (Hugo, Nebula, Top 100, Dune Universe), the plan builder records that provenance as durable data at plan time, so flattening never silently destroys where a book came from (F-507, new per D-14 and FD-01). The plan is deterministic: the same snapshot plus the same ruleset always produces a byte-identical plan (golden-tested).

**Edge handling.** A pack whose extraction is only partially successful (some members blocked) leaves the whole shell in place rather than set-aside. Operation ordering respects dependencies: `mkdir` runs before any move into it, and `rmdir-empty` never precedes the moves that emptied the folder. A book that cannot be placed becomes `no-op(manual-review)`, never a guessed move.

**Proven by.** AC-1 through AC-11 and AC-24 through AC-26 (v0.3.0 spec). Real-library run: 237 loose-root moves, 57 messy-name rename ops (reconciled against the v0.2.0 noise-family baselines), 1,817 total changes, plan determinism re-verified independently by a second reviewer [S: v0.3.0 spec.md gates G-1, G-3].

**Status: BUILT** for capture and reporting; the v0.5.0 half of F-507 (carrying provenance into the journal and manifest, and re-emitting the report post-apply) is spec-ready but not yet built.

### 3.5 Validate (F-404 plan validation)

**What it does.** The backstop that assigns every operation a verdict of `valid`, `warning(reason)`, or `blocked(reason)` before it could ever reach an executor.

**Key behaviors.** Checks target collisions within the plan and against the live disk (case-insensitively, matching NTFS), source-inside-target cycles, full path length using the extended-length allowance for the hard block while still warning near 260 characters for interop with tools that lack long-path support, illegal component names and reserved device names as a backstop to the normalizer, cross-volume moves sized against free disk space, and snapshot staleness (a source that vanished since the scan blocks the op rather than proceeding blind).

**Edge handling.** Two operations differing only in letter case collide and are blocked. A cross-volume operation summing past free space is blocked with `cross-volume-space-insufficient`. `LongPathsEnabled=0` on a target past 260 characters produces a warning carrying a how-to link rather than a silent failure.

**Proven by.** AC-12 through AC-15 (v0.3.0 spec). A purpose-built hostile fixture seeds a full hazard set (a planned collision, a case-only collision, a source-inside-target cycle, an over-length path, a reserved name, and an insufficient-space cross-volume op); the gate recorded 12 seeded hazards all blocked or warned with the correct pinned machine code, independently re-run by a second reviewer [S: v0.3.0 spec.md gate G-2]. See Section 5 for the full hazard table.

**Status: BUILT.**

### 3.6 Preview / export / report (F-505 plan export, F-506 dry-run HTML report)

**What it does.** Turns a validated plan into files a human (or a spreadsheet, or a non-engineer) can read: CSV, JSON, and Markdown exports, plus a single self-contained HTML dry-run report.

**Key behaviors.** The HTML report (F-506) is generated entirely by `abo-core`, with its template baked into the crate; no GUI is required to produce it, which is why it shipped a full release before any app screen existed. It opens correctly from a `file://` path with networking fully disabled: Literata is embedded as a subsetted woff2 data URI with a system-serif fallback, and a CI grep gate fails the build on any external host reference. Required sections, in order: masthead and dateline; a plain-language lead paragraph; a seven-group summary table; before/after example tables per group with struck-through removed noise; a warnings-needing-a-decision callout; the FD-10 deletion-guarantee block, verbatim; a provenance section (F-507); the complete change-list table with no row cap (one row per plan operation, no pagination, because it is a static file); and a closing signature line. Duplicates are counted in groups everywhere, never in raw copy counts (FD-08).

**Edge handling.** An already-tidy library still produces a report that says so, listing zero changes rather than omitting the section. A plan with only blocked operations exports non-empty artifacts that show the blocks, never silently dropping them.

**Proven by.** AC-18 through AC-23 (v0.3.0 spec). Real-library HTML report generated over the 1,817-change plan, opened with exactly one localhost request in the render evidence (i.e., effectively zero external network activity) and the CI grep gate green [S: v0.3.0 spec.md gate G-5]. The one item still pending at time of writing is the non-engineer read test itself (gate G-6): the real report has been delivered to jp and render-verified by the orchestrator, but jp's own confirmation read is recorded as the single non-blocking, human-only item before the v0.3.0 tag [S: v0.3.0 spec.md gate G-6; D-10].

**Status: BUILT**, tag pending jp's read confirmation (non-blocking per D-10).

## 4. Detailed feature breakdown

Every feature ID from the PRD registry (docs/internal/product-requirements.md Section 5), one row each. Status legend: **BUILT** = shipped and gate-checked in a tagged-or-tag-pending release; **SPEC-READY** = a release spec exists with authored AC but implementation has not started; **DEFERRED** = explicitly out of v1 scope (E-11, v1.1+ candidates).

| ID (handle) | One-line function | Release | Status | Notable implementation facts |
|---|---|---|---|---|
| F-101 (live tree scanner) | Recursive walk of a chosen root into a snapshot | v0.1.0 / hardened v0.2.0 | BUILT | Extended-length path opens from day one; junctions recorded, never followed; 14,799 real entries in under 1s |
| F-102 (WizTree CSV import) | Alternate snapshot source from a WizTree export | v0.2.0 | BUILT | Same schema as F-101, `source=csv`; rescued 2026-03-25 CSV imports with 0 malformed rows over 14,689 rows |
| F-103 (file typing) | Extension-based class per file | v0.1.0 | BUILT | Adds a `video` class (mp4/mkv/avi/mov/wmv/m4v) and `comic` (cbr/cbz); `.mp4` types as video, container inspection deferred |
| F-104 (job progress + cancel) | Progress events and cooperative cancel | v0.2.0 | BUILT | Cancel only at safe boundaries (between entries), never mid-file; a killed scan is visible as not-completed on restart |
| F-105 (snapshot persistence) | Immutable snapshots with metadata | v0.1.0 | BUILT | One `scans` row + N `entries` rows per scan, never mutated afterward |
| F-201 (folder classification engine) | A `FolderClass` per folder | v0.2.0 | BUILT | 9 classes, bottom-up, every verdict carries rule id + evidence; live real-library census recorded |
| F-202 (library health facts) | Aggregate problem counts and sizes | v0.2.0 | BUILT | Every metric states its counted unit (folders/files/bytes), no bare GB figures |
| F-203 (box-set detection) | Flag folders holding several complete books | v0.2.0 | BUILT | Harry Potter (11 files/7 titles) and Narnia (7) style cases detected, not misread as one book |
| F-204 (disc-structure detection) | Recognize disc-based books, propose renames for nonconforming names | v0.3.0 | BUILT | `Disc NN`/`CD NN`/`Disk NN` recognized; nonconforming variants get a conformant-shape rename |
| F-205 (parallel-format detection) | Flag books with both mp3 chapters and an m4b sibling | v0.3.0 | BUILT | Feeds F-402 preferred-format policy (m4b kept, loser set aside, never deleted) |
| F-301 (pattern matcher set) | The 9 discovery patterns as ordered matchers | v0.2.0 | BUILT | Pure functions; 95.2% fixture coverage; 237/238 loose-root parses reproduced exactly on live data |
| F-302 (noise strippers) | Ripper-tag, bitrate/size, rank/year, suffix, underscore strippers | v0.2.0 | BUILT | Idempotent (`strip(strip(x))==strip(x)`), 4096-case proptest, individually toggleable |
| F-303 (field extraction with confidence) | title/author/series/index/year/narrator + confidence | v0.2.0 | BUILT | Folder-first default confirmed by the FD-14 probe; inherited fields stay medium confidence, never promoted to high |
| F-304 (name normalizer) | Whitespace/punctuation/casing/illegal-char/reserved-name policy | v0.2.0 | BUILT | NFC normalization, 120-char word-boundary truncation, reserved Windows device names forbidden |
| F-401 (naming templates) | Configurable target patterns for standalone/series/disc books | v0.3.0 | BUILT | Default preset `abs-author-first`; missing-field fallbacks never emit empty segments |
| F-402 (structure policies) | One-book-per-folder, pack, sidecar, format, clutter policies | v0.3.0 | BUILT | Pack shell to set-aside is the default after full extraction; partial extraction leaves the shell in place |
| F-403 (plan builder) | Generate the change list from snapshot + ruleset | v0.3.0 | BUILT | Deterministic (byte-identical golden); 8 internal passes fold to 7 user-facing groups |
| F-404 (plan validation) | Collisions, path safety, cycles, disk space, staleness | v0.3.0 | BUILT | 12 seeded hazards blocked/warned with pinned machine codes in the hostile-fixture gate |
| F-405 (plan persistence and versioning) | Immutable plans with approval state | v0.3.0 | BUILT | Regenerating after a ruleset tweak always creates a new plan row, never mutates the old one |
| F-501 (everything view) REDEFINED | Virtualized full change list, tree optional | v0.6.0 | SPEC-READY | Redefined by D-16/FD-06: no longer the P0 tree diff; a tier-1 disclosure surface, later |
| F-502 (campaign group review) | Approve/reject/defer per group; per-change override | v0.4.0 | SPEC-READY | Per-change exclude lives inside group detail |
| F-503 (search and filter in preview) | Find any book/path/rule across the plan | v0.4.0 | SPEC-READY | Simple filter box |
| F-504 (explainability) | Every change shows rationale, matched pattern, confidence | v0.4.0 | SPEC-READY | The content behind "Show file details" for tier 1 |
| F-505 (plan export) | CSV, JSON, and Markdown plan artifacts | v0.3.0 | BUILT | CSV round-trips one row per op; Markdown groups by the 7 campaign groups |
| F-506 (dry-run HTML report) | Self-contained, non-engineer-readable HTML report | v0.3.0 | BUILT | 1,817-change real report generated, zero-network verified; jp's read confirmation pending (non-blocking) |
| F-507 (pack provenance capture and report) NEW | Record source-pack/award membership; export a provenance report | v0.3.0 (capture) / v0.5.0 (carry-through) | BUILT (v0.3.0 half) | Every flattened pack member recorded, including validation-blocked members; journal/manifest carry-through is v0.5.0 |
| F-601 (executor) | Apply approved changes in dependency order | v0.5.0 | SPEC-READY | Rename-first same-volume; copy+verify+delete only cross-volume |
| F-602 (journal + undo manifest) | Append-only journal; reversible manifest per apply job | v0.5.0 | SPEC-READY | Journal-before-act; JSON manifest export so recovery survives a sick DB |
| F-603 (rollback) | Full or partial undo from a manifest | v0.5.0 | SPEC-READY | Rollback is just another plan through the same pipeline |
| F-604 (post-apply verification) | Verify moved trees; refresh snapshot; report | v0.5.0 | SPEC-READY | Discrepancies block further groups until acknowledged |
| F-605 (set-aside / quarantine) | Move-not-delete holding area with provenance | v0.5.0 | SPEC-READY | Named "Set Aside" on disk; "quarantine" stays internal-only vocabulary |
| F-606 (interruption safety + resume) | Crash/cancel mid-apply leaves a resumable state | v0.6.0 | SPEC-READY | At most one change in doubt, auto-reconciled on restart |
| F-607 (dry-run harness) | Execute a plan against a virtual filesystem only | v0.5.0 | SPEC-READY | Same executor code path against `MemFs` instead of `RealFs` |
| F-608 (pause and resume apply) NEW | Pause an apply job between changes; resume | v0.5.0 | SPEC-READY | Pause takes effect between operations only; journal unaffected |
| F-701 (duplicate candidate detection) | Name+size exact grouping across the snapshot | v0.3.0 | BUILT | 406 groups / 856 copy files on the real library; version candidates never auto-resolved |
| F-702 (hash verification) | Opt-in content hash before any set-aside action | v0.6.0 | SPEC-READY | BLAKE3 over candidates only, never hash-everything |
| F-703 (duplicate review + report) | Grouped review UI plus CSV export | v0.6.0 | SPEC-READY | Dedupe is just another campaign group |
| F-704 (resolution policies) | keep-larger / keep-higher-bitrate / keep-m4b / flag-only | v0.6.0 | SPEC-READY | Default policy is flag-only |
| F-801 (ruleset model + persistence) | Named rulesets bundling templates, policies, toggles | v0.3.0 | BUILT | JSON body against a versioned schema; invalid bodies rejected on save |
| F-802 (ruleset import/export) | Portable JSON ruleset files | v0.6.0 | SPEC-READY | - |
| F-803 (app settings) | Library roots, set-aside root, reports folder, theme, retention | v0.4.0 | SPEC-READY | Hosts theme default `day`, snapshot retention default 10 |
| F-901 (app shell + navigation) | Sidebar nav, theme, command palette | v0.4.0 | SPEC-READY | Day/Evening themes |
| F-902 (library home) RENAMED | Warm cover-forward shelf; health facts inside sentences | v0.4.0 | SPEC-READY | Renamed from "dashboard"; no stat bands, no hero metrics |
| F-903 (plan preview surface) | Hosts campaign group review, search, explainability | v0.4.0 | SPEC-READY | - |
| F-904 (apply + activity surface) | Live job progress, journal view, verification, rollback entry | v0.5.0 | SPEC-READY | One job at a time, deliberately boring and explicit |
| F-905 (duplicates surface) | Hosts duplicate review | v0.6.0 | SPEC-READY | - |
| F-906 (settings + ruleset editor) | Ruleset CRUD with live re-plan preview counts | v0.4.0 | SPEC-READY | - |
| F-907 (cover extraction and fallback tiles) NEW | Read embedded art and cover.jpg, read-only | v0.4.0 | SPEC-READY | Square 1:1 only; deterministic hash-colored fallback tile when no cover exists |
| F-908 (error, empty, and loading states) NEW | Family-safe surface for every error family; empty/loading states | v0.4.0 | SPEC-READY | Every AppError family maps to a designed, non-jargon surface |
| F-909 (first-run and library root selection) NEW | Onboarding: pick library root, default ruleset/theme | v0.4.0 | SPEC-READY | Uses `tauri-plugin-dialog`; frontend never touches the filesystem directly |
| F-1001 (activity log) | Append-only record of every scan/plan/apply/rollback | v0.2.0 | BUILT | One `activity_records` row per action with params, outcome, timestamps |
| F-1002 (reports folder) | All exports as files | v0.3.0 | BUILT | Plans, manifests (later), verification (later), provenance, HTML report all land here |
| F-1003 (structured app logging) | `tauri-plugin-log` plus `tracing` in core; log rotation | v0.1.0 | BUILT | No telemetry, no crash reporting, no network |
| F-1101 (embedded tag reader) | Full tag reconciliation | v1.1+ | DEFERRED | The bounded read-only probe (FD-14) already ran once as a measurement, not a shipped path |
| F-1102 (ABS API integration) | Trigger ABS rescan; push provenance as collections | v1.1+ | DEFERRED | Provenance capture ships in v1 (F-507); the ABS-side push is what is deferred |
| F-1103 (online metadata lookup) | External book database lookups | v2+ | DEFERRED | Explicit non-goal; see Section 2 |
| F-1104 (cover art management) | Writing/managing cover art | v2+ | DEFERRED | ABS already handles covers natively |
| F-1105 (intake mode) | Watch/classify/file new acquisitions automatically | v1.1+ (posture undecided) | DEFERRED | Needs a product-posture decision from jp before it enters any roadmap slot |
| F-1106 (tag writing) | Write metadata back into audio files | v2+ | DEFERRED | Explicitly out; structure first |
| F-1107 (multi-library / media generalization) | Generalize the engine beyond one library/media type | v2+ | DEFERRED | Kept generic where free; not designed for on purpose |

## 5. Safety model

The product's central promise is that it is safe to point at 297 GB of hard-to-reacquire files. The invariants below are architecture, not settings: they cannot be toggled off in any ruleset [S: docs/internal/decision-ledger.md D-09].

### 5.1 As implemented today

Every release built so far (v0.1.0 through v0.3.0) is **read-only against the live library by construction**: there is no executor, no `Vfs::RealFs` write path wired to a plan, and no way to invoke a mutation from the current codebase. The only artifacts a run produces are database rows in the app's own SQLite file and export files (CSV/JSON/Markdown/HTML) in the reports folder. Nothing under the scanned root is created, modified, renamed, or deleted by anything built to date; every gate-evidence document (baseline-delta, fd14-tag-probe, v0.3.0 spec gates) states this as a verified constraint, not an assumption.

### 5.2 Specified for v0.5.0 (acting) - not yet built

| Invariant | What it means | Mechanism (spec-ready) |
|---|---|---|
| Never overwrite | A target that already exists is never clobbered | Executor re-checks target-does-not-exist immediately before every operation (TOCTOU backstop); an appeared target is `target-appeared`, a hard stop |
| Never delete audio | No code path in the product deletes an audio file, ever | Only `rmdir-empty` removes anything, and only on a folder verified empty; duplicates and clutter move to Set Aside, they are never deleted |
| Journal-before-act | Every operation is logged as intended before it happens | The executor writes an `intent` journal row and flushes it before each filesystem operation, then writes `done` or `failed` after |
| Single-writer | At most one apply job runs, ever, process-wide | A SQLite job lock plus an in-process async mutex; a second `apply_start` refuses with `job-already-running` |
| Vfs seam / dry-run is the same executor | Dry-run is not a separate, less-trustworthy code path | The identical executor runs against `MemFs` (in-memory) for dry-run and `RealFs` (disk) for a Real apply |
| Rollback is just another plan | Undo does not need special-cased executor logic | A manifest generates an inverse plan, which goes through the same validate/preview/apply pipeline as any other plan |
| Quarantine-only removal | The only "removal" the product performs is moving to a holding area | Set Aside preserves the original relative path so provenance is self-evident; the user empties it manually |

A Real (non-dry-run) apply against the actual library is additionally a **human-only gate** under every circumstance (D-10): no agent, script, or automation triggers it, regardless of how green the rest of the pipeline is. The pre-campaign backup decision (D-17) is a second human-only gate that must be recorded before any Real apply.

### 5.3 Validation hazard set (F-404, built and gate-proven)

The plan-validation stage (Section 3.5) is the pre-executor backstop. Its hostile-fixture gate seeded 12 hazards and proved every one blocked or warned with a pinned, stable machine code [S: v0.3.0 spec.md gate G-2]:

| # | Hazard | Verdict | Machine code |
|---|---|---|---|
| 1 | Two planned operations target the same path | blocked | `collision-in-plan` |
| 2 | Two planned targets differ only by letter case (NTFS-equivalent) | blocked | `collision-in-plan` (case-insensitive check) |
| 3 | A planned target already exists on disk | blocked | `collision-on-disk` |
| 4 | A move's source path sits inside its own target path | blocked | `cycle-detected` |
| 5 | Full path length exceeds the platform limit even with the `\\?\` allowance | blocked | `path-too-long` |
| 6 | Full path length is near 260 characters (interop risk with long-path-unaware tools) | warning | `path-too-long` (interop variant) |
| 7 | `LongPathsEnabled=0` detected and a target exceeds 260 characters | warning, with how-to link | `path-too-long` |
| 8 | A component uses a Windows-illegal character | blocked | `illegal-component` |
| 9 | A component is a reserved device name (`CON`, `COM1`, etc.) | blocked | `reserved-name` |
| 10 | A cross-volume move's summed byte estimate exceeds free space | blocked | `cross-volume-space-insufficient` |
| 11 | A plan's source snapshot has drifted (a source vanished since scan) | blocked | `snapshot-stale` |
| 12 | An approval action would run against zero approved operations | blocked | `nothing-approved` |

## 6. Storage and artifacts

### 6.1 SQLite tables, in plain terms

The app database lives at `%LOCALAPPDATA%\AudiobookOrganizer\abo.db` (never Roaming, never a OneDrive-synced path), in WAL mode [S: architecture.md Section 4].

| Table | Plain-language role | Built? |
|---|---|---|
| `scans` | One row per completed scan (or CSV import): where, when, how many entries, how many bytes | Yes |
| `entries` | One row per file or folder found in a scan | Yes |
| `classifications` | One row per folder: its class, why, and its parsed fields | Yes |
| `rulesets` | Named bundles of naming templates and structure policies | Yes |
| `plans` | One row per generated plan: which scan, which ruleset, when, status | Yes |
| `plan_ops` | One row per proposed change: source, target, group, rationale, confidence, validation verdict, provenance | Yes |
| `jobs` | One row per long-running operation (scan, hash, apply, rollback) and its state | Yes |
| `journal` | Append-only, one row per executed operation during an apply (not yet used, no executor exists) | Table shape reserved; not yet exercised |
| `manifests` | Completed apply jobs in reversible, exportable form (not yet used) | Table shape reserved; not yet exercised |
| `duplicate_groups` / `duplicate_members` | Candidate duplicate groups and their member files | Yes (candidate detection only; hash verification is v0.6.0) |
| `activity_records` | App-level audit trail: every scan/plan/apply/rollback with parameters and outcome | Yes |
| `settings` | One row: library root, set-aside root, reports folder, theme, snapshot retention | Reserved minimal shape from v0.1.0; the settings UI (F-803) is v0.4.0 |

### 6.2 Reports folder contents

Every export lands as a file in a `Reports/` folder beside the app data, plus anywhere the user picks [S: architecture.md Section 10, F-1002]. Recovery is designed to never depend on the app's own database being healthy: manifests export as JSON precisely so a sick database does not strand an undo. What exists today: plan exports (CSV, JSON, Markdown), the provenance report, and the dry-run HTML report. What arrives later: undo manifests (v0.5.0), post-apply verification reports (v0.5.0), and duplicate reports (v0.6.0).

### 6.3 Report file anatomy (F-506)

The dry-run HTML report is a single self-contained `.html` file, generated by `abo-core` with no GUI dependency, in a deliberately distinct "paper" theme (serif, print-friendly) rather than the app's own Day/Evening themes [S: docs/internal/releases/v0.3.0-planning/F-506-report-spec.md]. Its required sections, in order:

1. **Masthead and dateline** - product mark, human-readable date/time, plan and scan identifiers.
2. **Lead paragraph** - states plainly this is a preview, nothing has changed yet, how many files were read, how many changes were found, and that a decision list follows at the end.
3. **Seven-group summary table** - one row per campaign group (Section 3.4), with counts, size, and status.
4. **Before/after example tables** - 3 to 6 representative examples per group, noise struck through, with an "and N more" pointer to the full table.
5. **Warnings needing a decision** - a callout naming every book that needs the user's call before anything touches it.
6. **What will not happen** - the FD-10 guarantee block, verbatim: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone."
7. **Provenance** - every flattened pack/collection and its members, including validation-blocked members.
8. **Complete change-list table** - one row per plan operation, no row cap, no pagination (it is a static file).
9. **Signature line** - generated-locally statement, plan/scan identifiers, total row count.

## 7. What is deliberately NOT in the product

Non-goals, with the reasoning behind each [S: product-requirements.md Section 8; architecture.md Section 11]:

| Non-goal | Why |
|---|---|
| Not a media player | Audiobookshelf already does this; the tool only reorganizes files for ABS to serve |
| Not an ABS replacement | The tool complements ABS and never replaces it (D-03) |
| Not a metadata editor | No tag writing anywhere in v1; structure comes first (F-1106 deferred) |
| Not a downloader | Out of scope entirely; no acquisition workflow exists or is planned |
| No online metadata lookup | Folder-first parsing plus the FD-14 probe showed folder names are already the more complete, more consistent source (Section 2); an online lookup would also break the zero-network invariant |
| No tag writing | The FD-14 probe's read is strictly measurement; F-1106 (tag writing) is explicitly deferred past v1 |
| No deletion of anything | Set-aside (move, never delete) is the only removal mechanism; the deletion guarantee is load-bearing product identity (D-09), not a cautious default |
| No cloud sync | Everything is local and inspectable; a footer in the eventual GUI states this |
| No multi-user | Single-writer rule assumes one operator, one apply job, process-wide |
| No AI-driven renaming without deterministic rules | Every rename traces to a named pattern match and a rule ID; nothing is a model guess presented as fact |
| No ABS-side changes or tag writes (FD-12) | Pack and award membership is preserved in the provenance report instead of pushed into ABS; no v1 copy may promise otherwise |
| No auto-planning of video/course content (FD-17) | Video-dominated and course folders route to `manual-review` and are never auto-planned; a wrong guess on non-book content is worse than asking |
| No hash-everything on scan | BLAKE3 hashing (F-702, v0.6.0) runs only over duplicate candidates, opt-in, never over the whole 297 GB on every scan |
| No hardlink dedupe | Hardlinks would violate the "one book, one place, fully undoable" model and would confuse ABS |
| No auto-update in v1 | Fully offline posture; users manually download new installers through v0.9.0 and beyond (FD-22) |

## 8. Known gaps and recorded debt

Honest accounting of what is open, pulled directly from the specs' own recorded items rather than smoothed over:

- **Tag approval is still pending for every built release.** v0.1.0, v0.2.0, and v0.3.0 are each recorded as "built, gate walked by Fable, tag awaiting jp per D-10." Tag-cutting is a human-only action; nothing in the pipeline blocks it, but it has not happened yet [S: program-roadmap.md Section 8].
- **The v0.3.0 non-engineer read test (gate G-6) is the one open item on the newest release.** The real 1,817-change HTML report has been delivered and render-verified by the orchestrator, but jp's own confirmation read is recorded as pending and is explicitly non-blocking for merge, though the tag should follow it [S: v0.3.0 spec.md gate G-6].
- **No GUI exists yet.** Every capability described in Sections 3 and 4 as BUILT is exercised through the frozen tauri-specta IPC contract, integration tests, and the exported report/CSV/JSON/Markdown files; the only front end that has ever run is the v0.1.0 throwaway JSON-dump UI, which is explicitly disposable and is deleted when v0.4.0 lands [S: v0.1.0 spec.md Scope item 8].
- **No executor exists yet.** Nothing described in Section 5.2 (the v0.5.0 safety invariants) has been built or tested against real code; it is a specified contract, not yet a proven one. The dry-run report is real and proven; an actual file move is not yet possible in this codebase.
- **mp4 audio-vs-video ambiguity is an accepted, documented gap.** `.mp4` types as `video` unconditionally because extension alone cannot distinguish an audio-in-mp4 audiobook from an actual video file; container-level inspection was considered and explicitly deferred, not overlooked [S: v0.1.0 spec.md, F-103 requirement].
- **Duplicate resolution is candidate-only.** F-701 (built) finds 406 groups / 856 copy files by exact basename+size match, but no content hash has ever been computed; the true duplicate volume in bytes is unknown until F-702 (hash verification, v0.6.0) runs. No duplicate group is auto-resolved today or planned to be without either a verified hash or an explicit user override.
- **F-507 (pack provenance) is half-built.** Plan-time capture and the provenance report both work and are proven on the real library; carrying that provenance through the journal and manifest, and re-emitting the report after an apply, is specified for v0.5.0 and has not been built.
- **Snapshot retention is reserved, not enforced.** The `settings.snapshot_retention_n` column exists with a default of 10, but the sweep that actually deletes old scans to bound database growth is tied to the F-803 settings surface, which is v0.4.0 and not yet built [S: v0.1.0 spec.md Open Question OQ-2].
- **Reports-folder default path is provisional.** v0.3.0 writes exports beside `%LOCALAPPDATA%\AudiobookOrganizer\` by default; the real, user-configurable default is finalized when F-803 (app settings) lands in v0.4.0 [S: v0.3.0 spec.md Open Questions].
- **Drift-tolerance between baseline and live scans has no fixed numeric band.** The v0.2.0 gate deliberately reports library drift rather than judging it (a library changes between measurements), and the exact tolerance for when drift should be treated as a defect rather than expected change was left as a jp gate-review item (OQ-2) rather than hard-coded [S: v0.2.0 spec.md Open Questions; baseline-delta-2026-07-04.md].
- **Parser coverage is monitored, not locked.** The pattern set stayed open at 95.2% fixture coverage rather than freezing at the 90% threshold; if real-library coverage ever regresses below that threshold, the pre-agreed descope action is to freeze the pattern set and route the remainder to `manual-review` by design, not silently guess [S: program-roadmap.md Section 5, descope triggers].
