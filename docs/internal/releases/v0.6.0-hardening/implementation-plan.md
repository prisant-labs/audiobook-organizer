---
id: v0.6.0
title: "Implementation Plan - Release v0.6.0 (hardening)"
type: implementation-plan
date: 2026-07-03
status: review
owner: jprisant
tier: release-effort
scope: hardening
depends_on: v0.5.0-acting
produced-by: author agent (release implementation plan)
linked-spec: docs/internal/releases/v0.6.0-hardening/spec.md
phase-count: 8
ac-coverage: complete
sources:
  - docs/internal/releases/v0.6.0-hardening/spec.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - docs/internal/decision-ledger.md
executor-model-guidance: >
  Per FD-30 model-tiering: Opus-tier owns the safety-critical work (P1 startup
  reconciliation, cancellation coherence, hash gating, dedupe round-trip through
  journal/rollback). Sonnet-tier owns mechanical work (ruleset JSON serde, CSV
  export, table-driven policy tests, virtualized-list wiring, fixture generation).
  Fable reviews every gate boundary and the composite release gate before tag.
---

# Implementation Plan: Release v0.6.0 (hardening)

## Task Summary

- Status: IN PROGRESS. P1 (interruption safety + resume) is substantially landed on
  `feat/v0.6.0-p1-interruption-safety`; P1c (the resume-or-rollback surface) is parked
  awaiting UI direction. P2-P8 not started.
- Implements: `docs/internal/releases/v0.6.0-hardening/spec.md` (41 AC).
- Depends on: v0.5.0-acting (executor, journal, rollback, quarantine, dry-run harness, apply surface).
- Phase count: 9 (P0 added 2026-07-30, see below). AC coverage: complete.
- Last updated: 2026-07-30.

### Scope change 2026-07-30: History and undo pulled into this release

A deep external audit (Codex 5.6, `_local/audit/2026-07-30_audit_codex-56.md`) found that
v0.5.0's undo machinery was complete but UNREACHABLE: the History route was a placeholder
and no surface called either rollback-preparation command. Recovering an interrupted
journal correctly and then giving the user nowhere to act on it is not a finished safety
story, so History and undo ship in this milestone rather than a later one. Tracked as P0
below because it is a prerequisite for exposing real changes at all, not an extra feature.

The same audit found two defects in the P1 work as first landed; both are fixed and
recorded in the P1 status note.

## Completion Status

| Phase | Goal | Fulfills AC | Owner | Status |
|---|---|---|---|---|
| P0 | History + undo reachable (read model, screen, rollback wiring) | (scope change, see above) | LLM (Opus) | Landed, unmerged |
| P1 | Interruption safety + resume (reconciler, cancellation, access-denied) | AC-1..AC-9 | LLM (Opus) | P1a/P1b/P1d landed; P1c parked |
| P2 | Hash verification (BLAKE3, candidates-only, gating) | AC-10..AC-16 | LLM (Opus) | Not started |
| P3 | Resolution policies + dedupe as a campaign group | AC-23..AC-27 | LLM (Opus) | Not started |
| P4 | Duplicate review + report (data + CSV, group canon) | AC-17..AC-22 | LLM (Sonnet) | Not started |
| P5 | Duplicates surface (F-905) | AC-28..AC-31 | LLM (Sonnet) | Not started |
| P6 | Ruleset import/export (F-802) | AC-32..AC-35 | LLM (Sonnet) | Not started |
| P7 | Everything view (F-501 redefined) | AC-36..AC-39 | LLM (Sonnet) | Not started |
| P8 | Long-path battle testing + release gate | AC-40, AC-41 | LLM (Opus) + Fable | Not started |

**P1 status detail (2026-07-30).** P1a (reconcile primitives), P1b-1 (per-kind outcome
classification), P1b-2 (orchestration + journal repair), and P1b-3 (startup hook + IPC)
are landed and green. P1d verified AC-8 and AC-9 were already satisfied by v0.5.0 work.
P1c (the resume-or-rollback surface) is deliberately parked: mockups exist at
`_local/gui/2026-07-22/resume-rollback.html` and the maintainer wants to direct that
design before it is built. Two audit-found defects were fixed on top:

1. The new `reconcile-failed` error had no family-safe copy, which left the branch red on
   `pnpm typecheck` and the error-copy exhaustiveness test.
2. **Startup reconciliation was mode-blind.** It queried every `running` apply job without
   reading `jobs.mode` while the shell always supplied `RealFs`. Because the frontend pins
   dry-run, every stranded job in practice was a rehearsal, so a kill during a practice run
   would probe the real library to classify an operation that had only touched memory.
   Reconciliation is now gated on `jobs.mode`, fails closed on an unreadable mode (the
   column is nullable), and fails closed rather than sweeping multiple stranded jobs.

Still outstanding for P1: the true kill-process integration tests the spec calls for
(current coverage simulates the in-doubt state rather than killing a process), and the
AC-8 hand walkthrough.

Phases P1-P3 are strictly ordered (safety foundation first). P4-P7 can proceed in parallel once P2/P3 land the dedupe data model. P8 is the closing gate.

## Phase 0: History + undo reachable

**Goal:** make the undo machinery v0.5.0 built actually usable by a person. **Addresses:**
the scope change recorded above; no new AC (the underlying guarantees are v0.5.0's AC-11,
AC-14, AC-16, which were implemented but unreachable).

Steps:
1. Add a History read model in `crates/abo-core/src/exec/history.rs`: list past apply jobs
   newest-first, each with its undo offer already RESOLVED by the engine.
2. Resolve the offer in the engine, not the shell. Which undo path applies depends on
   engine invariants (was a manifest exported, are its ops reversible, did anything land,
   did reconciliation leave an op ambiguous). Deriving that in TypeScript would put a
   safety decision in the layer with the least context.
3. Order the checks by safety, not convenience: an unreadable mode and an ambiguous
   reconciliation both resolve to "needs a look" BEFORE any offer is considered, and a
   rehearsal is excluded before a manifest is looked for, so neither can fall through into
   an offer to move real files.
4. List practice runs and label them; never offer them an undo. Hiding them would make the
   record lie by omission.
5. Add the `history_list` command with a clamped limit.
6. Replace the `ComingSoon` History route with the real screen. Undo is a PLAN, not a
   button: each action prepares an inverse plan and hands it to the same review surface a
   forward tidy-up uses (D-09). Nothing moves on the strength of a click.
7. Add `AppError::HistoryUnavailable` with family-safe copy stating that books and undo
   files are untouched (an undo file is self-contained per AC-11, so it survives this read
   failing).

Verification:
- Engine tests for every offer arm, including a rehearsal with completed journal rows
  (must still be "practice run") and an ambiguous reconciliation (must be "needs a look"
  even though the run has ops that would otherwise qualify).
- A test that an ordinary walk-time failure is NOT mistaken for an unresolved ambiguity.
- Frontend tests that a practice run offers no undo control at all, that an ambiguous run
  offers no one-click reversal, and that the partial path forwards exactly the op ids the
  engine supplied.

**Status: landed** on `feat/v0.6.0-p1-interruption-safety` (10 engine tests, 9 screen
tests). Deliberately NOT in scope: per-operation drill-down, verification-discrepancy
display, and a contiguous-tail picker. The partial undo offers the whole recorded tail as
one action; `rollback_prepare_partial` re-checks contiguity itself and refuses a gap.

## Phase 1: Interruption safety + resume

**Goal:** startup reconciliation of the single in-doubt journal entry, coherent cancellation, and access-denied retry-once-then-halt-group. **Addresses:** AC-1, AC-2, AC-3, AC-4, AC-5, AC-6, AC-7, AC-8, AC-9.

Steps:
1. Add a reconciler entry point in `crates/abo-core/src/exec/` (e.g. `reconcile.rs`): on core init, query `journal` for the max-seq `intent` row lacking a matching `done`/`failed`; assert at most one (single-writer + flush invariant from v0.5.0 F-602).
2. Implement outcome verification: for `kind = rename`, check source/target existence on the `Vfs` seam; for `kind = copy+verify+delete`, use a target-size check to determine the phase reached. Write the correct terminal row via the existing journal append path.
3. Add a resume-or-rollback decision result to the reconciler output and wire it to the IPC surface consumed by the FD-04 (F-908 states) resume component (frontend rendering was authored in v0.4.0; here we supply the data and the resume/rollback commands).
4. Route resume through the existing job runner from the reconciled seq; route rollback through `rollback_prepare` (F-603) so it re-validates and previews.
5. Harden cancellation in the executor loop: honor the cancel token only at operation boundaries (F-104 semantics); on cancel, ensure the last journal state is terminal and the job is marked resumable.
6. Add access-denied handling in the executor: on the OS access-denied code, retry once; on second failure, emit `AppError` (FD-19 remediation) and halt the current campaign group, leaving the journal coherent.

Verification:
- Kill-injection tests (test-strategy Executor layer): a test harness that panics/aborts the process (or simulates via a fault-injecting `Vfs`) at the intent-then-kill point (AC-4) and the act-then-kill point (AC-5), then runs the reconciler and asserts journal + tree state.
- Unit tests for rename and copy-phase outcome verification (AC-2, AC-3).
- Cancellation test: cancel mid-job, assert coherent resumable state; plus a hand walkthrough recorded in the manual QA checklist (AC-8).
- Access-denied test with a permission-fault `Vfs` (AC-9).

Decision Gate: OQ resolution not required here. Confirm the FD-04 resume component contract (data shape) matches what v0.4.0 shipped; if it drifted, file a small frontend adjustment task.

Output Artifacts: `crates/abo-core/src/exec/reconcile.rs`; executor cancellation + access-denied changes in `exec/`; new IPC for resume/rollback-after-interruption; executor kill/cancel test suite.

Suggested Owner: LLM (Opus) - safety-critical.

## Phase 2: Hash verification

**Goal:** BLAKE3 over candidate members only, as a background job, gating set-aside behind verified hashes or explicit override. **Addresses:** AC-10, AC-11, AC-12, AC-13, AC-14, AC-15, AC-16.

Steps:
1. Add `blake3` (pinned) to `crates/abo-core/Cargo.toml`. Implement hashing in `crates/abo-core/src/dupes/` (e.g. `hash.rs`) operating only over `duplicate_members` of detected groups from F-701 (never a snapshot-wide walk).
2. Run hashing under the F-104 job model: spawn on the Tokio runtime, emit `job:progress`, honor cancel at file boundaries. Persist hash state on `duplicate_members` (AC-15) so re-open does not re-hash.
3. Implement the set-aside gate in the plan-generation path for the internal `dedupe-quarantine` pass (surfaced to the user as the "Copies" group, FD-26 (seven campaign groups)): refuse to emit set-aside ops for a group unless all members are hash-verified OR an explicit override flag is present on the request (AC-12).
4. Wire `dupes_hash_verify` IPC (already in the command surface) to the job.
5. Mark version candidates (same name+size, different hash) as a distinct, never-auto-resolved state (AC-14).

Verification:
- Table-driven tests: identical content -> equal hash -> resolvable; same name+size, different content -> distinct hashes -> version candidate, not auto-resolved (AC-14).
- Gate test: attempt to emit set-aside without verified hashes -> refused; with override flag -> allowed (AC-12).
- Persistence test: hash state survives a surface re-open (AC-15).
- Performance probe on a real-data copy feeds the descope decision (AC-16); record throughput in the campaign log.

Decision Gate: hash-performance descope trigger (AC-16). If throughput is unacceptable on real data, set the campaign to flag-only and record the decision; F-704 flag-only path (Phase 3) must be complete regardless.

Output Artifacts: `crates/abo-core/src/dupes/hash.rs`; migration touch if hash-state columns need adding; hash job wiring; dupes hash test suite.

Suggested Owner: LLM (Opus) - safety-adjacent (gates a destructive-adjacent action).

## Phase 3: Resolution policies + dedupe as a campaign group

**Goal:** four resolution policies feeding set-aside operations through the standard plan/apply/rollback pipeline. **Addresses:** AC-23, AC-24, AC-25, AC-26, AC-27.

Steps:
1. Implement the four policies in `crates/abo-core/src/dupes/` (e.g. `policy.rs`): keep-larger, keep-higher-bitrate, keep-m4b, flag-only (default). Each takes a group and returns a proposed keeper (flag-only returns a suggestion only).
2. For keep-higher-bitrate, resolve OQ-1 (spec Open Questions): if bitrate is unavailable without embedded-tag reading (F-1101 deferred), fall back to keep-larger and note it, or add a bounded lofty-subset read per FD-14 precedent. Await decision gate.
3. On user confirmation, emit set-aside operations into the user-facing "Copies" campaign group (FD-26 (seven campaign groups)), which maps to the internal `dedupe-quarantine` F-403 plan-pass id (an internal id only, never a UI or report label), via F-403 (plan builder); they flow through F-404 (plan validation) and F-601 (executor) unchanged (AC-25). flag-only emits no operations (AC-26).
4. Ensure set-aside losers go through F-605 (quarantine) preserving relative paths and provenance, so F-603 (rollback) restores them (AC-27).

Verification:
- Table-driven policy tests over fixture groups (AC-23, AC-24).
- flag-only emits zero operations (AC-26).
- Dedupe round-trip test on fixtures: resolve -> set aside -> rollback -> tree byte-identical (AC-27); the real-data-copy version is exercised in Phase 8 / campaign log.

Decision Gate: OQ-1 (keep-higher-bitrate bitrate source). Resolve before implementing that policy; the other three do not depend on it.

Output Artifacts: `crates/abo-core/src/dupes/policy.rs`; plan-builder `dedupe-quarantine` wiring; policy test suite.

Suggested Owner: LLM (Opus) - couples to journal/rollback.

## Phase 4: Duplicate review + report

**Goal:** the data and CSV export for group-by-group duplicate review, with GROUP as the canonical unit. **Addresses:** AC-17, AC-18, AC-19, AC-20, AC-21, AC-22.

Steps:
1. Add IPC payloads (`crates/abo-core/src/ipc.rs`) for a duplicates overview: list of groups, each with copy count, byte total, keeper suggestion, hash state. Counts are GROUPS; members are "copies" (AC-17, AC-18).
2. Implement CSV export in `crates/abo-core/src/dupes/` (one row per copy, group-key column); language and totals count groups (AC-20). Export lands in the reports folder (F-1002).
3. Bake the FD-10 guarantee copy and FD-08 register into the report strings module (centralized strings, FD-23). Primary vocabulary "set aside," never "deleted" as primary (AC-21).
4. Ensure no sample numbers are hardcoded; counts derive from the scan (AC-22).

Verification:
- Snapshot test of the CSV over a fixture with known groups (AC-20).
- String/register test asserting GROUP counts and the FD-10 guarantee sentence appear; assert "dedupe"/"operations"/"quarantine" absent from user-facing strings (AC-21).

Decision Gate: N/A.

Output Artifacts: duplicates IPC payloads; CSV exporter; report/strings entries; dupes report test suite.

Suggested Owner: LLM (Sonnet) - mechanical, table/serde-driven.

## Phase 5: Duplicates surface (F-905)

**Goal:** the React surface hosting F-703 review and F-704 policy selection. **Addresses:** AC-28, AC-29, AC-30, AC-31.

Steps:
1. Build the duplicates route under `src/` using generated bindings only (no raw `invoke`), TanStack Query for the groups list, Zustand for selection state.
2. Render group-by-group review with the FD-13 "Show file details" disclosure for copy locations; policy selector for the four F-704 policies (AC-28, AC-30).
3. Nav badge shows the GROUP count, updated on `job:completed` (event-driven, no polling) (AC-29).
4. Implement the F-702 override as an explicit warning-confirm dialog using the FD-09 danger token pair (AC-30, AC-13).
5. Wire FD-04 empty ("no duplicates found") and loading ("checking copies") states (AC-31).

Verification:
- Vitest component tests for selection + policy state and for the override two-step affordance.
- axe-core smoke on the surface (FD-21); contrast check of the danger token pair in both themes.
- Manual QA: keyboard walkthrough item added to the release checklist.

Decision Gate: N/A (consumes Phase 2/4 contracts).

Output Artifacts: `src/` duplicates route + components; nav badge wiring; Vitest tests.

Suggested Owner: LLM (Sonnet).

## Phase 6: Ruleset import/export (F-802)

**Goal:** portable JSON rulesets validated against a versioned schema, round-trip deterministic. **Addresses:** AC-32, AC-33, AC-34, AC-35.

Steps:
1. Implement export/import in `crates/abo-core/src/ruleset.rs`: serialize a ruleset (templates + policies + toggles) to JSON with `schema_version`; deserialize with schema validation.
2. Handle version mismatch per OQ-2 resolution (additive-migrate vs reject-with-remediation); emit an `AppError` on reject with a remediation string.
3. Import creates a new row; same-name import requires explicit confirmation (AC-34).
4. Add IPC commands for import/export (extend the ruleset CRUD surface); wire a minimal affordance in the settings/ruleset editor (F-906) - file picker via tauri-plugin-dialog (FD-29, frontend never touches fs directly).

Verification:
- Round-trip test: export -> import on a clean DB -> generate plan from the same snapshot -> byte-identical to the original (against the F-403 determinism golden) (AC-35).
- Schema-mismatch test: a bad/old version file is handled explicitly, never silently misparsed (AC-33).

Decision Gate: OQ-2 (schema-mismatch handling). Resolve before finalizing import behavior.

Output Artifacts: ruleset import/export in `ruleset.rs`; IPC commands; settings affordance; ruleset round-trip test.

Suggested Owner: LLM (Sonnet) - serde-driven, guarded by the determinism golden.

## Phase 7: Everything view (F-501 redefined)

**Goal:** virtualized full change list grouped by campaign group, tier-1 disclosure, responsive at scale. **Addresses:** AC-36, AC-37, AC-38, AC-39.

Steps:
1. Build the everything-view route under `src/` using TanStack Virtual over the paged plan (`plan_get` with filter), grouped by campaign group (AC-36).
2. Position it as a tier-1 disclosure entry, not the default review path (default stays the per-group cards from v0.4.0) (AC-37).
3. Row detail behind "Show file details" shows source/target plus matched pattern and confidence (FD-13 tier-1 content, F-504) (AC-38).
4. Add an optional tree-presentation toggle behind a flag; its absence must not block anything (AC-39).

Verification:
- Responsiveness check over the real 2026-03-25 baseline (718 folders / 13,970 files, labeled "2026-03-25 baseline") and, separately, the 20,000-file / 1,000-folder NFR scale target (no freeze in either; the two are not conflated); recorded in manual QA (AC-36).
- Vitest test for grouping and the disclosure content (AC-38).
- Confirm the descope path: the view (and independently the tree toggle) can be disabled without breaking the default review flow (AC-39).

Decision Gate: F-501 responsiveness descope trigger (AC-39). If unstable at end of window, disable and slip; do not block the tag.

Output Artifacts: `src/` everything-view route + components; Vitest tests.

Suggested Owner: LLM (Sonnet).

## Phase 8: Long-path battle testing + release gate

**Goal:** prove the full pipeline over >260-char paths with detect-and-warn, then verify the composite release gate. **Addresses:** AC-40, AC-41.

Steps:
1. Extend the fixture generator (from v0.2.0) to materialize runtime-only paths beyond 260 chars (never committed; generated into the temp dir per CI notes) (AC-40).
2. Add an integration suite running scan -> plan -> validate -> dry-run apply -> rollback over those fixtures using extended-length (`\\?\`) semantics (AC-40).
3. Implement/verify the FD-19 detect-and-warn UX: detect `LongPathsEnabled=0`, warn with a linked how-to on over-limit targets; retain near-260 warnings (AC-41).
4. Run the full composite release gate from the spec (kill/cancel, dedupe end-to-end on a real-data copy, ruleset round-trip, everything-view responsiveness, accessibility FD-21) and record evidence in `docs/internal/test-strategy.md`-referenced logs.

Verification:
- Long-path integration suite green on the Windows runner (AC-40).
- Detect-and-warn test (AC-41).
- Composite gate checklist all green; Fable reviews before tag.

Decision Gate: this is the tag gate. F-606 items are blocking; P1 items may descope per triggers. Fable signs off.

Output Artifacts: long-path fixtures + integration suite; detect-and-warn implementation; completed release-gate evidence log.

Suggested Owner: LLM (Opus) for the safety-critical long-path/executor work; Fable for the gate review.

## Test-First Posture

Per test-strategy Executor layer, the following tests are written before the implementation they cover, where practical:
- P1: kill-injection and cancellation tests before the reconciler/cancellation code.
- P2: hash-gate and version-candidate tests before the hashing/gating code.
- P3: policy table tests and the dedupe round-trip test before the policy/plan wiring.
- P6: the ruleset round-trip determinism test before import/export code.
- P8: the long-path integration suite before the detect-and-warn implementation.

## Branch / PR Plan

- One short-lived feature branch per phase (or per P4-P7 surface cluster), PR into `main`, agent self-merge on green while the repo is private (D-11, EXECUTION.md).
- Required green checks per PR: lint (fmt, clippy -D warnings, core-purity, bindings-drift), test matrix (ubuntu + windows, including the new suites), Windows build+bundle (macOS honesty-only). The RealFs rollback round-trip and, from this release, the kill/resume reconciliation tests run on every merge.
- P1-P3 merge in order; P4-P7 may merge in any order after P2/P3; P8 merges last and precedes the tag.
- Tag `v0.6.0` is cut from a green `main`; publishing the tag/release is human-only (D-10, S2 governance).

## Risks and Descope Triggers

- Hash performance unacceptable on real data -> dedupe runs flag-only; set-aside-by-hash is post-campaign (spec AC-16). Flag-only path (P3) must be complete regardless.
- F-501 (everything view) not responsive/stable by end of window -> slip it (and, first, the tree toggle) without blocking the tag (spec AC-39).
- Any executor invariant test flaky -> freeze the release until deflaked; the one accepted slippage point (S2 Section 5).
- OQ-1 (bitrate source) and OQ-2 (ruleset schema mismatch) unresolved -> block only their own phase steps (keep-higher-bitrate; import version handling), not the release.

## Definition of Done

The spec's Release Gate, restated as the exit checklist:
- [ ] Kill-during-apply reconciles in both windows; cancelled apply coherent and resumable; access-denied retry-once-then-halt-group (P0, blocking).
- [ ] Dedupe end to end on a real-data copy (candidates -> hash -> policy -> set aside -> rollback restores).
- [ ] Set-aside gated on verified hashes or explicit override.
- [ ] Duplicates counted as GROUPS on surface, badge, and report.
- [ ] Ruleset import/export round-trip yields a byte-identical plan.
- [ ] Everything view responsive at library scale (or descoped per trigger).
- [ ] Long-path battle test green; detect-and-warn verified.
- [ ] Accessibility verified (FD-21) on the three new/changed surfaces in both themes.
- [ ] CI matrix green on `main`; Fable has reviewed the gate; tag cut from green `main` (publish is human-only).
