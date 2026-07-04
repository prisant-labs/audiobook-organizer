---
title: Audiobook Organizer - Test Strategy
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (test-strategy)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Sections 4-6)
  - _local/planning/feature-function-breakdown_2026-07-02.md (Sections 5, 8, 9)
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md
  - docs/internal/decision-ledger.md
  - docs/internal/planning-audit-2026-07-03.md
---

# Audiobook Organizer - Test Strategy

## 1. Purpose and scope

This document expands the release plan's Section 6.4 test-strategy summary into the full strategy: what is tested, at which layer, with which technique, when it is introduced on the release ladder, and what evidence each gate produces. Every per-release implementation plan references this document; it is the durable "how we prove it" contract that the release folders (FD-16 (effort = release): docs/internal/releases/<version>-<codename>/) consume rather than re-author.

Acceptance criteria themselves live in specs, per standing rule 4. This strategy names techniques and evidence artifacts; it does not author AC. Where it lists a check, that check is realized as concrete AC in the owning release spec.

Two decisions frame everything below. D-07 (engine-first order: abo-core hardens on fixtures before any GUI) means the highest-value tests are headless Rust tests that run for four releases before a real screen exists. D-09 (safety invariants: quarantine-only, journal-before-act, single-writer, Vfs seam, rollback-as-plan, never-overwrite) means a specific set of executor tests are not "coverage" but the product's reason to exist; those are called out as signature gates and governed by the flake policy in Section 9.

What this strategy deliberately does not do: it sets no numeric line-coverage target. Coverage percentage is a weak proxy here; the meaningful measures are fixture-case coverage of the parser (Section 9 descope signal), the hazard checklist of the hostile fixture (2.9), and the byte-identical proofs (2.2, 2.5). A green rollback round-trip on the full fixture plan is worth more than any coverage number, and a high coverage number with a flaky executor invariant is a release freeze regardless (Section 10).

## 2. The test pyramid for this product

The engine (abo-core, zero Tauri deps per the architecture mapping in the feature-function breakdown Section 4) carries the load. The pyramid is deliberately bottom-heavy: cheap deterministic Rust tests dominate, the GUI is a thin typed layer over a frozen contract (D-07), and end-to-end GUI automation is deliberately absent (Section 6).

### 2.1 Pure-function unit tests (parsers, strippers, normalizers)

Introduced v0.2.0 (understanding). The parse layer is pure functions by design (F-301 (pattern matcher set), F-302 (noise strippers), F-303 (field extraction with confidence), F-304 (name normalizer)), which makes it the easiest and most valuable layer to test exhaustively.

- Table-driven tests keyed off real examples lifted from the discovery docs and prior-work regex recipes: the 9 catalogued patterns, the noise families (203 bracket-tag folders, 170 bitrate, 214 size, 143 rank prefixes, 116 year prefixes; 2026-03-25 baseline, pending fresh scan per FD-18), the 237/238 loose-root parses, underscored names, and the irregular series containers. Each row is (input, expected fields + confidence, expected residual noise). Sample demo arithmetic from prototypes is never hardcoded into an expectation (FD-27 (demo numbers are sample data)); expectations derive from real discovery strings or from generated fixtures.
- Property tests (proptest) for the idempotence invariant on strippers: strip(strip(x)) == strip(x) for arbitrary generated names, and for every fixture name. F-302 states idempotence as a hard requirement; proptest is how it is enforced against inputs no human enumerated.
- Normalizer property checks: output contains no Windows-illegal character (`<>:"/\|?*`), no reserved device name (CON, PRN, AUX, NUL, COM1-9, LPT1-9), no trailing dot or space, and is NFC-normalized (F-304). These are correctness invariants, run as proptest predicates over generated components.

Evidence: named cargo test modules (for example parse::strip::proptest_idempotence, parse::patterns::table). Coverage of fixture cases is the descope signal per Section 9.

### 2.2 Golden / snapshot tests (classification and plan determinism)

Introduced v0.2.0 for classification, v0.3.0 (planning) for plans. Uses insta for reviewable snapshots.

- Classification goldens: run F-201 (folder classification engine) and F-202 (library health metrics) over the fixture library (Section 3) and snapshot the FolderClass per folder plus the evidence (rule id) that justified it. Because every classification records why (F-201), the snapshot captures reasoning, not just the label, so a rule regression is visible in the diff.
- Plan-determinism golden (the v0.3.0 signature gate): same snapshot + same ruleset must produce a byte-identical plan (NFR: Determinism). The plan is serialized deterministically and snapshotted; any nondeterminism (map iteration order, timestamps leaking into ordered fields) fails the diff. F-403 (plan builder) names this as a golden test.
- Fixture goldens obey FD-27: fixtures are synthetic, contents are placeholders, and no snapshot encodes a prototype demo number as if it were a real target. Byte-stability of goldens depends on the FD-25 .gitattributes `* text=auto eol=lf` rule so a Windows checkout does not rewrite line endings under insta.

Evidence: committed .snap files under review; insta accept is a deliberate, diffed action recorded in the PR.

### 2.3 MemFs executor suites (via the Vfs seam)

Introduced v0.5.0 (acting). F-607 (dry-run harness) lands first in that release precisely so executor logic grows against memory before it ever touches a disk. The executor (F-601) runs against a Vfs trait with RealFs and MemFs implementations (D-09 Vfs seam).

- All executor logic (dependency ordering: mkdir before move-into, moves-out before rmdir; TOCTOU re-checks; single-writer enforcement) is unit-tested against MemFs seeded from a snapshot. Dry-run is defined as executing the full plan against MemFs (F-607), so these suites also validate the dry-run product surface D-04 (dry-run-first milestone) depends on.
- Journal-shape equivalence: dry-run and Real must produce identical journals over identical inputs, modulo phase markers (release plan v0.5.0 gate). MemFs makes this assertable without disk.

Evidence: exec::memfs suite names; a journal-shape equivalence test that diffs the dry-run journal against the RealFs journal from 2.4.

### 2.4 RealFs temp-dir integration

Introduced v0.5.0. The same executor, exercised against RealFs in an OS temp directory, closes the gap MemFs cannot: real rename semantics, real cross-volume copy+verify+delete (D-08 (rename-first executor)), real long-path behavior. Temp-dir trees are built by the fixture harness at runtime, never committed.

### 2.5 Rollback round-trip signature gate

Introduced v0.5.0; runs in CI on every merge from v0.5.0 onward. This is the release's signature gate and the mechanical proof of the PRODUCT.md undo promise: apply the full fixture plan for real in a temp dir, roll back through the same validate/preview/apply pipeline (F-603 (rollback) is a plan, not a special code path, per D-09), then recursive-hash compare the tree against its pre-apply state and assert byte-identical. F-603 names this as the gate that must pass before any real-library use.

Evidence: exec::rollback::round_trip test; the recursive-hash manifest it emits is cited by the v0.5.0 release report (D-10 non-blocking release reports).

### 2.6 Kill-resume reconciliation

Introduced v0.6.0 (hardening). Covers both interruption windows F-606 (interruption safety + resume) must survive: process aborted between journal intent and act (intent-without-done), and between act and verify (done-without-verify). The test drives the executor to abort at each boundary, restarts, runs startup reconciliation, and asserts exactly one operation was in doubt and was reconciled correctly in both directions (resume forward, or abort-with-rollback). Recoverability NFR: kill during apply leaves at most one operation in doubt, auto-reconciled on restart.

Evidence: exec::resume::intent_without_done and exec::resume::done_without_verify.

### 2.7 Adversarial never-overwrite tests

Introduced v0.5.0. The never-overwrite invariant (D-09) is proven adversarially: a target that did not exist at validation time is made to appear mid-apply (injected through the Vfs seam). The executor must halt the affected operation with `target-appeared` (error taxonomy, feature-function breakdown Section 8), leave the journal consistent, and never clobber the pre-existing file. Paired with a `source-vanished` case.

Evidence: exec::adversarial::target_appeared_midapply.

### 2.8 Determinism controls

Determinism is a first-class NFR (same snapshot + ruleset = identical plan) and the plan-determinism golden (2.2) can only stay stable if the sources of nondeterminism are seamed out. The controls, all engine-side:

- No wall-clock or system time leaks into any ordered or serialized field of a plan or classification; timestamps that must exist (job start, journal `at`) live in fields excluded from the determinism snapshot.
- Deterministic iteration: any map or set feeding operation ordering is sorted to a stable key (path, then op kind) before serialization, so plan_ops ordering (mkdir before move-into, moves-out before rmdir per F-403) is reproducible.
- Fixture builds are seeded: the hash-of-title used for the F-907 (cover extraction and fallback tiles) fallback tile color and any generated sample content derive from a fixed seed so goldens do not churn.
- Byte-stability rests on FD-25 line-ending normalization; a CI check that the working tree is clean after test runs catches any test that accidentally rewrites a committed golden.

### 2.9 Hostile-fixture validation suite

Introduced v0.3.0. F-404 (plan validation) must catch every hazard before anything reaches the executor. A purpose-built hostile fixture seeds each hazard and the suite asserts the correct per-operation verdict (valid / warning(reason) / blocked(reason)):

- planned target collision (two ops producing one path, compared case-insensitively for NTFS per FD-19),
- source-inside-target cycle,
- over-length path beyond the `\\?\` extended-length allowance, plus a near-260 interop warning (FD-19),
- reserved device name and illegal component (backstop behind F-304),
- insufficient free space for a cross-volume copy op (critical given the 1.3 TB free constraint),
- case-insensitive collision that only conflicts under NTFS folding (FD-19).

Evidence: plan::validate::hostile suite, one test per hazard, each asserting the machine code from the Plan error family.

### 2.10 Storage and migration tests

Introduced v0.1.0 (spine). The SQLite layer (sqlx, WAL, numbered migrations, feature-function breakdown Section 7) carries the snapshots, plans, and journal, so its integrity is a safety concern, not just persistence plumbing.

- Migration-from-empty and migration-from-existing: the v0.1.0 gate requires that the schema migration applies cleanly against an empty database and against a database already holding a prior migration's state. Both paths are unit-tested against a temp DB file.
- Corrupt-DB startup recovery (Storage error family: `db-corrupt-recovered`): a deliberately corrupted DB file is placed at the expected location; the test asserts the app logs, moves it aside to corrupt-backups\, recreates a fresh DB, and surfaces the recovery notice (the FD-04 corrupt-DB recovery surface). This behavior is inherited verbatim from the reference architecture and must be proven, not assumed.
- Snapshot-retention policy (FD-20 (SQLite scale posture)): keep last N scans (default 10, a setting in F-803 (app settings)); a test seeds N+k scans and asserts the oldest are pruned to bound DB growth. The single-writer job lock (D-09) has a test that a second apply_start is refused with `job-already-running` while one holds the lock.

Evidence: db::migrate and db::recovery test names; the retention test asserting row counts after prune.

## 3. Fixture harness design (the v0.2.0 highest-leverage asset)

The fixture generator is built first in v0.2.0 because every layer above depends on it. It is a Rust bin or build helper (fixtures/ per the architecture mapping) that materializes a synthetic library from a declarative manifest. Placeholder file contents, real sizes (so byte-total metrics and free-space checks are meaningful). The manifest must cover:

- all 9 naming patterns with real example strings from discovery,
- deep pack nesting, Hugo-style, depth 5 or more,
- mixed folders (direct audio plus child folders),
- multi-book folders (Narnia-style: N sibling books in one folder; the Harry Potter 11-files-across-7-titles hard case),
- nonconforming disc folders (Disc / CD / Disk variants, including the Verbal Advantage case) for F-204,
- parallel-format `0 M4B` cases (chapter mp3 plus m4b sibling) for F-205,
- unicode NFC and NFD twins of the same title (proves NFC normalization and dedupe equality),
- near-limit path lengths, GENERATED AT RUNTIME and never committed, so a Windows checkout never breaks and no `core.longpaths` git config is needed (release plan Section 6.1),
- reserved-name near-misses (CON.mp3, folder named NUL),
- zero-byte samples,
- exact-duplicate pairs (basename + size) for F-701 (duplicate candidate detection),
- a video/course cluster per FD-17 (the Zig Ziglar 52 Sales Lessons mp4 case, cbr/cbz comics, a radio play) that must route to manual-review and never be auto-planned,
- pack containers carrying provenance expectations per FD-01 (F-507 (pack provenance capture and report)): each pack fixture declares its source-pack membership so the golden asserts provenance is recorded in plan_ops and re-emitted post-apply.

The manifest is the single source of truth for expected counts, so classification goldens (2.2) and health-metric assertions read their expectations from it rather than from hand-typed numbers.

Manifest shape (conceptual): a declarative tree where each node is either a folder (with a declared expected FolderClass and, for packs, an expected provenance tag) or a file (with class, size, and optional embedded-tag expectations for the lofty-subset tests). The generator walks the manifest, writes the tree into a caller-supplied root (temp dir for tests, a named output dir for local inspection), and returns an index mapping every generated path to its declared expectations. Because the index is data, a test can assert "every folder the manifest declared as multi-book was classified multi-book-suspect" without enumerating paths by hand, and a manifest edit that adds a case cannot silently escape its golden.

Runtime-only generation is a hard rule, not a convenience: near-limit and over-limit paths, reserved-name near-misses (NUL, CON.mp3), and unicode NFD twins can all break a naive Windows git checkout. Generating them into a temp dir at test time means the repository never contains a hostile path, so a clone always succeeds and no `core.longpaths` git configuration is required (release plan Section 6.1). The tradeoff is that the harness itself must be correct; it therefore has its own small self-test asserting that a round-trip (generate, scan, compare to the index) reproduces the declared counts before any downstream suite trusts it.

## 4. Baseline validation against the real library

Two real-library reads are scheduled, both read-only, both in v0.2.0.

### 4.1 Drift-tolerant baseline scan (FD-18)

The v0.2.0 gate runs a read-only scan of E:\Books - Audio and compares health metrics to the FD-18 (drift-tolerant 2026-03-25 baselines): approximately 582 book-like folders, approximately 11 mixed folders, approximately 831 estimated ABS items, alongside the noise counts (203 bracket tags, 170 bitrate, 214 size, 143 rank prefixes, 116 year prefixes) and the 237/238 loose-root parses. These are targets within tolerance, not exact assertions: the 2026-03-25 snapshot is stale by definition and the measured drift is itself the deliverable (first fresh look since the baseline). Any surface citing these numbers labels them "2026-03-25 baseline, pending fresh scan". The scan must complete under 60 s (Scale NFR) and the count discrepancy noted in planning audit stream 1 item 7 (docs/internal/planning-audit-2026-07-03.md; files vs folders; dupes GB unknown) stays "unknown until measured".

Evidence: a baseline-scan report file recorded via F-1001 (activity log) plus a plain exported file (F-1002 (reports folder) formalizes that location only in v0.3.0 (planning)), cited by the v0.2.0 release report, recording measured-vs-baseline deltas.

### 4.2 Tag-quality probe (FD-14)

The v0.2.0 gate also runs the FD-14 (tag-quality probe): a bounded, read-only pass with the lofty subset over a few hundred real files, reporting embedded-tag field completeness (title, author, series, index, year, narrator present-or-absent rates). Purpose: validate whether the folder-first assumption holds before the engine commits to it. Protocol: fixed sample size recorded in the report; read-only (no writes, consistent with FD-29 (frontend never touches the filesystem) and the v1 no-tag-writing non-goal); result recorded as a probe report file. The PRD states the folder-first default supersedes the discovery doc's preferSource=tags default, with confidence tied to this probe's outcome. The probe result is the evidence that either confirms the default or flags it for reconsideration; it does not change behavior automatically.

Evidence: tag-quality probe report file; its completeness numbers are cited in the v0.2.0 release report and the PRD supersession note.

## 5. Frontend testing

Introduced v0.4.0 (seeing), when the five real surfaces land over the frozen tauri-specta contract (D-07). The frontend is deliberately thin, so its tests are narrow and targeted, not broad.

- Vitest component tests for the approval-state logic (F-502 (campaign group review)): approve / reject / defer per group, per-operation exclude dropping to no-op(user-excluded), and the rule that blocked operations cannot be approved (only fixed upstream or excluded). This state machine is the load-bearing frontend logic and gets direct coverage.
- Vitest tests for the strings module: FD-23 (localization) centralizes all user-facing copy in one strings module. A test asserts no user-facing surface hardcodes copy outside it (so later localization is possible) and that the FD-10 (deletion guarantee canon) sentence appears verbatim where the guarantee is shown, and that the banned vocabulary (dashboard, ops, dedupe, manifest, quarantine-in-UI per standing rule 3) does not appear in user-facing strings.
- Error-state component tests (planning audit stream 2 item 1): when the FD-04 (error, empty, and loading states) surfaces are built in v0.4.0, each gets a component test. Every AppError family maps to a family-safe surface (blocked campaign group, scan/apply failure, snapshot-stale re-validation prompt, corrupt-DB recovery notice, permission-denied) and every empty/edge state (already-tidy library, empty root, all-groups-excluded with the primary action disabled and explained, no duplicates found) and loading state (building-the-plan, re-scan progress) renders its intended copy. These are authored as tests, not inherited as a happy-path gap.
- axe-core accessibility smoke inside Vitest on the primary surfaces (FD-21 (accessibility verification method)), CI from v0.4.0.
- Mechanical token-contrast script (FD-21): checks every color token pair in both themes (data-theme="day" and data-theme="evening" per FD-09) against WCAG AA 4.5:1, including the dedicated error/danger token pair FD-09 adds. CI from v0.4.0. This is how WCAG AA is verified, not merely promised; --ink-3 tertiary text that conveys information must pass or be darkened (FD-21).
- Typed-bindings-only lint: no raw `invoke` in the frontend; generated bindings only (release plan v0.4.0 gate; FD-29). Enforced as a lint, complementing the bindings-drift CI gate (Section 7).

Two frontend concerns are explicitly not covered by automated tests and move to the manual checklist (Section 6) instead. Virtualization responsiveness (F-501 (everything view) and the plan preview staying responsive over the full 718-folder library, a v0.4.0 gate and the Responsiveness NFR) is a perception property jp verifies by hand on the real WebView2 build; a synthetic render-timing test would prove little about the felt experience. Visual rendering fidelity across the two themes is likewise human-observed, since WebView2 is the only engine jp can see. The component tests cover logic and copy; the human covers feel and pixels.

Evidence: Vitest suite names; the token-contrast script's pass report; axe-core smoke output. The keyboard walkthrough and virtualization-responsiveness check are manual (Section 6).

## 6. End-to-end GUI automation: deliberately deferred

E2E GUI automation is deliberately not built, the same call as repo-sync (release plan Section 6.4). In its place, a per-release manual QA checklist is kept in-repo under docs/internal/qa/, executed on Windows by jp (the only human who can drive the real WebView2 build; the architecture doc names WebView2-only observability as the Windows-first reality).

Each release adds or updates a checklist file docs/internal/qa/<version>-<codename>.md. From v0.4.0 the skeleton lists the human-validated flows:

- the review loop: open library home (F-902, renamed per FD-07 (library home), never "dashboard"), read health facts, inspect a plan, drill into a group, approve, exclude one operation, export,
- first-run and library-root selection (FD-05 (first-run and library root selection)): pick a root via the dialog, default ruleset abs-author-first, default theme day,
- error-states sampling (FD-04): trigger and eyeball a representative subset of the error / empty / loading surfaces,
- keyboard walkthrough of the primary surfaces (FD-21 keyboard item),
- both themes rendered (Day and Evening, FD-09).

Evidence: the completed, dated checklist file per release, with jp's pass/fail per line, cited by that release's report.

## 7. IPC contract and CI-mechanical gates

Introduced v0.1.0 (spine). The seam is the contract (architecture doc): the frontend depends on the generated tauri-specta bindings, which is what makes the test story cheap.

- Bindings-drift gate: regenerate the tauri-specta output and fail if it differs from the committed bindings (`pnpm bindings:check` then `git diff --exit-code`). Per FD-24, this gate runs on the Windows runner if the specta export links Tauri (verify during v0.1.0; document both options).
- Core-purity gate: abo-core must never depend on tauri, even transitively (`cargo tree -p abo-core` greps for tauri). This is what keeps the engine headlessly testable (D-07).
- Zero-network gate (FD-11 (fonts bundled, no network)): CI greps the exported HTML report template and the app for external hosts (Google Fonts link, CDN, remote images) and fails on any. Proves the FD-11 promise that the report embeds a subsetted Literata as a data URI and the app self-hosts woff2, with zero network requests.
- CI shape (FD-24): concurrency block with cancel-in-progress; permissions contents: read; thin-LTO release profile for per-push CI and full-LTO dist profile for release artifacts; WebKitGTK smoke recorded as deliberate non-adoption. Live workflow files land in the v0.1.0 spine, not in the docs-only branch, so a docs-only push never creates a red CI (FD-24).

Evidence: the three gate job names in ci.yml (bindings drift, core purity, zero-network grep), each a required check.

## 8. Real-data confidence ladder

Nothing Real runs against the actual library from CI or from an agent; that is human-only (D-10, and the release plan's autonomy allowlist). Confidence is earned by escalating from synthetic to real in controlled steps:

1. Fixtures (Section 3): every automated gate, all releases.
2. Copy of the Top 100 Sci-Fi subset (the discovery docs' suggested guinea pig): manual rollback round-trip on a disposable copy, v0.5.0.
3. Copy of the gnarliest Hugo pack (deepest nesting, provenance, parallel formats): manual round-trip, v0.5.0.
4. M-1 (campaign) staged groups (release plan): one Real apply per group against the actual library, human-driven, verification report and ABS spot-check between each. Gated on FD-17 backup posture recorded (D-17 (pre-campaign backup is user-defined): the M-1 gate stays open until a backup choice is recorded; nothing Real runs without it).

Steps 2 and 3 are RealFs on real data but on copies, so an agent may run them; step 4 is the actual library and is human-only.

Each copy-based step (2 and 3) runs the same procedure: hash the copy tree recursively to capture the pre-state, generate and validate a plan over a fresh scan of the copy, apply for real, run post-apply verification (F-604 (post-apply verification): targets exist, sizes match, sources gone), roll back, and re-hash to confirm byte-identical return. The step passes only if the round-trip is clean and the verification delta matches the plan's intent. A failure here, on real data the automated fixtures did not model, is exactly the signal these steps exist to surface before M-1. The gnarliest Hugo pack (step 3) additionally checks that F-507 provenance survived the flatten and re-emitted in the post-apply report (FD-01).

M-1 (step 4) follows the release plan's staged-group protocol: backup decision recorded first (D-17), fresh scan (the baseline is stale by definition), then one Real apply per campaign group with a verification report and an ABS spot-check between each, in the FD-26 (seven campaign groups) order. The pre-campaign Windows checks from FD-19 (LongPathsEnabled, Defender / Controlled Folder Access interference) run as a checklist step before the first Real apply.

## 9. Evidence policy

Every gate names its artifact so the release reports (D-10) can cite proof rather than assert success. The artifact is one of: a named test (module::test path), a report file in the reports folder or docs/internal/qa/, or a screenshot attached to the manual checklist. Each release report links the artifacts for the gates that release introduced or re-verified. The recurring citable artifacts are: the plan-determinism .snap, the rollback round-trip hash manifest, the hostile-fixture verdict table, the baseline-scan and tag-quality reports, the token-contrast pass report, the zero-network grep result, and the dated manual QA checklist.

The rule is proof over prose: a release report line reads "rollback round-trip green: exec::rollback::round_trip, hash manifest reports/v0.5.0/roundtrip.json" rather than "rollback works." A gate with no citable artifact is not a gate. When a gate is re-verified in a later release (the rollback round-trip runs on every merge from v0.5.0), the later release report cites the same test path and its most recent run, so the chain of proof is continuous rather than one-time. This keeps the non-blocking release reports (D-10) honest without turning them into a second test suite.

## 10. Flake policy

Carried from the release plan's consolidated descope triggers (Section 5): if any executor-invariant test flakes, the release freezes until it is deflaked. This is the one place slippage is accepted rather than descoped. Executor-invariant tests are specifically: the rollback round-trip (2.5), kill-resume reconciliation (2.6), never-overwrite adversarial (2.7), and the dry-run/Real journal-shape equivalence (2.3). A flake in any of these is treated as a possible real intermittent safety defect, not as test noise, and blocks the tag. Flakes elsewhere (a snapshot ordering wobble, a frontend timing test) are fixed on the normal cadence and do not freeze the release, but are still logged.

Operationally, an executor-invariant flake opens a tracking issue immediately, the offending test is quarantined only by being investigated rather than skipped (skipping a safety invariant to unblock a release is never allowed), and the release report records the freeze and its resolution. The bar is deliberately asymmetric: a false freeze costs a little schedule, a shipped intermittent executor defect can cost real audiobooks, and the whole product exists to make that second cost impossible.

## 11. Introduction schedule (summary)

| Layer / gate | Technique | Introduced | Evidence artifact |
|---|---|---|---|
| Parsers, strippers, normalizers | Table-driven + proptest idempotence | v0.2.0 | named cargo test modules |
| Classification | insta goldens over fixtures | v0.2.0 | committed .snap |
| Plan determinism | byte-identical golden | v0.3.0 | plan .snap |
| Hostile-fixture validation | one test per seeded hazard | v0.3.0 | verdict table |
| Baseline scan + tag probe | read-only real-library reads | v0.2.0 | report files (FD-18, FD-14) |
| MemFs executor + dry-run | Vfs-seam unit suites | v0.5.0 | exec::memfs suite |
| RealFs temp-dir integration | real rename / cross-volume | v0.5.0 | exec temp-dir tests |
| Rollback round-trip (signature) | apply, roll back, hash-compare | v0.5.0 (CI every merge) | hash manifest |
| Never-overwrite adversarial | target appears mid-apply | v0.5.0 | exec::adversarial |
| Kill-resume reconciliation | both interruption windows | v0.6.0 | exec::resume |
| IPC contract | bindings-drift + core-purity + zero-network | v0.1.0 | CI gate jobs |
| Frontend approval + strings + errors | Vitest component tests | v0.4.0 | Vitest suites |
| Accessibility | axe-core smoke + token-contrast script | v0.4.0 | contrast pass report |
| Manual QA | in-repo checklist, Windows, jp | v0.4.0 | dated checklist file |
| Storage / migration | empty + existing DB, corrupt recovery, retention | v0.1.0 | db test names |
| Real-data confidence | fixtures to copies to M-1 | v0.5.0+ | per-step reports |

## 12. Test ownership and model tiering

Test authorship follows FD-30 (model-tiering execution policy), so the highest-risk proofs are written and adversarially checked by the strongest tier:

- Opus subagents own the safety-critical test code and its adversarial design: the rollback round-trip (2.5), kill-resume reconciliation (2.6), never-overwrite adversarial (2.7), and the hostile-fixture verdict logic (2.9). These are the tests whose absence or weakness would let a real defect reach the actual library, so they are not delegated to mechanical authorship.
- Sonnet subagents own the mechanical, table-driven bulk: the parser and stripper tables (2.1), the fixture manifest entries (Section 3), and the boilerplate around insta goldens (2.2). Volume work on a well-specified shape.
- Fable owns gate review and final verification: reading the evidence artifacts, confirming a release's gates are green with cited proof, and making the flake-freeze call (Section 10) before a tag.

This ownership split is recorded here so a release's implementation plan assigns test work to the right tier by default rather than by ad hoc choice.
