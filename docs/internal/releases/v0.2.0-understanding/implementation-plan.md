---
id: v0.2.0
title: "Implementation Plan - Release v0.2.0 (understanding): scan, classify, parse"
date: 2026-07-03
status: review
owner: jprisant
type: implementation-plan
linked-spec: docs/internal/releases/v0.2.0-understanding/spec.md
linked-release: docs/internal/releases/v0.2.0-understanding/
depends_on: docs/internal/releases/v0.1.0-spine/implementation-plan.md
phase-count: 7
ac-coverage: complete
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - docs/internal/test-strategy.md
  - docs/internal/decision-ledger.md decisions D-07, D-09, FD-14, FD-17, FD-18, FD-19, FD-24, FD-30
executor_model_guidance: >
  Per FD-30: Opus-tier owns the classification engine, the scanner edge-case
  hardening (FD-19 Windows path reality), and the confidence-merge logic in F-303
  (correctness-sensitive, complex). Sonnet-tier owns the fixture generator plumbing,
  the table-driven matcher/stripper tests, CSV parsing, and the activity-log CRUD
  (mechanical, table-shaped). Fable reviews every phase decision gate and owns the
  final release-gate verification (G-01..G-09).
---

# Implementation Plan: Release v0.2.0 (understanding) - scan, classify, parse

## Task Summary

Status: review. This plan decomposes v0.2.0 into 7 phases: fixture harness first, then scanner + job model, CSV import, the parse stack, the classification engine, the activity log, and the real-library read-only gate with the FD-14 tag-quality probe. Every phase is test-first where practical: the golden and property tests are written before or alongside the code that satisfies them. No mutation of any real library occurs in this release (D-09, read-only).
Last updated: 2026-07-03.

## Completion Status

| Phase | Goal | Fulfills AC | Owner | Status |
|---|---|---|---|---|
| P1 | Fixture harness (built first) | AC-F1..AC-F5 | Sonnet (Opus reviews determinism) | Not started |
| P2 | F-101 scanner hardening + F-104 job model | AC-101.1..101.6, AC-104.1..104.4 | Opus | Not started |
| P3 | F-102 WizTree CSV import | AC-102.1..102.3 | Sonnet | Not started |
| P4 | Parse stack (F-301, F-302, F-303, F-304) | AC-301.*, AC-302.*, AC-303.*, AC-304.* | Opus (F-303 merge) + Sonnet (tables) | Not started |
| P5 | Classification engine (F-201, F-202, F-203) | AC-201.*, AC-202.*, AC-203.* | Opus | Not started |
| P6 | F-1001 activity log | AC-1001.1, AC-1001.2 | Sonnet | Not started |
| P7 | Real-library read-only gate + FD-14 probe + baselines | G-03, G-05, G-06, G-07, G-08, G-09 | Fable (verification) + Opus (probe) | Not started |

## Phase 1: Fixture harness (built first)

**Goal:** materialize a deterministic synthetic library from a declarative manifest so every downstream golden test has a stable bedrock. **Addresses:** AC-F1, AC-F2, AC-F3, AC-F4, AC-F5.

**Steps:**
1. Create `crates/abo-core/fixtures/` (or a `fixtures/` workspace crate per the breakdown Section 4 layout) with `manifest.rs` defining a `FixtureManifest` of folder/file specs (name, class-intent, size, optional attributes).
2. Author the manifest entries: one example per each of the 9 F-301 patterns; deep nesting (depth >= 5, Hugo-style); mixed; multi-book (Narnia 7-file, Harry Potter 11-file/7-title); nonconforming disc; parallel-format `0 M4B`; NFC/NFD Unicode pairs; reserved-name near-misses; zero-byte samples; exact-duplicate pairs. Per FD-17 add a video/course cluster (52 Sales Lessons mp4, cbr/cbz, radio play); per FD-01 add a pack shell carrying source-pack membership on member books.
3. Implement `generate(manifest, temp_root) -> GeneratedLibrary` writing placeholder files at declared sizes; refuse to write outside `temp_root`; generate near-limit-path and reserved-name fixtures only where the host OS permits, skip-with-note otherwise.
4. Wire a `tempfile`-based test harness fixture so `cargo test` builds and tears down the tree; ensure `.gitignore`/test-dir choice keeps generated trees out of the working tree (test-strategy.md Section 2).

**Verification:** a test regenerates from the same manifest twice and recursive-compares byte-identical (AC-F4); a test asserts summed on-disk sizes equal manifest-declared totals (AC-F5); CI `test` job on ubuntu + windows is green with nothing committed under the repo tree (AC-F1).

**Decision Gate:** Fable confirms the manifest covers every family the later golden tests need before P4/P5 depend on it. If a family is missing, add it here, not ad hoc later.

**Output Artifacts:** `crates/abo-core/fixtures/manifest.rs`, `generate.rs`, fixture-harness test module.

**Suggested Owner:** Sonnet (plumbing); Opus reviews the determinism guarantee.

## Phase 2: F-101 scanner hardening + F-104 job model

**Goal:** harden the v0.1.0 walker against the FD-19 edge list and wrap all long operations in the cancellable, progress-emitting job model. **Addresses:** AC-101.1..101.6, AC-104.1..104.4.

**Steps:**
1. In `crates/abo-core/src/scan/`, extend the walker to use extended-length (`\\?\`) path semantics on Windows; record (do not follow) reparse points/junctions with a `junction-skipped(path)` note; record permission-denied entries and continue; preserve case; apply ruleset excludes/globs.
2. Add `LongPathsEnabled` detection on Windows; emit a warning record with a how-to link when a target exceeds 260 chars and long paths are disabled; keep near-260 interop warnings.
3. In `crates/abo-core/src/` job module, implement the job model on the Tokio runtime: `job:progress` events (items done, total estimate, current path), a cancellation token checked at entry boundaries, and a persisted `jobs` row (from the v0.1.0 schema) visible after restart.
4. Map scanner failures to `AppError` (`root-not-found`, `root-not-directory`, `permission-denied`, `junction-skipped`, `csv-parse` shared with P3) in `error.rs`.

**Verification (tests first):** junction-loop termination test asserts the scan ends and records the junction (AC-101.2); permission-denied-continues test (skip-with-note where OS disallows seeding) (AC-101.3); fixture-count golden (AC-101.1); cancel-at-boundary test asserts scan halted before completion and `jobs` row = `cancelled` (AC-104.2); monotonic-progress test (AC-104.1); killed-scan `jobs`-row-visible test (AC-104.4).

**Decision Gate:** confirm cancel semantics (partial vs discarded snapshot) are documented and tested before classification consumes snapshots. N/A to later apply-side reconciliation (v0.6.0).

**Output Artifacts:** `src/scan/walker.rs` (hardened), job module, `error.rs` additions, scanner + job test suites.

**Suggested Owner:** Opus (safety-adjacent, FD-19 correctness).

## Phase 3: F-102 WizTree CSV import

**Goal:** parse the WizTree export into the same `entries` schema as a live scan so downstream code runs identically. **Addresses:** AC-102.1, AC-102.2, AC-102.3.

**Steps:**
1. In `src/scan/csv_import.rs`, parse WizTree columns (name, size, allocated, modified, attributes); reconstruct parent linkage from the path column; write `entries` rows flagged `source = csv`.
2. Map malformed rows to `csv-parse(row)` with the row index; recover-and-continue where safe.
3. Add a test importing `_local/prior-work/WizTree_2026-03-25.csv` producing a valid snapshot; add a parity test running classify+parse on a CSV snapshot and an equivalent live-scan fixture and comparing classifications.

**Verification:** import-real-CSV test yields a valid `source = csv` snapshot with parent linkage (AC-102.1); parity test passes (AC-102.2); one-bad-row fixture raises `csv-parse(row)` (AC-102.3).

**Decision Gate:** N/A.

**Output Artifacts:** `src/scan/csv_import.rs`, CSV import test suite.

**Suggested Owner:** Sonnet.

## Phase 4: Parse stack (F-301, F-302, F-303, F-304)

**Goal:** convert names into structured fields with explicit confidence, and produce filesystem-safe components. **Addresses:** AC-301.1..301.4, AC-302.1..302.4, AC-303.1..303.3, AC-304.1..304.5.

**Steps (test-first per pass):**
1. `src/parse/matchers/` - implement the 9 pattern matchers as pure functions returning fields + match score, run in specificity order, ties returning `ambiguous`. Write the table-driven tests (real discovery examples) before each matcher (AC-301.1, 301.2). Add the 237/238 loose-root fixture assertion (AC-301.3).
2. `src/parse/strip.rs` - implement composable toggleable strippers (bracket tags, bitrate, size, rank prefix, year prefix, release-group suffix, underscores) from the discovery regexes; extract year as a field before stripping; guard against empty results. Write the proptest idempotence property `strip(strip(x)) == strip(x)` before/with the passes (AC-302.2), plus table tests (AC-302.1, 302.3, 302.4).
3. `src/parse/extract.rs` - merge matcher output up the tree, emit per-field confidence (high/medium/low); explicit-over-inherited precedence; conflict flagging; fabricate nothing (AC-303.1, 303.2). Wire the folder-first default and expose a confidence-weight hook tuned by the FD-14 probe (AC-303.3, consumed in P7).
4. `src/parse/normalize.rs` - illegal-char strip, reserved-name guard, trailing dot/space removal, length cap (default 120) with grapheme-safe word-boundary truncation, NFC normalization; never emit empty (AC-304.1..304.5). Table-driven tests including NFC/NFD equality.
5. Add a parser-coverage metric that reports the fraction of fixture names parsing cleanly (feeds the P7 descope decision, AC-301.4).

**Verification:** matcher/stripper/normalizer table tests green; proptest idempotence green; confidence golden matches expectations; coverage metric emitted.

**Decision Gate:** if the coverage metric is below ~90%, freeze the pattern set and route the remainder to `manual-review` (record the decision). Fable ratifies the freeze at P7. (AC-301.4, descope trigger.)

**Output Artifacts:** `src/parse/{matchers/,strip.rs,extract.rs,normalize.rs}`, parser test suites, coverage reporter.

**Suggested Owner:** Sonnet for the table-driven matchers/strippers/normalizer; Opus for the F-303 confidence-merge logic (correctness-sensitive).

## Phase 5: Classification engine (F-201, F-202, F-203)

**Goal:** assign a deterministic `FolderClass` to every folder bottom-up, compute health metrics, and detect multi-book folders. **Addresses:** AC-201.1..201.6, AC-202.1..202.3, AC-203.1..203.3.

**Steps:**
1. `src/classify/engine.rs` - deterministic bottom-up rules producing one of `book`, `series-container`, `pack-container`, `staging`, `mixed`, `multi-book-suspect`, `empty`, `docs-resources`, `manual-review`; record rule id + evidence JSON per folder (AC-201.1, 201.2, 201.5). Route FD-17 video/course clusters and radio plays to `manual-review` (AC-201.3). Distinguish `empty` from `docs-resources` (AC-201.6). Unclassifiable = `manual-review` (AC-201.4).
2. `src/classify/multibook.rs` - F-203 heuristics: N sibling audio files with distinct parsed titles (Narnia), numbered same-series files (Wings of Fire), the Harry Potter 11-file/7-title hard case; do not flag disc-split single books or single-book-plus-bonus (AC-203.1..203.3). Consumes P4 parse output.
3. `src/classify/metrics.rs` - F-202 aggregate counts and byte totals per class and per problem type; every metric declares its unit (FD-08); already-tidy yields zeros (AC-202.1..202.3).
4. Write insta golden snapshots over the fixture library for classification, metrics, and multi-book flags (AC-201.1, AC-202.1).

**Verification:** insta golden snapshots match; determinism test (same snapshot = byte-identical classification, AC-201.5); FD-17 manual-review test (AC-201.3, gate G-06); multi-book false-positive test (AC-203.3); byte-total test ties to fixture-declared sizes (AC-202.1 with AC-F5).

**Decision Gate:** Fable confirms classification golden expectations are correct (not just green) before the P7 real-library run compares against baselines.

**Output Artifacts:** `src/classify/{engine.rs,multibook.rs,metrics.rs}`, insta snapshot files, classification test suites.

**Suggested Owner:** Opus (L-complexity, correctness is the product).

## Phase 6: F-1001 activity log

**Goal:** append-only record of every scan, import, classify, and parse run with parameters and outcome. **Addresses:** AC-1001.1, AC-1001.2.

**Steps:**
1. In `src/db/`, add `activity_records` writes (schema from v0.1.0 / this release's migration): action name, params JSON, outcome, timestamps.
2. Hook scan, CSV import, classify, and parse entry points to append one record each; record failures with error code, not only successes.
3. Test asserts one row per run, failure rows carry error codes, and prior rows are never mutated (append-only, AC-1001.2).

**Verification:** activity-log test suite green (AC-1001.1, AC-1001.2).

**Decision Gate:** N/A.

**Output Artifacts:** `src/db/activity.rs`, activity-log test.

**Suggested Owner:** Sonnet.

## Phase 7: Real-library read-only gate + FD-14 probe + baselines

**Goal:** run the hardened engine read-only against the real library and the WizTree CSV, verify against FD-18 baselines, run the FD-14 tag-quality probe, and settle the parser-coverage freeze decision. **Addresses:** G-03, G-05, G-06, G-07, G-08, G-09 (release gate).

**Steps:**
1. Run a read-only live scan of `E:\Books - Audio` and time it (< 60 s target, NFR Scale); import `_local/prior-work/WizTree_2026-03-25.csv` (G-07); compute health metrics on both.
2. Compare metrics to the FD-18 2026-03-25 baselines: ~582 book-like, ~11 mixed, ~831 items; noise counts 203 bracket / 170 bitrate / 214 size / 143 rank / 116 year; 237/238 loose-root parses. Record the delta; label every figure "2026-03-25 baseline, pending fresh scan" (G-03; per OQ-2 do not fail on drift alone, report it).
3. Implement and run the FD-14 tag-quality probe: a bounded lofty subset reads embedded tags on a few hundred real files, read-only; emit a field-completeness report; write a verdict on whether the folder-first assumption holds and set the F-303 confidence weight accordingly (G-08). This is the only place lofty is used this release, and it never writes.
4. Read the parser-coverage metric from P4; if below ~90%, freeze the pattern set and route the remainder to `manual-review`, recording the decision (G-05).
5. Confirm the full CI matrix is green: lint (core-purity, bindings-drift), test (ubuntu + windows), build (windows GA + macos honesty), per docs/internal/ci-plan.md and FD-24 (G-09).

**Verification:** scan-time evidence recorded; baseline-delta report written to docs/internal (or the release folder) labeled per FD-18; FD-14 field-completeness report + verdict recorded; coverage-freeze decision recorded if triggered; CI matrix green screenshot/log linked.

**Decision Gate:** Fable owns final gate verification G-01..G-09; jp reviews OQ-1 (preferSource reopen?) and OQ-2 (drift tolerance) at this gate. No tag is cut until all gate items are green (tagging is human-only per D-10).

**Output Artifacts:** baseline-delta report, FD-14 probe report + verdict, coverage-freeze decision note, CI evidence links.

**Suggested Owner:** Opus for the probe implementation; Fable for gate verification and synthesis.

## Test-First Posture (summary)

Per docs/internal/test-strategy.md Section 6.4 layers, tests precede or accompany implementation:
- P1 writes the fixture determinism + size tests before generation is trusted.
- P4 writes the matcher/stripper table tests and the proptest idempotence property before the passes pass them.
- P5 writes the insta classification/metrics goldens before/with the engine.
- P2 writes junction-loop, permission-continue, and cancel tests before the hardening lands.
- P7 is verification-only against real data (read-only) plus CI matrix confirmation.

## Branch / PR Plan

Short-lived feature branches off `main`, one per phase (or per feature cluster within P4/P5), PRs into `main`, agent self-merges green PRs while the repo is private (D-11, EXECUTION.md). Required green checks per PR: lint (fmt, clippy -D warnings, core-purity, bindings-drift), test (ubuntu + windows), build (windows + macos). Branch naming `feat/v0.2.0-<phase>` (e.g. `feat/v0.2.0-fixtures`, `feat/v0.2.0-scanner`, `feat/v0.2.0-parse`, `feat/v0.2.0-classify`). Live CI workflow files already exist from v0.1.0 (FD-24); this release adds no workflow changes.

## Risks and Descope Triggers

| Risk / trigger | Pre-agreed action |
|---|---|
| Parser fixture coverage < ~90% (P4/P7) | Freeze the pattern set; remainder routes to `manual-review` by design (not a failure). Record the freeze. [release plan Section 5] |
| macOS build red in CI > 1 week | Downgrade macOS build to allow-fail + tracking issue; never block this Windows release on it. [release plan Section 5; FD-24] |
| Real-library scan exceeds 60 s | Profile the walker; the target is a soft gate - record the number, investigate before treating as a defect. [NFR Scale; OQ-2] |
| FD-14 probe finds tags materially more complete than folder names | Do not change the v1 folder-first default here; record the finding, raise OQ-1 to jp for a v1.1 F-1101 decision. [FD-14] |
| Fresh-scan metrics diverge sharply from 2026-03-25 baselines | Report the delta as the deliverable (library has changed); do not fail G-03 on expected drift alone. [FD-18; OQ-2] |
| Classification golden churn (fixtures shift) | Regenerate goldens deliberately with Fable review; never auto-bless a snapshot diff without confirming the class change is intended. |

## Definition of Done

The spec's release gate, restated as the exit checklist: G-01 (fixtures classify/parse to goldens), G-02 (stripper idempotence proven), G-03 (real read-only scan < 60 s, metrics vs FD-18 baselines recorded and labeled), G-04 (junction/permission/cancel tests including junction-loop termination), G-05 (parser-coverage measured, freeze applied if < ~90%), G-06 (FD-17 video/course to manual-review), G-07 (WizTree CSV import feeds the baseline compare), G-08 (FD-14 tag-quality probe run with recorded verdict), G-09 (full CI matrix green). All completion-status rows `Done`; every spec AC has at least one `Done` phase addressing it; tag cut is human-only (D-10) and out of this plan's scope.
