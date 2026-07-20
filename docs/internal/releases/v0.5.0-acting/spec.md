---
id: v0.5.0
title: "Release v0.5.0 (acting) - executor and rollback (alpha)"
date: 2026-07-03
status: review
owner: jprisant
tier: release-effort
scope: "Engine execution and safety: apply approved plans and undo them, proven on fixtures and copies only"
depends_on: docs/internal/releases/v0.4.0-seeing/spec.md
produced-by: AUTHOR agent (release-spec)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.5.0)
  - _local/planning/feature-function-breakdown_2026-07-02.md (E-06, F-904)
  - PRODUCT.md
  - docs/internal/design-system.md
  - docs/internal/test-strategy.md
  - docs/internal/decision-ledger.md (D-08, D-09, D-10, FD-01, FD-02, FD-04, FD-09, FD-10, FD-13, FD-19, FD-30)
  - docs/internal/planning-audit-2026-07-03.md (Stream 2, items 1, 4, 6)
  - _local/gui/07-complete-flow.html (apply/activity/done copy reference)
---

# Spec: Release v0.5.0 (acting) - executor and rollback (alpha)

## Task Summary

Status: review (pending jp approval of the planning suite).
Release theme: the dangerous release. This is the first release that changes files on disk. It lands the executor, the journal, undo, post-apply verification, and quarantine, all proven exclusively against fixtures and disposable copies. No Real apply against the actual library happens in this release regardless of how green the suite is (D-10 human-only gate).

Feature checklist (AC live per feature below):
- [x] F-607 (dry-run harness): Vfs seam, MemFs, dry-run is the same executor against memory
- [x] F-601 (executor): rename-first same-volume, copy+verify+delete cross-volume, TOCTOU re-checks, single-writer
- [x] F-602 (journal + undo manifest): journal-before-act, JSON manifest export, provenance carried
- [x] F-603 (rollback): inverse plan through the same validate/preview/apply pipeline
- [x] F-604 (post-apply verification): targets exist, sizes match, sources gone, delta health metrics
- [x] F-605 (quarantine): set-aside area outside the library root with provenance and retention note
- [x] F-608 (pause and resume apply): between-operations pause, journal unaffected
- [x] F-904 (apply + activity surface): one boring job, scrolling journal tail, Stop and Pause, failure/resume surfaces

Open questions: 2 (see Open Questions).

## Purpose

Everything before this release describes or previews changes. v0.5.0 (acting) is where the tool earns its reason to exist: it applies an approved plan and can undo it completely. The strategy of the whole ladder is that catastrophe lives in the file mover, so the executor arrives only after two full releases of parsing and planning (v0.2.0, v0.3.0) have hardened the data it consumes, and after v0.4.0 (seeing) has let a human review and approve a real plan.

The release is engineered so that the risky code is exhaustively testable without touching disk: the executor runs against a virtual filesystem seam (F-607, dry-run harness), so dry-run is literally the same executor running against memory, and Real apply is the same executor running against the real filesystem. The signature proof of the release is a rollback round-trip: apply a full plan for real in a temp dir, roll back, and verify the tree is byte-identical.

## Scope

In scope: the eight features listed in the Task Summary. These implement epic E-06 (execution and safety) except F-606 (interruption safety + resume), which is deliberately held to v0.6.0 (hardening), plus F-904 (apply + activity surface) from E-09 (GUI surfaces) and the new F-608 (pause and resume apply) per FD-02.

The load-bearing invariants for this release, all from D-09 (safety invariants): quarantine-only (no delete of audio anywhere), journal-before-act, single-writer (one apply job process-wide), the Vfs seam so dry-run is the same executor against memory, rollback is "just another plan" through the same validate/preview/apply pipeline, and never-overwrite.

## Non-Goals

- No Real (non-dry-run) apply against the actual library at E:\Books - Audio. That is a human-only action per D-10 (full-ladder go scope) and Section 2 of the release plan. This release proves safety on fixtures and copies only. [decision-ledger D-10]
- No interruption/crash reconciliation and resume-after-kill. That is F-606 (interruption safety + resume), held to v0.6.0 (hardening). This release's pause (F-608) is a cooperative between-operations pause of a running job, not crash recovery. [feature-function-breakdown E-06]
- No duplicate hash verification or dedupe resolution flow. F-702 (hash verification), F-703 (duplicate review + report), F-704 (resolution policies) are v0.6.0. Quarantine here is the mechanism (F-605); dedupe as a campaign group that uses it comes later.
- No automatic emptying of the set-aside area. Retention is manual by design (F-605); the tool never deletes what it set aside.
- No new parsing, classification, or planning capability. This release consumes the frozen plan contract from v0.3.0 (planning) and the surfaces from v0.4.0 (seeing).

## Users / Actors

- jp (tier 1): runs fixture and copy campaigns, reads the journal and verification detail, performs the manual round-trip on copies, decides when any Real apply happens (a later human-only gate, not this release).
- Household members (tier 2): the apply + activity surface (F-904) is authored to their bar. No paths, no "operations" vocabulary, no exit codes as the primary interface. Plain-language copy: books being moved, changes made, undo available. Technical truth stays behind "Show file details" (FD-13).
- Implementing and verifying agents: the executor, journal/rollback, and validation are safety-critical and Opus-tier (FD-30); the apply surface and table-driven executor tests are lower-tier.

## Requirements

R-1. The executor operates through a `Vfs` trait with `RealFs` and `MemFs` implementations, so the same executor code path serves dry-run (against `MemFs` seeded from the snapshot) and Real apply (against `RealFs`). Dry-run and Real produce the same journal shape [S: feature-function-breakdown F-607; release-plan Section 4 v0.5.0].

R-2. Same-volume operations use filesystem rename (metadata-only, no data copied); cross-volume operations use copy + verify + delete-source, explicitly marked in the plan. This is D-08 (rename-first executor): the feasible 297 GB full copy is spent as a pre-campaign backup, not as the apply mechanism [decision-ledger D-08; feature-function-breakdown F-601].

R-3. Before each operation the executor re-checks that the source exists and the target does not (TOCTOU backstop, NTFS case-insensitive comparison), and never overwrites an existing target [S: feature-function-breakdown F-601; error taxonomy `target-appeared`, `source-vanished`].

R-4. At most one apply job exists process-wide (single-writer rule), enforced by a job lock in SQLite plus an in-process mutex; a second `apply_start` fails with `job-already-running` [decision-ledger D-09; feature-function-breakdown F-601].

R-5. Before executing each operation, an `intent` journal row is appended and flushed; after execution a `done` (or `failed` + error) row is appended. The completed journal is the undo manifest, and it also exports as JSON to the Reports folder so recovery never depends on the app database being healthy [S: feature-function-breakdown F-602].

R-6. Pack/award provenance recorded at plan time (F-507, pack provenance capture and report) is carried in the journal and the exported manifest, and the provenance report is re-emitted after apply reflecting final locations [decision-ledger FD-01].

R-7. Rollback is not a special code path: given a manifest, the tool generates the inverse plan, validates it with the same F-404 (plan validation) machinery, previews it with the same UI, and applies it with the same executor. Partial rollback selects a contiguous tail of the journal [S: feature-function-breakdown F-603].

R-8. After a job, the tool verifies each target exists, sizes match the snapshot, and sources are gone; triggers an incremental rescan of affected roots; and produces a delta health-metrics report. Discrepancies flag loudly and block further campaign groups until acknowledged [S: feature-function-breakdown F-604].

R-9. Quarantine (the set-aside area) lives outside the library root, preserves each item's original relative path for self-evident provenance, records why each item was set aside, and is emptied only by the user [S: feature-function-breakdown F-605; D-09].

R-10. An apply job can be paused between operations and resumed via `job_pause`/`job_resume` IPC; pause takes effect at operation boundaries only and leaves the journal unaffected. This powers the prototype's "Pause between books" control. A cooperative Stop control (F-104 semantics, safe boundaries) also exists [decision-ledger FD-02].

R-11. Access-denied during a Real apply operation is retried once, then halts the current campaign group with a family-safe surface and an error-taxonomy entry; the scanner and executor use extended-length (\\?\) path semantics [decision-ledger FD-19].

R-12. The apply + activity surface (F-904) is deliberately boring: one job, a scrolling journal tail rendered as plain sentences, one big unambiguous state, Stop and Pause controls, and a designed failure/resume surface (FD-04). It never shows raw paths on the primary surface; those live behind "Show file details" (FD-13). Deletion-guarantee copy uses the FD-10 canon [decision-ledger FD-04, FD-10, FD-13; PRODUCT.md].

## Acceptance Criteria

### F-607 (dry-run harness)

- [x] AC-1. A `Vfs` trait exists in `abo-core::exec` with `RealFs` and `MemFs` implementations; the executor is generic over `Vfs` and contains no direct `std::fs` calls in its operation logic. [S: feature-function-breakdown F-607]
- [x] AC-2. `apply_start(plan_id, mode: DryRun)` runs the full approved plan against a `MemFs` seeded from the plan's snapshot and completes without touching disk (verified by a test that fails if any real path is created). [S: feature-function-breakdown F-607; IPC `apply_start`]
- [x] AC-3. A dry-run and a Real apply over identical inputs produce identical journal entry sequences except for phase-timing metadata and the RealFs/MemFs marker (byte-compared in a test, modulo those documented fields). [S: release-plan Section 4 v0.5.0 gate]

### F-601 (executor)

- [x] AC-4. Same-volume `move` and `rename` operations complete via filesystem rename with no byte copy (asserted by a MemFs counter and, on RealFs, by timing/inode-stability where observable). [decision-ledger D-08]
- [x] AC-5. A cross-volume operation performs copy, then size verify (and hash verify where the plan marked it), then delete-source, in that order; a verify mismatch halts with `copy-verify-mismatch` and leaves the source intact. [S: feature-function-breakdown F-601; error taxonomy]
- [x] AC-6. Before each operation the executor re-checks source-exists and target-absent (case-insensitive). A source that vanished halts the group with `source-vanished`; a target that appeared halts with `target-appeared`; in both cases the journal is left consistent. [S: feature-function-breakdown F-601]
- [x] AC-7. Never-overwrite is adversarially tested: a target file is created mid-apply by a concurrent writer; the executor halts with `target-appeared` and never overwrites it, and the partial journal is internally consistent (every `intent` has a matching `done` or `failed`). [S: release-plan Section 4 v0.5.0 gate; decision-ledger AC additions]
- [x] AC-8. A second `apply_start` while a job is running fails immediately with `job-already-running`; the single-writer lock is held in SQLite and released on job completion, failure, or crash-detected startup. [decision-ledger D-09]
- [x] AC-9. On a Real apply operation returning access-denied, the executor retries exactly once; a second failure halts the current campaign group (not the whole job silently) and surfaces the FD-19 access-denied error-taxonomy entry; targets use \\?\ extended-length semantics. [decision-ledger FD-19]

### F-602 (journal + undo manifest)

- [x] AC-10. For every executed operation an `intent` row is written and flushed to the `journal` table before the filesystem call, and a `done` or `failed` row after; a test that kills the process between intent and act leaves exactly one operation with an `intent` and no terminal row (the in-doubt entry; reconciliation itself is v0.6.0). [S: feature-function-breakdown F-602]
- [x] AC-11. A completed apply job exports its manifest as JSON to the Reports folder; the manifest is self-contained (op ids, source, target, kind, order) and reverse-executable without reading the app database. [S: feature-function-breakdown F-602; F-1002 reports folder]
- [x] AC-12. Each journal entry and the exported manifest carry the operation's pack/award provenance from F-507 (pack provenance capture and report); after apply the provenance report is re-emitted to Reports reflecting final locations. [decision-ledger FD-01]
- [x] AC-13. A `journal-write-failed` condition is a hard stop: the executor does not proceed to the filesystem call if the intent flush fails. [S: error taxonomy `journal-write-failed`]

### F-603 (rollback)

- [x] AC-14. `rollback_prepare(manifest_id)` produces an inverse `PlanId` that passes the same F-404 (plan validation) checks and is previewable through the same surface a forward plan uses. [S: feature-function-breakdown F-603; IPC `rollback_prepare`]
- [x] AC-15. Rollback round-trip signature gate: apply the full fixture plan for real in a temp dir, roll back, and verify the tree is byte-identical to the original by recursive hash compare. This test runs in CI on every merge from this release forward. [S: release-plan Section 4 v0.5.0 gate; test-strategy.md executor layer]
- [x] AC-16. Partial rollback of a contiguous journal tail restores exactly the operations in that tail and leaves earlier operations applied; a non-contiguous selection is refused. [S: feature-function-breakdown F-603]
- [ ] AC-17. Manual round-trip evidence recorded: the round-trip performed by hand on a COPY of Genre - SciFI\Top 100 Sci-Fi Books, then on a COPY of the gnarliest Hugo pack (copies only, agent-safe), each byte-identical after rollback. [S: release-plan Section 4 v0.5.0 gate; decision-ledger AC additions]

### F-604 (post-apply verification)

- [x] AC-18. After a job the verifier confirms each target exists, each moved item's size matches the snapshot, and each source path is gone; any discrepancy is reported per-operation. [S: feature-function-breakdown F-604]
- [x] AC-19. Verification triggers an incremental rescan of affected roots and emits a delta health-metrics report (for example, noisy names count before to after). [S: feature-function-breakdown F-604]
- [x] AC-20. A detected discrepancy blocks approval and execution of further campaign groups until the user acknowledges it; the block is surfaced in the activity surface, not silent. [S: feature-function-breakdown F-604; FD-04]

### F-605 (quarantine)

- [x] AC-21. Quarantine root resolves outside the library root (default beside the library, for example E:\Books - Audio\Quarantine\<job-id>\), and set-aside items preserve their original relative path under the job folder. [S: feature-function-breakdown F-605]
- [x] AC-22. Every set-aside item records a reason (duplicate-of X, non-preferred format, clutter class) and its provenance; a test asserts the reason and original relative path are recoverable from the quarantine record. [S: feature-function-breakdown F-605; D-09]
- [x] AC-23. Nothing in the quarantine path is auto-deleted by the product; a retention note states the user empties it manually. No audio file is deleted anywhere in the apply or rollback path (asserted by a test that scans for delete-of-audio calls). [decision-ledger D-09, FD-10]

### F-608 (pause and resume apply)

- [x] AC-24. `job_pause(job_id)` causes the running apply job to stop before the next operation and enter a paused state; `job_resume(job_id)` continues from the next operation. Pause never interrupts an in-progress filesystem operation. [decision-ledger FD-02; IPC additions]
- [x] AC-25. Pausing and resuming leaves the journal unaffected: the sequence of entries after a pause/resume is identical to an uninterrupted run over the same inputs. [decision-ledger FD-02]
- [x] AC-26. A Stop control performs a cooperative cancel at a safe operation boundary (F-104 semantics), leaving a consistent journal and a coherent partial state; "Skip ahead" from the prototypes is demo-only and does not ship. [decision-ledger FD-02]

### F-904 (apply + activity surface)

- [x] AC-27. The apply surface shows exactly one running job with a scrolling journal tail rendered as plain sentences (no raw paths, no "operation" vocabulary), one unambiguous overall state, and a single primary action per state. [S: feature-function-breakdown F-904; PRODUCT.md principle 5]
- [x] AC-28. The surface exposes working Stop and Pause controls wired to F-608 and the cancel token; the pause label toggles between "Pause between books" and "Resume". [decision-ledger FD-02; prototype 07-complete-flow.html]
- [x] AC-29. A failure during apply renders the FD-04 family-safe failure surface (what happened in plain language, what is safe, what to do next), and a blocked-further-groups state after a verification discrepancy; neither shows a raw OS error without its code and remediation behind "Show file details". [decision-ledger FD-04, FD-13]
- [x] AC-30. The deletion guarantee appears using the FD-10 canon copy exactly: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone." Primary vocabulary for quarantine stays "set aside". [decision-ledger FD-10]
- [x] AC-31. Error and danger states use the dedicated error/danger token pair (distinct from --alert terracotta), verified WCAG AA in both data-theme="day" and data-theme="evening". [decision-ledger FD-09, FD-21]

## Behavior / Examples

Dry-run equals Real, mechanically (AC-3). The user (or a test) runs `apply_start(plan, DryRun)`; the executor walks the approved operations against `MemFs`, writing the same intent/done journal rows it would write on `RealFs`. Switching to `Real` runs the identical code against `RealFs`. The two journals diff only in timestamps and the filesystem marker. This is what makes the dry run a first-class product (D-04) rather than a separate simulation.

Never-overwrite under attack (AC-7). A test seeds a plan that will move Book A into folder X. Mid-apply, a concurrent writer creates X. When the executor reaches that operation, its target-absent re-check fails; it writes a `failed` row with `target-appeared`, halts the group, and does not touch X. Recursive hash of everything already moved matches expectations; the journal has no dangling intent.

Apply surface copy (AC-27, AC-30), lifted to register from prototype 07-complete-flow.html: "Books are being moved into their new folders. Files are renamed, never copied, so this is quick and nothing is duplicated or deleted. You can stop between books at any time." Progress line: "N of M changes made, undo file growing alongside." Done state: "982 changes went through, every one was double-checked afterwards, and the undo file is saved." (982 is sample data per FD-27, illustration only.)

Quarantine provenance (AC-21, AC-22). A non-preferred m4b loser from a parallel-format book is set aside to E:\Books - Audio\Quarantine\<job-id>\<original relative path>\, with a record: reason "non-preferred format (kept m4b sibling)", original path, job id. A rollback of that job's tail restores it to its original location byte-identical.

## Non-Functional Requirements

- Safety (the whole point): no operation overwrites or deletes audio; quarantine is the only removal; journal precedes every mutation; single-writer enforced. [PRODUCT.md; NFR table]
- Determinism: dry-run and Real journals identical modulo documented fields; rollback round-trip byte-identical. [release-plan test strategy]
- Recoverability: an apply killed between intent and act leaves at most one operation in doubt (full reconciliation is v0.6.0). [NFR table; feature-function-breakdown F-606 boundary]
- Responsiveness: apply runs on the Tokio runtime with `apply:op-executed` events feeding the journal tail; no UI freeze, no polling. [feature-function-breakdown Section 6]
- Accessibility: apply/failure states meet WCAG AA in both themes; status is icon plus label, never color alone; dedicated error token pair. [PRODUCT.md; FD-09, FD-21]
- Windows reality: \\?\ extended-length paths; access-denied retry-once-then-halt-group; case-insensitive collision checks. [FD-19]

## Revisions

| Date | Change | Author |
|---|---|---|
| 2026-07-03 | Initial spec authored for the planning suite. | AUTHOR agent |
| 2026-07-19 | Build evidence recorded; all AC checked except AC-17 (reduced-scope evidence pending jp ratification). | Fable orchestrator |

## Build evidence (2026-07-17 to 2026-07-19)

Built across PRs #23 (P1+P2), #24 (P3+P4), #27 (P5+P6), #28 (P7+P8), branch chain main..feat/v0.5.0-apply (32 commits, d016bbf head). Every phase passed an Opus adversarial task review (fix waves re-reviewed to Approved); a final whole-branch review verified the six cross-phase seams, mechanical sweeps (trailers 32/32, zero dashes, capabilities exactly 7, core purity, plan_ops/journal immutability), and returned READY. Fine-grained evidence trail: `.superpowers/sdd/progress.md`.

- F-607: Vfs seam with uniform cross-backend error contract; AC-3 equality proven RealFs-vs-MemFs with distinct job ids.
- F-601: never-overwrite enforced at the OS primitive (MoveFileExW without replace; create_new copies); adversarial mid-apply target test; single-writer = flag + BEGIN IMMEDIATE row + single-instance plugin + startup reclaim.
- F-602: journal-before-act structural, kill-test proven; manifest schema v1 with three-way mode marker; unmarked legacy files fail safe to dry-run.
- F-603: rollback is a real plan through F-404 and the same executor; AC-15 signature gate deterministic 0.06s (test in cargo suite; CI activation pending the billing fix); two-phase inverse ordering so set-aside teardown halts can never strand restorations.
- F-604: per-op verification through the job's own Vfs; append-only verification_blocks; block gates approval AND execution, undo provably never trapped.
- F-605: FD-34 sibling "Set Aside" root with {job-id} substitution; no-audio-delete confined by test and by the seam exposing no recursive delete.
- F-608: pause at op boundaries before the intent write; journal byte-equality structural; Stop = distinct stopped state.
- F-904: mode-aware plain-sentence surface; FD-10 canon byte-exact; FD-04 three-part failure panel from the per-code copy map; headed live gate walk (4 runs, both themes, 0 console errors) with MemFs-speed deviations recorded.

AC-15 CI note: green locally and wired into cargo test; "green in CI on every merge" activates when GitHub Actions billing is restored (BILLING INCIDENT 2, merges queued).
AC-17 status: round-trips performed on disclosed 3-book subset copies of both named folders (Top 100 Sci-Fi 479MB, Hugo pack 990MB), byte-identical SHA-256 before/after AND set-aside fully drained; scratch copies retained at E:\tmp\abo-rt\ for a fuller run. Awaiting jp: ratify the reduced scope or request the fuller copy round-trip.

## Sources & Evidence

| Ref | Source | Class |
|---|---|---|
| S1 | _local/planning/release-plan-and-ci_2026-07-02.md, Section 4 v0.5.0 and Section 6.4 test strategy | A (ratified planning doc) |
| S2 | _local/planning/feature-function-breakdown_2026-07-02.md, E-06 (F-601..F-607), F-904, Section 6 IPC, Section 8 error taxonomy | A |
| S3 | PRODUCT.md (design contract, register, safety principles) | A (authoritative contract) |
| S4 | docs/internal/decision-ledger.md decisions D-08, D-09, D-10, FD-01, FD-02, FD-04, FD-09, FD-10, FD-13, FD-19, FD-21, FD-27, FD-30 | A (decision ledger) |
| S5 | docs/internal/planning-audit-2026-07-03.md, Stream 2 items 1, 4, 6 | A |
| S6 | _local/gui/07-complete-flow.html (apply/activity/done copy reference) | B (prototype, sample data) |
| S7 | docs/internal/design-system.md (copy register, theme tokens); docs/internal/test-strategy.md (test layers, evidence conventions) | A (companion docs in this suite) |

## Open Questions

- OQ-1. Manifest JSON schema versioning: the manifest must survive being read by a future version for undo. Proposed: embed a `manifest_schema_version` and keep readers backward-compatible; confirm the exact fields carried for provenance (FD-01) at implementation time against the F-507 schema. [owner: implementing agent, v0.5.0]
- OQ-2. Cross-volume hash-verify default: F-601 marks cross-volume ops copy+verify+delete with size verify always and hash verify where the plan marked it. Whether hash verify is default-on for all cross-volume audio moves (safer, slower) or opt-in is deferred to the executor implementation and revisited with F-702 (hash verification) in v0.6.0. [owner: jp, before any cross-volume Real apply]
