---
id: v0.5.0-plan
title: "Implementation Plan: Release v0.5.0 (acting) - executor and rollback (alpha)"
type: implementation-plan
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (release-implementation-plan)
linked-spec: docs/internal/releases/v0.5.0-acting/spec.md
depends_on: docs/internal/releases/v0.4.0-seeing/implementation-plan.md
phase-count: 8
ac-coverage: complete
executor-model-guidance: >
  Per FD-30 (model-tiering execution policy): the executor, journal/manifest,
  rollback, and validation-of-inverse-plan are safety-critical and are Opus-tier
  with Fable reviewing every gate. The Vfs seam and MemFs are Opus-tier (they
  underpin every executor test). Post-apply verification is Opus-tier (it guards
  further groups). The apply + activity surface and table-driven executor
  fixtures are Sonnet-tier. Quarantine mechanics are Opus-tier (safety path).
sources:
  - docs/internal/releases/v0.5.0-acting/spec.md
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.5.0, Section 6 CI, 6.4 test strategy)
  - _local/planning/feature-function-breakdown_2026-07-02.md (E-06, F-904, Section 6 IPC, Section 8 errors)
  - E:/Projects/product-on-purpose/repo-sync-tool/EXECUTION.md (branch/merge policy)
  - docs/internal/decision-ledger.md (D-08, D-09, D-10, FD-01, FD-02, FD-04, FD-19, FD-30)
---

# Implementation Plan: Release v0.5.0 (acting) - executor and rollback (alpha)

## Task Summary

Status: complete (all 8 phases done; gate walked 2026-07-19; merged to main 2026-07-20 via PRs #23, #30, #27, #28, #29; AC-17 ratification + tag awaiting jp per D-10).
This plan decomposes v0.5.0 (acting) into eight phases. The order is deliberate and safety-first: the Vfs seam and MemFs land before any executor logic, so every executor behavior is written and tested against memory before it can ever touch a real disk. The rollback round-trip (the release's signature gate) and never-overwrite adversarial tests are written as failing tests before the code that makes them pass, where practical. The hard rule from D-10 (human-only Real apply) holds throughout: nothing in this plan runs a Real apply against the actual library at E:\Books - Audio; agent-safe execution is fixtures and disposable copies only.

Phase status is tracked in the Completion Status table below. AC status lives in the spec.

## Completion Status

| Phase | Goal | Fulfills AC | Owner | Status |
|---|---|---|---|---|
| P1 | Vfs seam + MemFs + dry-run harness | AC-1, AC-2, AC-3 | LLM (Opus) | Done |
| P2 | Journal + manifest (journal-before-act, provenance, export) | AC-10, AC-11, AC-12, AC-13 | LLM (Opus) | Done |
| P3 | Executor core (rename-first, TOCTOU, never-overwrite, single-writer, access-denied) | AC-4, AC-5, AC-6, AC-7, AC-8, AC-9 | LLM (Opus) | Done |
| P4 | Quarantine (set-aside with provenance) | AC-21, AC-22, AC-23 | LLM (Opus) | Done |
| P5 | Rollback as a plan + round-trip signature gate | AC-14, AC-15, AC-16, AC-17 | LLM (Opus) + human (copy round-trips) | Done |
| P6 | Post-apply verification + delta metrics + block-further-groups | AC-18, AC-19, AC-20 | LLM (Opus) | Done |
| P7 | Pause/resume + Stop (IPC + executor cooperation) | AC-24, AC-25, AC-26 | LLM (Opus) | Done |
| P8 | Apply + activity surface (F-904) | AC-27, AC-28, AC-29, AC-30, AC-31 | LLM (Sonnet) + Fable review | Done |

## Phase 1: Vfs seam + MemFs + dry-run harness

**Goal:** land the `Vfs` trait with `RealFs` and `MemFs` so all later executor code is generic over the filesystem and testable against memory. **Addresses:** AC-1, AC-2, AC-3.

**Steps:**
1. Add `crates/abo-core/src/exec/vfs.rs` defining a `Vfs` trait: `exists`, `is_dir`, `rename`, `copy_file`, `remove_file`, `remove_dir`, `create_dir_all`, `metadata` (size), with `\\?\` extended-length path handling centralized here (FD-19).
2. Implement `RealFs` (delegates to `std::fs` with extended-length prefixing on Windows) and `MemFs` (in-memory tree seeded from a snapshot) in `exec/vfs.rs`.
3. Add `crates/abo-core/src/exec/mod.rs` with an `Executor<V: Vfs>` skeleton that consumes an approved plan and a `Vfs`; no operation logic yet, just the walk-and-dispatch loop.
4. Wire `apply_start(plan_id, mode: DryRun|Real)` in `src-tauri/src/commands/` to construct `Executor<MemFs>` for DryRun (seeded from the plan's snapshot) and `Executor<RealFs>` for Real; export via tauri-specta.
5. Add the `Vfs`-seam smoke test that fails if a DryRun run creates any real path.

**Verification:** `cargo test -p abo-core exec::vfs` passes; a DryRun over a fixture plan touches no disk (temp-dir watcher assertion); executor operation logic contains no direct `std::fs` calls (grep-style unit assertion or clippy lint).

**Decision Gate:** the journal shape is defined here in the abstract (the entries P2 will write) so P2 and P3 agree; confirm the intent/done/failed row shape before P2 starts.

**Output Artifacts:** `exec/vfs.rs`, `exec/mod.rs` skeleton, `apply_start` command wiring, first Vfs test module.

**Suggested Owner:** LLM (Opus) - the seam underpins every safety test.

## Phase 2: Journal + manifest

**Goal:** journal-before-act with flush, manifest export to Reports, and provenance carried per FD-01. **Addresses:** AC-10, AC-11, AC-12, AC-13.

**Steps:**
1. Confirm the `journal` and `manifests` tables from the v0.1.0 migration match the needed shape (`journal`: job_id, seq, op_id, phase, at, detail_json; `manifests`: id, job_id, plan_id, json_path, reversible). Add an additive migration under `crates/abo-core/migrations/` only if fields are missing (schema is still pre-v1, additive-safe).
2. Add `crates/abo-core/src/exec/journal.rs`: `write_intent(op)` flushes an `intent` row before returning; `write_done(op)` / `write_failed(op, err)` append terminal rows. A failed intent flush returns `journal-write-failed` and the executor must not proceed (AC-13).
3. Include each op's F-507 (pack provenance capture and report) provenance in `detail_json` for intent rows (AC-12).
4. Add `crates/abo-core/src/exec/manifest.rs`: on job completion export a self-contained reverse-executable JSON manifest to the Reports folder (F-1002), and re-emit the provenance report reflecting final locations (AC-12).
5. Tests first: a kill-between-intent-and-act test (simulated by injecting a panic after intent flush) leaves exactly one op with intent and no terminal row (AC-10); a manifest-round-trip test reads the JSON back without the app DB and reconstructs the reverse op list (AC-11).

**Verification:** `cargo test -p abo-core exec::journal exec::manifest` passes; manifest JSON validates against its own schema and is readable standalone.

**Decision Gate:** OQ-1 (manifest schema versioning) resolved here: embed `manifest_schema_version` and pin the provenance fields against F-507.

**Output Artifacts:** `exec/journal.rs`, `exec/manifest.rs`, optional additive migration, journal/manifest test modules, exported manifest + provenance report samples in fixtures.

**Suggested Owner:** LLM (Opus).

## Phase 3: Executor core

**Goal:** rename-first same-volume, copy+verify+delete cross-volume, TOCTOU re-checks, never-overwrite, single-writer, access-denied semantics. **Addresses:** AC-4, AC-5, AC-6, AC-7, AC-8, AC-9.

**Steps:**
1. In `exec/mod.rs`, implement per-op dispatch: `move`/`rename` same-volume via `Vfs::rename` (no copy, AC-4); cross-volume detected by volume prefix, routed to copy + size verify (+ hash verify where the plan op marked it) + delete-source, halting on mismatch with `copy-verify-mismatch` (AC-5).
2. Add the pre-op TOCTOU backstop: re-check source exists and target absent (NTFS case-insensitive) before every op; map failures to `source-vanished` / `target-appeared` and halt the group with a consistent journal (AC-6).
3. Enforce never-overwrite: the target-absent check is the gate; the write path never opens a target with truncate/overwrite semantics (AC-7).
4. Single-writer: acquire a job lock row in SQLite plus an in-process mutex at `apply_start`; a second start returns `job-already-running`; release on completion, failure, and crash-detected startup (AC-8).
5. Access-denied handling: wrap RealFs ops so an access-denied error retries exactly once, then halts the current campaign group with the FD-19 error-taxonomy entry (AC-9).
6. Tests first (MemFs where possible, RealFs temp-dir where the behavior is filesystem-specific): rename-no-copy counter test (AC-4); cross-volume verify-mismatch leaves source (AC-5); TOCTOU source-vanished/target-appeared (AC-6); adversarial never-overwrite with a concurrent writer creating the target mid-apply (AC-7); double-start rejection (AC-8); access-denied retry-once-then-halt via an injected-error Vfs (AC-9).

**Verification:** `cargo test -p abo-core exec` green including the adversarial never-overwrite suite; journal-consistency assertion (every intent has a terminal row) holds after each halt path.

**Decision Gate:** N/A (behavior fixed by spec R-2, R-3, R-4).

**Output Artifacts:** completed `exec/mod.rs` operation logic, injected-error test Vfs, executor test suites.

**Suggested Owner:** LLM (Opus) - safety-critical core.

## Phase 4: Quarantine

**Goal:** set-aside area outside the library, provenance preserved, retention manual, no audio deletion anywhere. **Addresses:** AC-21, AC-22, AC-23.

**Steps:**
1. Add `crates/abo-core/src/exec/quarantine.rs`: resolve the quarantine root from settings (F-803) defaulting outside the library root (E:\Books - Audio\Quarantine\<job-id>\); build the destination as quarantine-root + original relative path (AC-21).
2. Record per-item quarantine reason and provenance in the journal detail and manifest (AC-22).
3. Assert no delete-of-audio anywhere: a test scans the executor and quarantine code paths (and runs a fixture apply+rollback) confirming no audio file is removed, only set-aside or empty-dir removed (AC-23).
4. Add the retention note surface content (consumed by F-904 and the report): the user empties set-aside manually.

**Verification:** `cargo test -p abo-core exec::quarantine`; provenance and original relative path recoverable from the quarantine record; delete-of-audio scan test green.

**Decision Gate:** N/A.

**Output Artifacts:** `exec/quarantine.rs`, quarantine test module, retention copy string in the strings module (FD-23 centralization).

**Suggested Owner:** LLM (Opus).

## Phase 5: Rollback as a plan + round-trip signature gate

**Goal:** rollback is an inverse plan through the same validate/preview/apply pipeline; round-trip is byte-identical. **Addresses:** AC-14, AC-15, AC-16, AC-17.

**Steps:**
1. Add `crates/abo-core/src/exec/rollback.rs`: `rollback_prepare(manifest_id)` reads the manifest, generates the inverse operation list, and persists it as a new plan so it flows through F-404 (plan validation) and the preview surface unchanged (AC-14).
2. Wire the `rollback_prepare` IPC command in `src-tauri` and export bindings.
3. Partial rollback: accept a contiguous journal tail selection; refuse non-contiguous selections with a clear error (AC-16).
4. Round-trip signature test (write first, expect red until P3+P5 complete): apply the full fixture plan for real in a temp dir via `Executor<RealFs>`, run rollback, recursive-hash-compare the tree to the original; wire this into `cargo test` so CI runs it on every merge from this release forward (AC-15). Reference the test-strategy.md executor layer.
5. Manual round-trip evidence (human/agent on copies only, never the live library): copy Genre - SciFI\Top 100 Sci-Fi Books, run apply+rollback, confirm byte-identical; then the gnarliest Hugo pack copy; record results in the release evidence log (AC-17).

**Verification:** `cargo test -p abo-core exec::rollback` and the round-trip test green in CI; manual copy round-trips recorded with hashes.

**Decision Gate:** the round-trip gate must be green before any consideration of a Real apply in a later human-only step; if flaky, invoke the descope trigger "any executor invariant test flaky -> release freezes until deflaked" (release plan Section 5).

**Output Artifacts:** `exec/rollback.rs`, `rollback_prepare` command, round-trip CI test, manual round-trip evidence entries.

**Suggested Owner:** LLM (Opus) for code; human (or agent on copies) for the manual round-trips.

## Phase 6: Post-apply verification

**Goal:** verify targets/sizes/sources, incremental rescan, delta metrics, block further groups on discrepancy. **Addresses:** AC-18, AC-19, AC-20.

**Steps:**
1. Add `crates/abo-core/src/exec/verify.rs`: after a job, for each op confirm target exists, size matches the snapshot, source is gone; collect discrepancies (AC-18).
2. Trigger an incremental rescan of affected roots (reuse F-101 scanner scoped to affected paths) and compute a delta health-metrics report from F-202 (library health metrics) before/after (AC-19).
3. On any discrepancy, set a job/campaign state that blocks approval and execution of further groups until acknowledged; expose it to F-904 (AC-20).
4. Tests: seed a fixture apply with an injected discrepancy (a target missing) and assert verification catches it and sets the blocking state.

**Verification:** `cargo test -p abo-core exec::verify`; delta metrics computed correctly on a fixture; blocking state observable via `job_status`.

**Decision Gate:** N/A.

**Output Artifacts:** `exec/verify.rs`, verification test module, delta-metrics payload struct in `abo-core::ipc`.

**Suggested Owner:** LLM (Opus).

## Phase 7: Pause/resume + Stop

**Goal:** cooperative between-operations pause/resume and a Stop cancel, journal unaffected. **Addresses:** AC-24, AC-25, AC-26.

**Steps:**
1. Extend the executor loop to check a pause flag and a cancel token at operation boundaries only (never mid-op); add `job_pause(job_id)` and `job_resume(job_id)` IPC commands and export bindings (AC-24).
2. Ensure pause/resume does not write any journal entry of its own: journal sequence after pause/resume equals an uninterrupted run (AC-25).
3. Wire Stop to the existing F-104 (job progress + cancel) cancellation token, cancelling at a safe boundary and leaving a coherent partial state; confirm "Skip ahead" is not implemented anywhere (demo-only, AC-26).
4. Tests: pause mid-run then resume, diff journal against an uninterrupted run (AC-25); stop mid-run, assert consistent journal and resumable/coherent state (AC-26).

**Verification:** `cargo test -p abo-core exec` pause/stop cases green; journal-equality test passes.

**Decision Gate:** N/A (F-608 semantics fixed by FD-02).

**Output Artifacts:** `job_pause`/`job_resume` commands, pause flag in the executor, pause/stop test modules.

**Suggested Owner:** LLM (Opus) - touches the executor loop.

## Phase 8: Apply + activity surface (F-904)

**Goal:** the deliberately boring apply surface with journal tail, Stop/Pause, and family-safe failure/resume states. **Addresses:** AC-27, AC-28, AC-29, AC-30, AC-31.

**Steps:**
1. Add the apply/activity surface under `src/` (React) rendering one job, a scrolling journal tail from `apply:op-executed` events as plain sentences (no raw paths), one overall state, one primary action (AC-27). Use generated bindings only, no raw `invoke`.
2. Wire Stop and Pause buttons to `job_cancel` and `job_pause`/`job_resume`; toggle the pause label "Pause between books" / "Resume" (AC-28), matching prototype 07-complete-flow.html register.
3. Implement the FD-04 failure surface and the post-verification blocked-further-groups state; keep raw OS error + code + remediation behind "Show file details" (AC-29, FD-13).
4. Place the FD-10 canon deletion-guarantee string exactly and use "set aside" as primary quarantine vocabulary (AC-30); pull all copy from the centralized strings module (FD-23).
5. Apply error/danger states using the dedicated error/danger token pair; run the mechanical contrast check (FD-21) for both data-theme="day" and data-theme="evening" (AC-31); add an axe-core smoke test on the surface.
6. Tests: Vitest component tests for the state machine (running -> paused -> running -> done, running -> failed, done-blocked-by-discrepancy); axe-core smoke; contrast check in CI (from v0.4.0 baseline).

**Verification:** `pnpm test` component + axe smoke green; contrast script green for both themes; manual QA checklist walkthrough of the apply surface on Windows recorded.

**Decision Gate:** N/A.

**Output Artifacts:** apply/activity surface component(s), Vitest tests, strings-module entries, contrast/axe CI hooks.

**Suggested Owner:** LLM (Sonnet) for the surface and tests; Fable reviews against the design-system register and FD-04/FD-10 copy.

## Branch / PR plan

- One short-lived feature branch per phase (or per adjacent pair P1+P2, P3+P4) off `main`, PR into `main`, agent self-merge on green while the repo is private, per EXECUTION.md and D-11 (governance). No force-push.
- Required green CI checks per merge (release plan Section 6.1): lint (fmt, clippy -D warnings, core-purity, typecheck, bindings-drift), test matrix (ubuntu + windows), build (windows GA + macOS honesty). From this release forward the test job includes the RealFs rollback round-trip in a temp dir.
- P5 must not merge until the round-trip signature test is green in CI on the Windows runner.
- Human-only gate reminder (D-10): no PR or CI step performs a Real apply against the actual library; agent-safe execution is fixtures and disposable copies only.

## Risks and descope triggers

| Risk / trigger | Pre-agreed action |
|---|---|
| Any executor invariant test flaky (round-trip, never-overwrite, single-writer) | Release freezes until deflaked; this is the one place slippage is accepted, not descoped (release plan Section 5). |
| Cross-volume copy+verify path slow or fragile on real copies | Keep same-volume rename as the primary tested path (D-08); mark cross-volume behind explicit plan flags; revisit hash-verify default with F-702 in v0.6.0 (OQ-2). |
| macOS bundle red in CI > 1 week | Downgrade macOS job to allow-fail + tracking issue; never block this Windows release on it (release plan Section 5). |
| Access-denied behavior hard to reproduce for tests | Use an injected-error Vfs to simulate access-denied deterministically (Phase 3 step 5); document real Defender/Controlled-Folder-Access checks for the M-1 runbook (FD-19), not this release. |
| Pause/resume perturbs the journal | Treat as an invariant failure (journal-equality test is the gate); pause is metadata-only, never a journal event (FD-02). |

## Definition of Done

The release gate, restated as the exit checklist (all must be green before v0.5.0 tags; tag cut is human-only):

- [x] All eight phases show Done in the Completion Status table.
- [x] Rollback round-trip signature test green in CI on Windows and running on every merge (AC-15). Evidence: green on the 2026-07-20 main-push merge run; the round-trip is part of cargo test --workspace, a required check per EXECUTION.md Sections 6-7.
- [x] Never-overwrite adversarial suite green; journal consistent after every halt path (AC-7).
- [x] Dry-run and Real produce identical journals modulo documented fields (AC-3).
- [ ] Manual round-trip recorded on a COPY of Top 100 Sci-Fi subset and on a COPY of the gnarliest Hugo pack, each byte-identical after rollback (AC-17).
- [x] Quarantine preserves relative paths and provenance; no audio deleted anywhere (AC-21, AC-22, AC-23).
- [x] Post-apply provenance report re-emitted (AC-12); manifest exported and standalone-readable (AC-11).
- [x] Single-writer rejection, access-denied retry-once-then-halt-group, TOCTOU halts all covered by tests (AC-6, AC-8, AC-9).
- [x] Pause/resume leaves the journal unaffected; Stop leaves a coherent state; no "Skip ahead" ships (AC-24, AC-25, AC-26).
- [x] Apply surface: FD-10 canon guarantee copy, FD-04 failure/blocked states, error tokens WCAG AA in both themes (AC-29, AC-30, AC-31).
- [x] HARD RULE verified: no Real apply against the actual library occurred in producing any of this evidence (D-10).
