---
id: v0.3.0
title: "Implementation Plan - Release v0.3.0 (planning)"
type: implementation-plan
codename: planning
date: 2026-07-03
status: review
owner: jprisant
produced-by: author agent (release implementation plan)
linked-spec: docs/internal/releases/v0.3.0-planning/spec.md
depends_on: v0.2.0-understanding
sources:
  - docs/internal/releases/v0.3.0-planning/spec.md
  - _local/planning/release-plan-and-ci_2026-07-02.md (Sections 4, 6)
  - _local/planning/feature-function-breakdown_2026-07-02.md (E-04, IPC, schema)
  - docs/internal/decision-ledger.md (D/FD ledger)
executor-model-guidance: >
  FD-30 tiering. Opus-tier (correctness-critical, complex): F-403 plan builder,
  F-404 validation, F-507 provenance capture, determinism golden. Sonnet-tier
  (mechanical): templates table, exporters, report template scaffolding, ruleset
  CRUD, reports-folder plumbing, table-driven tests. Fable reviews every release
  gate item and the determinism + hostile-fixture gates before tag.
ac-coverage: complete
phase-count: 8
---

# Implementation Plan: Release v0.3.0 (planning)

## Task Summary

- Status: complete (all 8 phases done; gate walked 2026-07-05; tag awaiting jp per D-10). Plan decomposes v0.3.0 spec (AC-1..AC-33) into 8 phases.
- Test-first: each phase names the tests it adds (per docs/internal/test-strategy.md layers) before the implementation tasks that make them pass.
- Signature deliverables: deterministic validated plan over the real snapshot; self-contained HTML dry-run report a non-engineer can read.
- Model tiering per FD-30 in the frontmatter and per task below.

## Completion Status

| Phase | Goal | Fulfills AC | Owner | Status |
|---|---|---|---|---|
| P1 | Data model + migration (plans, plan_ops, rulesets, duplicate_*, reports) | AC-16, AC-29, AC-31 | LLM (Sonnet) | Done |
| P2 | Naming templates + presets | AC-1, AC-2, AC-3 | LLM (Sonnet) | Done |
| P3 | Structure policies + ruleset model | AC-4, AC-5, AC-6, AC-7, AC-29, AC-30 | LLM (Opus) | Done |
| P4 | Plan builder + campaign groups + determinism | AC-8, AC-9, AC-10, AC-11 | LLM (Opus) | Done |
| P5 | Plan validation + hostile fixture | AC-12, AC-13, AC-14, AC-15, AC-17 | LLM (Opus) | Done |
| P6 | Provenance capture, disc + parallel-format, duplicates | AC-7, AC-24, AC-27, AC-28, AC-32, AC-33 | LLM (Opus) | Done |
| P7 | Exports (CSV/JSON/Markdown) + reports folder | AC-18, AC-19, AC-31 | LLM (Sonnet) | Done |
| P8 | Dry-run HTML report + provenance section + gates | AC-20..AC-26 | LLM (Opus) + Fable review | Done |

## Phase 1: Data model and migration

**Goal:** land the SQLite tables plan/validation/dedupe/reports need, additive over the v0.2.0 schema. **Addresses:** AC-16, AC-29, AC-31.

Steps:
- Add a numbered migration under `crates/abo-core/migrations/` creating `plans`, `plan_ops`, `rulesets`, `duplicate_groups`, `duplicate_members` per breakdown Section 7. `plan_ops` includes a provenance column (pack membership + award/rank) for F-507; `plans` includes `stats_json` and `status`.
- Add the snapshot-retention field to `settings` (default keep last 10 scans, FD-20); no UI yet.
- Add sqlx query modules under `crates/abo-core/src/db/` for plan and ruleset CRUD.

Verification: migration applies from empty and from a v0.2.0 DB; a db-layer test inserts a plan + ops and reads them back equal. Test-strategy layer: IPC/storage.

Decision Gate: freeze `plan_ops` column set here (F-405 immutability depends on it). N/A otherwise.

Output Artifacts: new migration file; `db/plans.rs`, `db/rulesets.rs`.

Suggested Owner: LLM (Sonnet).

## Phase 2: Naming templates and presets

**Goal:** render target paths from template variables and the three presets, default `abs-author-first`. **Addresses:** AC-1, AC-2, AC-3.

Steps:
- Add `crates/abo-core/src/plan/templates.rs`: variable set (`{Author}`, `{Title}`, `{Series}`, `{SeriesIndex}` width-configurable, `{Year}`, `{Narrator}`, `{Subtitle}`), the three presets, and explicit missing-field fallbacks (omit segment, never emit `()` or empty folders).
- Encode D-02: no genre/award folder segment under `abs-author-first`.

Verification (test-first): write a table-driven test over the discovery example set asserting each preset's target shape, each missing-variable fallback, and no award folder under default. Test-strategy layer: Parsers/templates.

Decision Gate: N/A.

Output Artifacts: `plan/templates.rs`; template test module.

Suggested Owner: LLM (Sonnet).

## Phase 3: Structure policies and ruleset model

**Goal:** the F-402 policy set with safe defaults, persisted through the F-801 ruleset. **Addresses:** AC-4, AC-5, AC-6, AC-7, AC-29, AC-30.

Steps:
- Add `crates/abo-core/src/ruleset.rs`: schema-versioned JSON body bundling templates + structure policies + cleanup toggles; CRUD wired to `db/rulesets.rs`; reject bodies failing schema validation with a machine code.
- Implement F-402 policy fields: one-book-per-folder, pack-shell destination (default quarantine, `leave-in-place` toggle, FD-01), sidecar keep-with-book, non-audio clutter defaults (quarantine nfo/sfv/playlist/weblink; keep ebook/cover), preferred-format m4b, empty-folder removal.
- Route FD-17 video/course/radio-play classes to `manual-review` (no auto-plan).
- Ship the default ruleset carrying `abs-author-first` + pack-shell-to-quarantine.

Verification (test-first): tests for pack-shell default vs toggle, partial-pack shell stays in place (AC-5), clutter defaults, and `manual-review` routing; ruleset save-reject test; default-ruleset value assertions. Test-strategy layer: Plan builder + storage.

Decision Gate: confirm pack-shell-to-quarantine default (FD-01) is the shipped value. N/A otherwise.

Output Artifacts: `ruleset.rs`; policy tests; default ruleset seed.

Suggested Owner: LLM (Opus) - policy correctness is safety-adjacent.

## Phase 4: Plan builder and campaign groups

**Goal:** generate the ordered, immutable, deterministic operation list grouped by campaign group. **Addresses:** AC-8, AC-9, AC-10, AC-11.

Steps:
- Add `crates/abo-core/src/plan/builder.rs`: consume snapshot + classifications + ruleset, emit `plan_ops` with source, target, kind, group, rationale (sentence + rule id), confidence, byte size.
- Implement the eight internal passes (`staging-separation`, `loose-root-books`, `strip-noise`, `split-multi-book`, `flatten-packs`, `normalize-series`, `dedupe-quarantine`, `empty-cleanup`) and the FD-26 fold to seven user-facing groups (labels shared with the report).
- Enforce dependency ordering (mkdir before move-into; moves out before rmdir); emit `no-op(manual-review)` where a required template field is missing.
- Persist via Phase 1 CRUD so regeneration creates a new plan (F-405 immutability).

Verification (test-first): determinism golden test (byte-identical serialized plan across two runs, AC-8); op-schema completeness test (AC-9); dependency-ordering test on a nested fixture (AC-10); group-fold test asserting seven labels (AC-11). Test-strategy layer: Plan builder (golden + insta).

Decision Gate: freeze the plan serialization format used by the golden (changing it later invalidates goldens). 

Output Artifacts: `plan/builder.rs`; golden plan snapshot; builder tests.

Suggested Owner: LLM (Opus).

## Phase 5: Plan validation and hostile fixture

**Goal:** the F-404 backstop with the full Windows-reality hazard list, plus the approval state machine. **Addresses:** AC-12, AC-13, AC-14, AC-15, AC-17.

Steps:
- Add `crates/abo-core/src/plan/validate.rs`: per-op verdict `valid`/`warning(reason)`/`blocked(reason)` with the AppError machine codes from breakdown Section 8.
- Implement checks: in-plan collisions (case-insensitive NTFS), on-disk collisions, source-inside-target cycles, path length with `\\?\` extended-length block threshold plus near-260 interop warning (FD-19), illegal/reserved names (backstop), cross-volume `copy+verify+delete` sizing vs free space, snapshot staleness. Detect `LongPathsEnabled=0` and attach a how-to link reference; record junctions/reparse points and never follow.
- Implement approval state machine (approve/reject/exclude); a `blocked` op cannot be approved.
- Build the hostile fixture (extends the v0.2.0 generator): seeded planned collision, case-only collision, cycle, over-length path, reserved name, insufficient-space cross-volume op.

Verification (test-first): hostile-fixture suite asserts each hazard's verdict + code (AC-12); path-length dual-threshold test (AC-13); LongPathsEnabled warning + junction non-traversal test (AC-14); case-collision test (AC-15); approval state machine test incl. blocked-cannot-approve (AC-17). Test-strategy layer: Plan builder (hostile fixture).

Decision Gate: confirm the extended-length block threshold value and the near-260 warning threshold.

Output Artifacts: `plan/validate.rs`; hostile-fixture manifest; validation tests.

Suggested Owner: LLM (Opus).

## Phase 6: Provenance, disc, parallel-format, duplicates

**Goal:** capture pack provenance, detect disc and parallel-format cases, and group duplicate candidates by group. **Addresses:** AC-7, AC-24, AC-27, AC-28, AC-32, AC-33.

Steps:
- F-507 in `plan/builder.rs`: for every flatten-packs op, record source-pack membership + award/rank marker (the `^` caret) into the `plan_ops` provenance column; record all memberships for multi-pack books; record even members that validation blocks.
- F-204 `plan/disc.rs`: recognize `Disc NN`/`CD NN`/`Disk NN` as conformant; propose renames for nonconforming variants (feeds `normalize-series`).
- F-205: detect the `0 M4B` parallel-format case; produce a quarantine op for the non-preferred copy per F-402 (keep m4b).
- F-701 `crates/abo-core/src/dupes/detect.rs`: exact basename+size grouping into groups (canonical unit, FD-08); normalized-title version candidates labeled distinctly, never auto-resolved. No hashing (deferred to F-702).

Verification (test-first): provenance-capture test over a fixture pack incl. blocked member (AC-24); manual-review no-op test for FD-17 classes (AC-7); disc conformant-vs-nonconforming test (AC-32); parallel-format keep-m4b test (AC-33); duplicate three-copy-one-group test and version-candidate labeling test (AC-27, AC-28). Test-strategy layer: Plan builder + classification.

Decision Gate: N/A.

Output Artifacts: `plan/disc.rs`, `dupes/detect.rs`; provenance field usage in builder; tests.

Suggested Owner: LLM (Opus).

## Phase 7: Exports and reports folder

**Goal:** CSV/JSON/Markdown plan exports plus the provenance report, all landing in the reports folder. **Addresses:** AC-18, AC-19, AC-31.

Steps:
- Add `crates/abo-core/src/plan/export.rs`: CSV (one row per op, stable columns), JSON (round-trippable), Markdown (grouped by the seven campaign groups with per-group counts; sample-data labeling per FD-27 where illustrative).
- Add the provenance report exporter (F-507) writing beside the plan.
- Add `crates/abo-core/src/reports.rs`: stable reports root beside `%LOCALAPPDATA%\AudiobookOrganizer\`, deterministic file names; every export writes here plus a user-picked location.

Verification (test-first): CSV/JSON/Markdown round-trip and grouping tests (AC-18); empty-plan and fully-blocked-plan export tests (AC-19); reports-folder landing test for each artifact (AC-31). Test-strategy layer: Plan builder + storage.

Decision Gate: N/A.

Output Artifacts: `plan/export.rs`, `reports.rs`; export tests.

Suggested Owner: LLM (Sonnet).

## Phase 8: Dry-run HTML report and release gates

**Goal:** generate the self-contained HTML report to F-506-report-spec.md, wire the CI zero-network gate, and clear the release gate. **Addresses:** AC-20, AC-21, AC-22, AC-23, AC-24, AC-25, AC-26.

Steps:
- Add `crates/abo-core/src/plan/report.rs` and a baked template (no network assets). Sections per F-506-report-spec.md: masthead/dateline/lead, seven-group summary table, before/after example tables with struck-through noise, warnings-needing-a-decision callout, FD-10 "what will not happen" canon block verbatim, F-507 provenance section, and the COMPLETE change-list table (no row cap).
- Embed Literata as a subsetted data-URI woff2 with a system serif fallback stack (FD-11); remove any prototype Google Fonts `<link>`.
- Count duplicates as groups (FD-08); label illustrative figures as sample data (FD-27); ensure no copy promises ABS-side changes or tag writes (FD-12).
- Add a CI grep gate (extend ci.yml lint job) failing on any external host or remote `<link>`/`<script src>` in the template or app.

Verification (test-first): generated-report test asserts one change-list row per plan op and presence of every required section (AC-21); CI grep gate green over a generated report and the template (AC-20); FD-10 verbatim block and group-count checks (AC-23); provenance-section presence + no-ABS-promise check (AC-26); provenance completeness over blocked members (AC-25). Manual QA: non-engineer read test over the real snapshot, recorded per test-strategy conventions (AC-22). Fable reviews G-1..G-8 before tag.

Decision Gate: G-3 real-snapshot review, G-5 zero-network, G-6 non-engineer read, and G-7 provenance completeness are human-in-the-loop gate items; the release does not tag until all eight release-gate items (spec Release Gate) are green.

Output Artifacts: `plan/report.rs`; baked HTML template; embedded Literata subset; ci.yml grep-gate step; manual QA evidence note.

Suggested Owner: LLM (Opus) authors; Fable reviews the gate.

## Branch and PR plan

- One short-lived feature branch per phase (or per adjacent cluster: P1+P2 may share a branch; P4+P5 stay separate given their weight). Trunk-based, PRs into `main`, agent self-merges green PRs while the repo is private (D-11, EXECUTION.md).
- Required green checks before merge (release plan Section 6): lint (fmt, clippy -D warnings, core-purity, bindings-drift), test matrix (ubuntu + windows: `cargo test --workspace` incl. goldens, hostile fixture, exports), build (windows GA + macos honesty). The report zero-network grep gate is added to lint in P8.
- No workflow-file changes here beyond the P8 grep-gate step; live CI already landed in v0.1.0 (FD-24).

## Risks and descope triggers

- Determinism flakiness (nondeterministic map ordering in serialization) - mitigate with ordered collections and a byte-compare golden; if flaky, freeze until deflaked (release plan Section 5: executor-invariant-class rule applied to the determinism gate).
- Real-snapshot counts drift from the 2026-03-25 baseline - expected (FD-18); G-3 reviews sanity, not exact match. Fresh-scan numbers replace the labeled baseline.
- Report font-subset size bloats the single-file report - subset Literata to the glyphs used; if still large, narrow the character set (FD-11 requires embedded + zero network, not a size cap).
- Hostile-fixture path-length cases on the Windows runner - generate over-length paths at runtime into temp (never committed), per release plan Section 6 note.

## Definition of done

The spec's Release Gate, restated as the exit checklist:
- [ ] G-1 determinism golden green (AC-8).
- [ ] G-2 hostile-fixture suite green (AC-12..AC-15).
- [ ] G-3 real-snapshot Markdown plan reviewed; loose-root ~237, strip-noise ~203 (FD-18 labeled) (AC-18).
- [ ] G-4 approval state machine tested (AC-17).
- [ ] G-5 self-contained HTML report opens with no network; CI grep gate green (AC-20, AC-21).
- [ ] G-6 non-engineer read test recorded (AC-22).
- [ ] G-7 provenance report contains every flattened pack member (AC-25).
- [ ] G-8 duplicates counted as groups; GB figures state their quantity (AC-27, AC-28).
- [ ] All 33 AC have at least one Done phase row; full green CI matrix on `main`; Fable gate review complete.
