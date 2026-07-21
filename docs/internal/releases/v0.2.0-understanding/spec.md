---
id: v0.2.0
title: "Release v0.2.0 (understanding): scan, classify, parse"
date: 2026-07-03
status: review
owner: jprisant
type: release-spec
tier: engine-only (no GUI surfaces; abo-core hardening behind the frozen tauri-specta seam)
scope: release
depends_on: docs/internal/releases/v0.1.0-spine/spec.md
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.2.0, Section 6.4 test strategy)
  - _local/planning/feature-function-breakdown_2026-07-02.md (E-01 scan and ingest, E-02 classification, E-03 parsing and normalization, E-10 observability)
  - _local/planning/audiobook-organizer-strategy-brief_2026-07-02.md
  - _local/initial-discovery/ (naming patterns, classification buckets, 2026-03-25 baselines)
  - _local/prior-work/WizTree_2026-03-25.csv
  - docs/internal/test-strategy.md
  - docs/internal/decision-ledger.md decisions D-07, D-08, D-09, FD-14, FD-17, FD-18, FD-19, FD-20, FD-27, FD-30
---

# Spec: Release v0.2.0 (understanding) - scan, classify, parse

## Task Summary

Status: built; gate walked 2026-07-04; tag awaiting jp per D-10.
Release theme: the tool can look at a real library and tell the truth about it, headlessly, on hardened fixtures first and the real 297 GB tree second.
Scope: fixture harness (built first), F-101 (live tree scanner) hardening, F-102 (WizTree CSV import), F-104 (job progress + cancel), the classification engine (F-201, F-202, F-203), the full parse stack (F-301, F-302, F-303, F-304), and F-1001 (activity log).
Open questions: 2 (see Open Questions).
Last updated: 2026-07-03.

Feature AC roll-up (all unchecked at review time):
- [ ] Fixture harness AC-F1..AC-F5
- [ ] F-101 (live tree scanner) AC-101.1..AC-101.6
- [ ] F-102 (WizTree CSV import) AC-102.1..AC-102.3
- [ ] F-104 (job progress + cancel) AC-104.1..AC-104.4
- [ ] F-201 (folder classification engine) AC-201.1..AC-201.6
- [ ] F-202 (library health metrics) AC-202.1..AC-202.3
- [ ] F-203 (multi-book folder detection) AC-203.1..AC-203.3
- [ ] F-301 (pattern matcher set) AC-301.1..AC-301.4
- [ ] F-302 (noise strippers) AC-302.1..AC-302.4
- [ ] F-303 (field extraction with confidence) AC-303.1..AC-303.3
- [ ] F-304 (name normalizer) AC-304.1..AC-304.5
- [ ] F-1001 (activity log) AC-1001.1..AC-1001.2
- [x] Release gate G-01..G-09 (walked by Fable 2026-07-04; see Release Gate section)

## Context

v0.1.0 (spine) proved the architecture once with a tracer bullet: a Cargo workspace with a Tauri-free `abo-core`, SQLite via sqlx with WAL and the first migration, a minimal F-101 (live tree scanner) plus F-103 (file typing) and F-105 (snapshot persistence), structured logging (F-1003), pinned tauri-specta bindings, and green CI. This release consumes that skeleton and fills the first two pipeline stages with real capability: it turns a folder tree (or a WizTree CSV) into a queryable snapshot, then classifies every folder and parses every name into structured fields with explicit confidence.

Per D-07 (engine-first order), abo-core hardens on fixtures before any GUI exists; per D-09 (safety invariants), everything here is read-only analysis (no executor, no mutation of the library). The single highest-leverage asset built here is the fixture generator: a synthetic library that lets classification and parsing be golden-tested exhaustively without touching the real drive. This release ends with the first fresh, honest look at the library since the 2026-03-25 baseline (FD-18), and that measured drift is itself a deliverable.

Nothing here ships a user-facing surface. The v0.1.0 throwaway UI is the only front end; it renders JSON dumps. The plain-language register (PRODUCT.md) still governs any strings that surface in health metrics and the activity log, because those strings flow into v0.4.0 (seeing) unchanged.

## In Scope

### Fixture harness (built FIRST)

What it is: a `fixtures` generator (Rust bin or build script under `crates/abo-core/fixtures/` or a workspace `fixtures/` crate) that materializes a synthetic library from a declarative manifest. It is the test bedrock for every downstream golden test in this release and later releases. Per docs/internal/test-strategy.md Section 2, the generated tree is written to a temp dir at test time and never committed to git (Windows checkouts must never break on near-limit path lengths).

Key behaviors: the manifest declares folder and file shapes; the generator writes placeholder file contents at real declared sizes so health-metric byte totals are checkable. Fixture families cover: all 9 naming patterns (F-301) with real examples lifted from discovery; deep Hugo-style nesting (depth 5 or more); mixed folders (direct audio plus child folders); multi-book folders (Narnia-style 7 files, Harry Potter-style 11 files spanning 7 titles); nonconforming disc folders; parallel-format `0 M4B` cases; Unicode names in both NFC and NFD forms of the same title; near-limit path lengths generated at runtime; reserved-name near-misses (`CON`, `COM1`, trailing dots/spaces); zero-byte samples; exact-duplicate pairs; and per FD-17, a video/course cluster (the Zig Ziglar "52 Sales Lessons" mp4 case, cbr/cbz comics, and a radio-play folder) plus per FD-01 a pack-with-provenance shell (Hugo/Nebula membership captured on member books) so v0.3.0 provenance fixtures already exist.

Edge cases: generation must be deterministic (same manifest = same tree, byte-identical) so golden snapshots are stable; the generator must refuse to write outside its temp root; over-length and reserved-name fixtures are produced only where the host OS permits, and skipped-with-note otherwise so the suite is green on both Windows and the macOS/Linux CI runners.

Acceptance criteria:
- [ ] AC-F1: `cargo test` materializes the full fixture library into a temp dir and tears it down cleanly; nothing is written under the repo working tree and no fixture with a near-limit path is committed to git. [S: test-strategy.md Section 2; release plan Section 4 v0.2.0]
- [ ] AC-F2: the manifest declares at least one example of each of the 9 naming patterns, deep nesting (depth >= 5), mixed, multi-book (Narnia and Harry Potter shapes), nonconforming disc, parallel-format `0 M4B`, NFC/NFD Unicode pairs, reserved-name near-misses, zero-byte samples, and exact-duplicate pairs. [S: release plan Section 4 v0.2.0]
- [ ] AC-F3: per FD-17, the manifest includes a video/course cluster (mp4 course, cbr/cbz comics, radio play) and, per FD-01, a pack shell carrying source-pack membership on member books. [S: FD-17, FD-01]
- [ ] AC-F4: generation is deterministic - regenerating from the same manifest yields a byte-identical tree (verified by recursive compare in a test). [S: NFR Determinism, breakdown Section 9]
- [ ] AC-F5: declared file sizes are honored so health-metric byte totals computed over the fixture match manifest-declared totals exactly. [S: F-202 acceptance]

### F-101 (live tree scanner)

What it is: `walkdir`-based recursive traversal of a chosen root, capturing per entry: path, kind, size, mtime, depth, and parent linkage, persisting into the `entries` table of an immutable snapshot (F-105 from v0.1.0). This release hardens the minimal v0.1.0 walker against the real-world edge list, incorporating FD-19 (Windows path reality).

Key behaviors: extended-length (`\\?\`) path semantics on Windows so paths beyond 260 chars open and scan; permission-denied entries are recorded (as an `AppError::permission-denied(path)` note on the entry) and the scan continues, never aborts; reparse points and junctions are recorded but never followed (junction loops must terminate); names are case-preserving; excluded roots and glob patterns come from the ruleset. Per FD-19, when a target path exceeds 260 chars and `LongPathsEnabled=0` is detected, the scan records a warning with a linked how-to; near-260 paths keep an interop warning.

Edge cases: junction loop (A points into B points into A) terminates without infinite recursion; a permission-denied subtree contributes what it can and flags the rest; OneDrive placeholder files (reparse points) are recorded, never hydrated; a root that is not a directory or does not exist fails cleanly with the taxonomy error, not a panic.

Acceptance criteria:
- [ ] AC-101.1: scanning the fixture library yields exact expected entry counts and byte totals (insta golden). [S: F-101 acceptance sketch; breakdown Section 5 E-01]
- [ ] AC-101.2: a fixture tree containing a junction loop terminates and records the junction without following it; a test asserts termination and the `junction-skipped(path)` record. [S: FD-19; breakdown Section 5 F-101; release plan gate]
- [ ] AC-101.3: a permission-denied entry is recorded and the scan continues to completion (test seeds a denied path where the OS allows; skipped-with-note otherwise). [S: FD-19; breakdown F-101]
- [ ] AC-101.4: paths beyond 260 chars are scanned via extended-length semantics; when `LongPathsEnabled=0` is detected on Windows, a warning with a how-to link is recorded. [S: FD-19]
- [ ] AC-101.5: names are case-preserving in the snapshot; NFC and NFD Unicode inputs are both captured (normalization is F-304's job, not the scanner's). [S: FD-19; F-304]
- [ ] AC-101.6: `root-not-found` and `root-not-directory` return the taxonomy error, never a panic. [S: breakdown Section 8 error taxonomy]

### F-102 (WizTree CSV import)

What it is: an alternate snapshot source that parses the WizTree export format (file name, size, allocated, modified, attributes) into the same `entries` schema as F-101, flagged `source = csv`. The rescued source CSV lives at `_local/prior-work/WizTree_2026-03-25.csv`.

Key behaviors: produces a snapshot indistinguishable downstream from a live scan except for the `source` flag, so classification and parsing run identically on it; lets planning start from the existing 2026-03-25 snapshot for cheap what-if analysis without touching the drive. Parent linkage is reconstructed from the path column.

Edge cases: a malformed CSV row fails with `csv-parse(row)` identifying the row, without aborting the whole import where recovery is safe; attributes column maps to kind/junction flags consistently with the live scanner; the rescued 2026-03-25 CSV imports without error and its counts feed the baseline comparison.

Acceptance criteria:
- [ ] AC-102.1: importing `_local/prior-work/WizTree_2026-03-25.csv` produces a valid snapshot with `source = csv` and reconstructed parent linkage. [S: release plan Section 4 v0.2.0; FD-18]
- [ ] AC-102.2: a snapshot imported from CSV classifies and parses through the identical downstream code path as a live scan (test runs both on an equivalent fixture and compares classifications). [S: breakdown F-102]
- [ ] AC-102.3: a malformed row raises `csv-parse(row)` with the offending row index; a fixture CSV with one bad row is covered. [S: breakdown Section 8 error taxonomy]

### F-104 (job progress + cancel)

What it is: the job model wrapping all long operations (in this release: scan and CSV import). Jobs spawn on the Tokio runtime, emit `job:progress` events (items done, total estimate, current path), honor a cancellation token at safe boundaries, and persist a `jobs` row so a crashed job is visible on restart.

Key behaviors: progress events fire without freezing anything (all work off the UI thread); cancellation takes effect at a safe boundary (between entries during a scan), never mid-operation; a cancelled scan leaves a coherent partial or discarded snapshot per the documented semantics (no half-written entry rows treated as complete). Per FD-02, this is the real Stop control semantic (cooperative cancel at safe boundaries); the prototypes' "Skip ahead" is demo-only and never ships.

Edge cases: cancel requested before the job starts is a no-op with a clear status; cancel during scan stops at the next entry boundary and the `jobs` row records `cancelled`; a process kill mid-scan leaves the `jobs` row visible as not-completed on restart (full apply-side reconciliation is v0.6.0, out of scope here).

Acceptance criteria:
- [ ] AC-104.1: a scan job emits `job:progress` events carrying items-done, total-estimate, and current path; a test observes monotonically non-decreasing progress. [S: breakdown F-104; IPC surface]
- [ ] AC-104.2: a cancellation token honored at an entry boundary stops the scan; the `jobs` row records `cancelled`; a test asserts the scan halted before completion. [S: breakdown F-104; FD-02]
- [ ] AC-104.3: cancellation never interrupts mid-entry (no torn snapshot rows); the snapshot left behind is coherent per documented cancel semantics. [S: breakdown F-104; D-09]
- [ ] AC-104.4: a `jobs` row persists across process restart and is visible as not-completed after a killed scan. [S: breakdown F-104]

### F-201 (folder classification engine)

What it is: assigns a `FolderClass` to every folder, evaluated deterministically bottom-up (children before parents). Classes (the Codex taxonomy): `book`, `series-container`, `pack-container`, `staging`, `mixed`, `multi-book-suspect`, `empty`, `docs-resources`, `manual-review`. Every classification records why (rule id plus evidence JSON) so the UI can later explain itself.

Key behaviors: a folder with audio files and no child folders is a `book` candidate; a folder whose children are book folders sharing a parsed series is a `series-container`; direct audio plus child folders is `mixed`; no audio anywhere beneath is `empty`; staging areas (`_sort`, `_process`) are `staging`. Per FD-17, folders dominated by video/course content (mp4 courses, cbr/cbz comics) and radio plays route to `manual-review` and are never auto-planned. `manual-review` is a first-class outcome, not an error.

Edge cases: a folder unclassifiable by any rule becomes `manual-review` (designed behavior); the video/course cluster (52 Sales Lessons) and radio plays classify as `manual-review` even though they contain audio-class files; `docs-resources` folders (only ebooks/nfo/images, no audio) are distinct from `empty`; classification is deterministic and order-independent given a fixed snapshot.

Acceptance criteria:
- [ ] AC-201.1: every folder in the fixture library receives exactly one `FolderClass` matching golden expectations (insta snapshot). [S: release plan gate; breakdown F-201]
- [ ] AC-201.2: each classification records a rule id and evidence JSON explaining why. [S: breakdown F-201]
- [ ] AC-201.3: per FD-17, the video/course cluster and radio-play fixtures classify as `manual-review`, not `book`. [S: FD-17; planning audit stream 1 finding 4, docs/internal/planning-audit-2026-07-03.md]
- [ ] AC-201.4: an unclassifiable fixture folder classifies as `manual-review` (first-class outcome, no error raised). [S: breakdown F-201]
- [ ] AC-201.5: classification is deterministic and bottom-up: children classify before parents; same snapshot yields byte-identical classification output. [S: NFR Determinism; breakdown F-201]
- [ ] AC-201.6: `empty` (no audio anywhere beneath) is distinguished from `docs-resources` (non-audio content present) in golden expectations. [S: breakdown F-201]

### F-202 (library health metrics)

What it is: aggregate counts and byte totals per class and per problem type (loose root books, noisy names, deep nesting depth >= 5, duplicates, empties). These are the numbers that drive the later library home (F-902) and the campaign progress meter.

Key behaviors: metrics are computed over a snapshot and are re-computable so re-scanning after an apply (later releases) shows them moving toward zero; each metric names the quantity it counts (folders vs files vs bytes) per FD-08 (state which quantity any GB figure refers to). Copy that surfaces these numbers follows the plain-language register (numbers inside sentences).

Edge cases: an already-tidy fixture yields zeroed problem metrics without error; byte totals match fixture-declared sizes exactly (AC-F5); the duplicate count here is candidate-group count, sized but not resolved (resolution is v0.6.0).

Acceptance criteria:
- [ ] AC-202.1: health metrics over the fixture library match golden expectations for counts and byte totals per class and per problem type. [S: release plan gate; breakdown F-202]
- [ ] AC-202.2: every metric declares its unit (folders, files, or bytes); no bare GB figure is emitted without stating what it counts. [S: FD-08]
- [ ] AC-202.3: an already-tidy fixture yields zeroed problem metrics with no error. [S: breakdown F-202; FD-04 already-tidy edge (state authored in v0.4.0)]

### F-203 (multi-book folder detection)

What it is: heuristics that flag folders holding several complete books as sibling files. Output feeds the later plan builder's split proposals.

Key behaviors: N sibling audio files whose names parse as distinct titles (Narnia: 7 m4b, 7 books) flag as `multi-book-suspect`; numbered same-series files at folder level (Wings of Fire) flag distinctly; the known hard case (Harry Potter's 11 direct mp3 files spanning 7 titles plus extras) is detected as multi-book, not misread as one book.

Edge cases: a legitimate single book split into `Disc NN` subfolders is NOT multi-book (that is disc structure, F-204, v0.3.0); a folder with one book plus a bonus/sample file is not falsely flagged; ambiguous cases surface as `multi-book-suspect` for review rather than auto-splitting.

Acceptance criteria:
- [ ] AC-203.1: the Narnia-style fixture (7 sibling audio files, distinct parsed titles) is flagged `multi-book-suspect`. [S: breakdown F-203]
- [ ] AC-203.2: the Harry Potter-style fixture (11 direct mp3 files spanning 7 titles plus extras) is detected as multi-book, not a single book. [S: breakdown F-203]
- [ ] AC-203.3: a single book with a bonus file, and a disc-based single book, are NOT flagged multi-book (no false positive). [S: breakdown F-203; F-204]

### F-301 (pattern matcher set)

What it is: one matcher per discovery naming pattern (9 total), run in specificity order; each returns extracted fields plus a match score. All matchers are pure functions, table-driven-tested against real examples lifted from discovery.

Key behaviors: the 9 patterns are (1) `Title by Author` (the 238 root files, 237 parse cleanly), (2) `Author - Title`, (3) `Title by Author` folder variant, (4) `Year - Author - Title (Series #N) [noise]` with `^` award marker (Hugo/Nebula), (5) `N - Title - Author - Year` (Top 100), (6) `NN.N - Title [noise]` series entries, (7) `Author_Name_-_Title` underscored, (8) bare `Title`, (9) `Author - Series` containers with irregular separators (`Frank Herbert-Dune-#1-Chronicles[1-8}`). Matchers run in specificity order; ties surface as `ambiguous` for review.

Edge cases: the one root file of 238 that does not parse cleanly falls through to `manual-review`/`ambiguous`, not a wrong parse; the award marker `^` is captured as provenance signal (feeds FD-01 pack provenance in v0.3.0), not stripped as noise; conflicting matches at equal score return `ambiguous`.

Acceptance criteria:
- [ ] AC-301.1: each of the 9 matchers correctly extracts fields from its table of real discovery examples (table-driven tests). [S: breakdown F-301; discovery docs]
- [ ] AC-301.2: matchers run in specificity order; a name matchable by two patterns resolves to the more specific one, and equal-score ties return `ambiguous`. [S: breakdown F-301]
- [ ] AC-301.3: the 237-of-238 loose-root parse expectation holds on the corresponding fixture (labeled 2026-03-25 baseline); the 1 non-clean file surfaces as `ambiguous`/`manual-review`, not a wrong parse. [S: FD-18; discovery]
- [ ] AC-301.4: parser fixture coverage is measured and reported; if coverage falls below ~90% the pattern set is frozen and the remainder routes to `manual-review` (descope trigger, designed behavior). [S: release plan Section 5; brief AC additions]

### F-302 (noise strippers)

What it is: composable, individually toggleable passes that remove ripper tags, bitrate/size markers, rank/year prefixes, release-group suffixes, and convert underscores to spaces, using the discovery regexes as the starting point. Idempotence is a hard property test.

Key behaviors: trailing `[...]` bracket tags (203 folders, 2026-03-25 baseline), bitrate markers (170), size markers (214), rank prefixes (143), year prefixes (116, extracted as a field before stripping), release-group suffixes (` jZQ`, `[Thomas]`), underscores-to-spaces. Each pass is independently toggleable. Year is kept as a field before its prefix is removed.

Edge cases: `strip(strip(x)) == strip(x)` for every fixture name (idempotence); a bracket that is part of a real title (rare) is handled by pass ordering and covered by a fixture; stripping never deletes a whole name to empty (a name that is entirely noise is preserved or flagged, not zeroed).

Acceptance criteria:
- [ ] AC-302.1: each stripper pass removes its target noise on the discovery-derived fixtures and leaves non-noise intact (table-driven). [S: breakdown F-302; discovery]
- [ ] AC-302.2: idempotence property test passes across all fixture names: `strip(strip(x)) == strip(x)` (proptest). [S: release plan gate; breakdown F-302]
- [ ] AC-302.3: year prefixes are extracted as a field before stripping (no year data lost); the 116-folder baseline is labeled 2026-03-25. [S: breakdown F-302; FD-18]
- [ ] AC-302.4: no stripper reduces a name to empty; an all-noise name is preserved or flagged, not zeroed. [S: breakdown F-302; model-inference]

### F-303 (field extraction with confidence)

What it is: merges matcher output up the tree (a file inside a parsed series folder inherits series/author context) and emits per-field confidence: high (explicit in the name), medium (inherited from parent), low (guessed). Plans later surface low-confidence fields for review.

Key behaviors: fields are title, author, series, index, year, narrator; a file inside a parsed `series-container` inherits series and author at medium confidence; explicitly named fields are high confidence; guessed fields are low. Tag-based extraction is deliberately absent in v1 (F-1101 deferred); the folder-first assumption is stated per FD-14, superseding the discovery doc's `preferSource=tags` default, with confidence tied to the FD-14 probe verdict.

Edge cases: a field present in both name and inherited context prefers the explicit (name) value at high confidence; conflicting inherited vs explicit values surface the explicit and flag the conflict; a file with no parseable field and no inheritable context yields low-confidence or empty fields, never a fabricated value.

Acceptance criteria:
- [ ] AC-303.1: fields extracted from fixtures carry per-field confidence (high/medium/low) matching golden expectations. [S: breakdown F-303]
- [ ] AC-303.2: a file inside a parsed series-container inherits series and author at medium confidence; an explicit in-name field overrides an inherited one at high confidence. [S: breakdown F-303]
- [ ] AC-303.3: the folder-first assumption is the documented default (superseding `preferSource=tags`), with confidence weighting tied to the FD-14 tag-quality probe verdict (see release gate G-08). [S: FD-14; planning audit stream 1 finding 9]

### F-304 (name normalizer)

What it is: produces final path components from extracted fields: collapse whitespace, normalize separators, strip Windows-illegal characters, forbid reserved device names, forbid trailing dots/spaces, enforce a max component length with word-boundary truncation, and apply Unicode NFC normalization.

Key behaviors: illegal characters `<>:"/\|?*` stripped; reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`) forbidden; trailing dots/spaces removed (Win32 strips them silently); max component length configurable, default 120 chars, truncated at a word boundary; NFC normalization so visually identical NFC/NFD names compare equal. This is normalization only in this release; the plan-side path-safety backstop is F-404 (v0.3.0).

Edge cases: NFC and NFD inputs for the same title normalize to the identical component; a name that is exactly a reserved device name gets a safe suffix or is flagged; truncation never splits a multi-byte grapheme; an all-illegal-character name normalizes to a safe non-empty placeholder or is flagged, never emitted as empty.

Acceptance criteria:
- [ ] AC-304.1: illegal Windows characters and trailing dots/spaces are removed from produced components (table-driven). [S: breakdown F-304; FD-19]
- [ ] AC-304.2: reserved device names are detected and made safe (suffixed or flagged), never emitted verbatim. [S: breakdown F-304]
- [ ] AC-304.3: NFC and NFD forms of the same title normalize to a byte-identical component. [S: breakdown F-304; FD-19]
- [ ] AC-304.4: component length is capped at the configured default (120) with word-boundary truncation that never splits a grapheme. [S: breakdown F-304]
- [ ] AC-304.5: normalization never yields an empty component; an all-illegal name produces a safe placeholder or a flag. [S: breakdown F-304; model-inference]

### F-1001 (activity log)

What it is: an append-only record (`activity_records` table) of every scan, import, classify, and parse run with its parameters and outcome. The audit trail the product exposes in the UI footer later.

Key behaviors: one record per action with action name, params JSON, outcome, and timestamps; no telemetry, no network (records are local and inspectable). In this release the logged actions are scan, CSV import, classify, and parse runs.

Edge cases: a failed run records the failure outcome and error code, not just successes; the log is append-only (no update/delete of prior records in normal operation).

Acceptance criteria:
- [ ] AC-1001.1: every scan, import, classify, and parse run appends one `activity_records` row with action, params JSON, outcome, and timestamps. [S: breakdown F-1001; data model Section 7]
- [ ] AC-1001.2: a failed run records its failure outcome and error code; the log is append-only (a test asserts no prior row is mutated). [S: breakdown F-1001; error taxonomy]

## Out of Scope

- Planning, validation, exports, rulesets, HTML report: F-401..F-405, F-505, F-506, F-701, F-801, F-1002 land in v0.3.0 (planning). See docs/internal/releases/v0.3.0-planning/spec.md.
- Disc-structure and parallel-format detection (F-204, F-205): v0.3.0. Fixtures for both are generated here, but the detection features ship next release.
- Any GUI surface (F-901..F-906) and app settings (F-803): v0.4.0 (seeing). The only front end this release is the v0.1.0 throwaway UI.
- Executor, journal, rollback, quarantine, dry-run harness (E-06): v0.5.0 (acting). This release performs zero mutations; it is read-only analysis only (D-09).
- Duplicate resolution, hashing (F-702..F-704): v0.6.0. This release only sizes candidate groups as a health metric; it does not resolve them.
- Embedded full tag reading and cover extraction (F-1101, F-907): the FD-14 probe here reads a bounded lofty subset read-only for measurement only; general tag-based extraction and cover rendering are v0.4.0+ per D-15/FD-03.
- Provenance capture and report (F-507): v0.3.0. Fixtures carry pack membership now (AC-F3) so the v0.3.0 feature has data.

## Release Gate

Composite checklist (release plan Section 4 v0.2.0, upgraded per FD dispositions). Evidence pointers follow docs/internal/test-strategy.md conventions (name the test suite; CI job that runs it). All must be green before v0.2.0 tags.

Gate walked by Fable, 2026-07-04. Every phase passed an independent adversarial task review with all Critical/Important findings fixed and re-verified. Evidence docs: baseline-delta-2026-07-04.md and fd14-tag-probe-2026-07-04.md in this folder.

- [x] G-01: fixture library scans, classifies, and parses to golden expectations (12 insta goldens + parse tables, green locally and in CI). [F-201, F-202, F-301, F-303 AC]
- [x] G-02: stripper idempotence proven by proptest across composed-realistic and arbitrary-unicode generators; the NFC-ordering flake found during P5 was fixed (strip before NFC) with a deterministic seed regression; proptest module re-run 5x at 4096 cases clean. [F-302 AC-302.2]
- [x] G-03: real-library read-only scan completed in under 1 s (warm cache; two orders of magnitude inside the soft 60 s target), 14,799 entries / 320.8 GB / 0 skipped / 113 NearMaxPathInterop warnings (all deep Hugo/Nebula paths, all scanned via extended-length opens). Baselines vs CSV vs live recorded three-column in baseline-delta-2026-07-04.md with every figure FD-18-labeled; headline: loose-root 237/238 parses and the corrected loose-root metric read 238 EXACTLY on live data; book-like 582 -> 582. Drift reported, not judged, per OQ-2 resolution below. [NFR Scale; FD-18]
- [x] G-04: junction-loop termination, permission-denied continue, and cancel-at-boundary all covered by named tests (P2 review verified each). [F-101, F-104]
- [x] G-05: parser coverage 95.2% (20/21; sole miss is the documented AC-301.3 outlier), above the ~90% freeze threshold: NO freeze; pattern set stays open. Decision recorded here. [F-301 AC-301.4]
- [x] G-06: FD-17 routing tested directly (52 Sales Lessons mp4 cluster, comics, radio play all manual-review). [F-201 AC-201.3]
- [x] G-07: the rescued 2026-03-25 WizTree CSV imports to the EXACT discovery totals (14,689 entries, 319,290,437,409 bytes, 719 dirs, 13,970 files, 0 skipped) and feeds the G-03 comparison. [F-102 AC-102.1]
- [x] G-08: FD-14 probe run over a 300-file deterministic sample, read-only (lofty behind an optional feature; absent from default builds; no tauri or HTTP client in its graph). Verdict: folder-first HOLDS (folders carry title/author for 100% of the sample vs tags 85%/96%); F-303 default weighting unchanged; OQ-1 not reopened. Full numbers in fd14-tag-probe-2026-07-04.md. [FD-14]
- [x] G-09: full CI matrix green on every merged PR of this release (runs on PRs #6, #7+fix, #9, #10, #11) and on this gate branch's PR at merge time. Two linux-clippy cfg-gating incidents were caught by the matrix and fixed forward; the cfg rule now rides in every implementer brief. [FD-24]

## Source Traceability

| Feature / gate | Discovery / planning source | D/FD decisions |
|---|---|---|
| Fixture harness | release plan Section 4 v0.2.0; test-strategy.md Section 2 | FD-17 (video/course fixture), FD-01 (pack-provenance fixture), FD-27 (sample-data rule) |
| F-101 (live tree scanner) | breakdown E-01 (scan and ingest) F-101; release plan Section 4 v0.2.0 | FD-19 (extended-length, junctions, permission-denied, case-preserving), D-09 (read-only) |
| F-102 (WizTree CSV import) | breakdown E-01 F-102; `_local/prior-work/WizTree_2026-03-25.csv` | FD-18 (baseline source) |
| F-104 (job progress + cancel) | breakdown E-01 F-104; IPC surface Section 6 | FD-02 (real Stop / cooperative cancel; no Skip-ahead) |
| F-201 (folder classification engine) | breakdown E-02 (classification) F-201; Codex classification buckets | FD-17 (video/course to manual-review), D-07 (engine-first) |
| F-202 (library health metrics) | breakdown E-02 F-202 | FD-08 (state the counted quantity), FD-18 (baselines) |
| F-203 (multi-book folder detection) | breakdown E-02 F-203; discovery Narnia/HP cases | - |
| F-301 (pattern matcher set) | breakdown E-03 (parsing and normalization) F-301; discovery 9 patterns | FD-18 (237/238 baseline), FD-27 (sample data) |
| F-302 (noise strippers) | breakdown E-03 F-302; discovery regex recipes | FD-18 (203/170/214/143/116 baselines) |
| F-303 (field extraction with confidence) | breakdown E-03 F-303 | FD-14 (folder-first supersedes preferSource=tags) |
| F-304 (name normalizer) | breakdown E-03 F-304 | FD-19 (Windows path/name reality) |
| F-1001 (activity log) | breakdown E-10 (observability) F-1001; data model Section 7 | - |
| Release gate | release plan Section 4 v0.2.0, Section 5 | FD-14, FD-17, FD-18, FD-24 |

## Open Questions

- OQ-1: F-303 confidence weighting for the folder-first default is tuned by the FD-14 probe verdict (G-08). If the probe finds embedded tags materially more complete than folder names on a large fraction of the real library, does that reopen the preferSource default for v1, or is it strictly deferred to F-1101 (v1.1)? Recorded here; jp decides at the G-08 gate. [FD-14]
- OQ-2: exact drift tolerance band for G-03 (how far the fresh scan may diverge from the 2026-03-25 baselines before the divergence is treated as a defect rather than expected library change). Proposed: report the delta, do not fail the gate on drift alone since the library has changed since March; confirm at gate review. [FD-18]

## Revisions

| Date | Change | By |
|---|---|---|
| 2026-07-03 | Initial release-level spec authored for the planning suite. | jprisant (author agent) |
