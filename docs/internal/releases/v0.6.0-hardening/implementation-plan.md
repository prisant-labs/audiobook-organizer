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
| P0 | History + undo reachable (read model, screen, rollback wiring) | (scope change, see above) | LLM (Opus) | MERGED 2026-07-31 |
| P1 | Interruption safety + resume (reconciler, cancellation, access-denied) | AC-1..AC-9 | LLM (Opus) | **COMPLETE.** P1a/P1b/P1d MERGED 2026-07-31; P1c MERGED 2026-08-05 (PR #11). AC-8 hand walkthrough still owed by jp |
| P2 | Hash verification (BLAKE3, candidates-only, gating) | AC-10..AC-16 | LLM (Opus) | **Engine MERGED 2026-08-06 (PR #15) BUT NOT REACHABLE FROM THE APP** (see the note below). **`AC-16` MEASURED 2026-08-15: descope trigger NOT met, ship as designed** ([evidence](hash-throughput-2026-08-15.md)). `AC-13` (two-step override) is the remainder, and it is an affordance on a resolve action that does not exist until `P3` (resolution) and a surface that does not exist until `P5` (`F-905`); see the `AC-13` note under Phase 5, step 4 |
| **P2b** | **Book-level duplicate comparison (F-1110)** | **AC-51..AC-55** | **LLM (Opus)** | **MERGED 2026-08-15 (PR #29).** All five criteria, engine-only, no IPC change. Descope path not taken |
| P3 | Resolution policies + dedupe as a campaign group | AC-23..AC-27 | LLM (Opus) | **Steps 1-2 MERGED 2026-08-15 (PR #34)**; **steps 3-4 done 2026-08-16**: confirmed resolutions emit ordinary Archive ops (`AC-25`) and the round-trip is proven byte-identical (`AC-27`). Scoped to FILE losers; see the note below. **Policies written against BOOKS, not files** (FD-44). `keep-higher-bitrate` cut per F-1108 |
| P4 | Duplicate review + report (data + CSV, group canon) | AC-17..AC-22 | LLM (Sonnet) | **Engine done 2026-08-16** (`dupes/review.rs`): the group-first review model and the `AC-20` CSV. IPC payloads and writing the file into the Reports folder are **deliberately `P5`'s**; see the note below |
| P5 | Duplicates surface (F-905) | AC-28..AC-31 | LLM (Sonnet) | **COMPLETE 2026-08-19/20**, in three open PRs: #41 (backend), #42 (the scale it is built on), #43 (the surface). `dupes_hash_verify` is the caller `P2`'s engine never had; the screen replaces the "coming soon" panel; `AC-13`'s control is wired at last. See the two notes below Phase 4 |
| P6 | Ruleset import/export (F-802) | AC-32..AC-35 | LLM (Sonnet) | Not started |
| P7 | Everything view (F-501 redefined) | AC-36..AC-39 | LLM (Sonnet) | Not started |
| P8 | Long-path battle testing + release gate | AC-40, AC-41 | LLM (Opus) + Fable | Not started |
| P9 | Library freshness: scan triggers + on-entry check (F-609) | AC-42..AC-46 | LLM (Sonnet) | **NEW 2026-08-05**, from the UI round 2 crit. Not started |
| P10 | Open a folder in the OS file manager (F-610) | AC-47..AC-50 | LLM (Sonnet) | **COMPLETE 2026-08-22.** All four criteria. `abo_core::reveal` owns the gate and the launch; two commands (`reveal_in_folder` by path, `reveal_root` by name); one `OpenFolder` component used on both path-bearing surfaces plus two permanent sidebar links. **The capability allowlist is unchanged** (`AC-47`): no `fs`, no `shell`, no new plugin. See the note below |


**A note on `P10`, because two decisions in it are worth keeping.**

**The gate lives in `abo_core`, not in the Tauri command.** `AC-48` requires the
command to refuse any path outside the library or Archive roots, and that check
is the only thing standing between a minimal-capability WebView and a general
"open any path on this machine" primitive. Putting it in the command layer would
make it true for exactly as long as the next caller remembered it, which is the
shape `AC-12` was found in and the shape precondition 3 is still in. It is
enforced in `abo_core::reveal::resolve_revealable`, which canonicalizes both
sides and compares them component-wise, and it FAILS CLOSED: a path that cannot
be resolved at all is refused for the same reason one outside the roots is.

**The sidebar links send a root NAME, not a path.** The Archive root is usually
absent from settings because the plan builder derives it, so a quick link that
passed a path would have to reconstruct that derivation in TypeScript and would
drift from the builder the first time it changed. `reveal_root(RevealRoot)` keeps
the rule in one place and means no path crosses the IPC boundary for those two
links at all.

**Three security tests were each proved against the specific defect they guard**,
by reintroducing it and watching only that test fail: a textual prefix check in
place of a component-wise one (which every other test survives, which is exactly
why the sibling-prefix case needed its own test), canonicalization removed from
both sides, and canonicalization removed from the target alone.

**Two phases added 2026-08-05** from jp's crit of the UI round 2 prototypes, both P1 and both descopable.

`P9` (`F-609`) closes a real trust gap rather than adding a feature: a plan is built from a stored scan, so it can describe a library that no longer exists, and today the app only notices at apply time, where the refusal reads as a failure at the worst moment. `AC-42` alone (scan age visible plus a manual re-scan) delivers most of the value and is trivial; `AC-43` to `AC-46` can move to v0.7.0 without blocking the tag.

`P10` (`F-610`) is jp's request for clickable folder paths, which cannot be a link because `FD-29` gives the web layer no shell access. A narrow backend command that refuses any path outside the library and set-aside roots keeps the capability model unchanged. Inline affordances throughout the tidy-up and review surfaces, plus two permanent sidebar quick links.

**Ordering note.** `F-1110` (multi-file book duplicate comparison) sits between `P2` and `P3` per `FD-44`, because `P3`'s resolution policies should be written against books rather than files. Doing it after `P3` means writing those policies twice. Scheduled as `P2b` on 2026-08-05; unblocked 2026-08-14 when jp settled `AC-53`.

### P2 reachability, found 2026-08-15 while measuring AC-16

**The `F-702` verification job is complete and cannot be run from the app.** Three facts establish it, and each was checked rather than inferred:

1. **Step 4 of Phase 2 below, "Wire `dupes_hash_verify` IPC (already in the command surface) to the job", was never done, and its parenthetical was wrong.** No command by that name exists anywhere in the repository.
2. **`verify_groups` has no callers** outside its own module's tests and the new `AC-16` measurement.
3. **Until this change there was no production `ContentSource` at all**: every implementation was an in-memory test double inside a `cfg(test)` module, so nothing shipping could have hashed a real file even if a command had called it. That is the strongest form of the finding, because it means the gap could not have been closed by wiring alone.

This is the same shape as the defect that created `P0`: the 2026-07-30 audit found v0.5.0's undo machinery complete but UNREACHABLE, with the History route a placeholder and no surface calling rollback. "Engine merged" is true and is not the same as "the feature works", and this plan said the first while reading like the second.

**Not fixed here, deliberately.** The command belongs with the surface that calls it (`P5`, `F-905`), and inventing an IPC entry point with no caller repeats the mistake in the other direction. What is fixed is the record: the status column above now says reachable-from-nothing rather than implying otherwise, and `FsContentSource` means the read path exists for `P5` to reach.

### P2b status detail (2026-08-14)

**What landed.** A match tier on folder duplicate groups: `TitleOnly` < `Fingerprint` < `Structural`, in `crates/abo-core/src/dupes/books.rs`, with the consumer half in `plan/query.rs` and `plan/report.rs` so a book-level duplicate is counted where a user can see it.

**Three design points worth not re-deriving:**

1. **F-1110 raises a tier on an existing group; it never emits one.** A fingerprint match implies a title match, so every book-level duplicate already sits inside a normalized-title version-candidate group. A second group would count the same book twice against `FD-08`. This is also what makes `AC-55` work: partitioning by fingerprint would split a one-file copy and a twelve-file copy into a group of one each, and a group of one is dropped, so they would stop grouping at all.
2. **The population is the classifier's `book` verdict, not "folder with a parsed title".** Measured on the standard fixture: `Genre - SciFI` parses the title "SciFI", and so do series containers and staging folders.
3. **A book folder must have no `book` ancestor.** A disc-split title classifies as `book` and so does each of its disc folders; `Verbal Advantage` alone produced five. Every book's `Disc 1` normalises to the title "disc 1", so without this rule unrelated books fingerprint-match through their disc folders.

**`AC-54`, the content tier.** Migration 0008 adds `duplicate_member_files`, one row per audio file beneath a FOLDER member, carrying the same three-state hash encoding migration 0007 uses. A folder has no hash, so inventing a folder digest was rejected: it would mean choosing a set and an order and defending both forever. Two folders match when their sorted multisets of file hashes agree.

The member-level `content_hash` / `hash_error` pair is deliberately left NULL for folder members, which is what keeps the `AC-12` auto-resolve gate shut for book groups **by construction**: the gate reads that column, finds nothing, and refuses. Asserted anyway, and the assertion was mutation-tested (stamping a member hash inside the verification job makes it fail).

**A counting defect found by rendering, not by testing.** Two copies of a twelve-part book carry identically named, identically sized parts, so the exact detector produces a group per part. Rendered over a library with one duplicated twelve-part book and one duplicated single-file book, the report read "Move 14 duplicate copies to the Archive" for two duplicated books. `FD-08` makes one book one group. An exact group is now marked `subsumed_by_book_group` when a candidate book group already reports the same duplication: the rows stay (they are true, and the complete change list may want them) but stop being counted. The rule requires the group to span at least TWO of the duplicated copies, because a disc-split book carries a `track01.mp3` on every disc.

**Fixtures.** The `F-1110` cases are purpose-built trees in `crates/abo-core/tests/book_dupe_detection.rs`, not additions to `standard_library_manifest`, which twelve test files and a snapshot directory read. The standard fixture contains no multi-file book copied twice; adding one is worth doing and is its own reviewable change, because it moves goldens everywhere at once.

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

**Kill-process tests landed 2026-07-31.** The spec's kill-injection requirement is
now met for real rather than simulated. `crates/abo-core/src/bin/kill_harness.rs` is a
feature-gated binary that runs an actual apply against a real temp library through
`RealFs` and then calls `std::process::abort` mid-operation - no unwinding, no `Drop`,
no userspace flush - and `crates/abo-core/tests/kill_recovery.rs` reconciles whatever
that leaves on disk. This matters because every other reconciler test builds the
in-doubt state by hand, which proves the reconciler reasons correctly about a given
database state but assumes the state a kill actually produces; the assumption was the
untested part. Both journal-before-act cases are covered: intent-then-kill (AC-4, the
source is still in place, resume restarts this op) and act-then-kill (AC-5, the rename
landed, the journal is repaired as `done` not `failed`, resume continues from the next
op). A third test pins the at-most-one-repaired-row invariant. The suite was
mutation-checked: inverting the completed-rename classification fails the AC-5 test and
correctly leaves the other two passing.

The harness is gated behind a `kill-harness` cargo feature (off by default, enabled by
the self dev-dependency for `cargo test -p abo-core`), so a normal build never produces
it and core purity is unaffected.

Still outstanding for P1: the AC-8 hand walkthrough (manual QA), and P1c.

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

**Status: MERGED to `main` 2026-07-31** (10 engine tests, 9 screen tests). Deliberately NOT in scope: per-operation drill-down, verification-discrepancy
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

Decision Gate: hash-performance descope trigger (AC-16). **MEASURED 2026-08-15; the trigger is NOT met. Recommendation: ship F-702 as designed, no descope.** Evidence: [hash-throughput-2026-08-15.md](hash-throughput-2026-08-15.md). The read path plus BLAKE3 sustain 2,765 MB/s while the library's 7200 RPM SATA drive delivers 42 to 80 MB/s, so the hashing code accounts for 2 to 3 percent of the wait and a descope would remove none of it. AC-10's candidates-only rule already cut the work by 95% (14.96 GB of candidates, not the 298.72 GB library): verifying one duplicate group takes about a second, and verifying all 293 takes 3 to 6 minutes once, in a cancellable background job whose results persist. The F-704 flag-only path (Phase 3) is still built regardless, per the original wording.

Output Artifacts: `crates/abo-core/src/dupes/hash.rs`; migration touch if hash-state columns need adding; hash job wiring; dupes hash test suite. **Added 2026-08-15 for AC-16:** `FsContentSource` in `hash.rs` (the production filesystem read path; every prior `ContentSource` was an in-memory test double, so nothing shipping could hash a real file) and `crates/abo-core/tests/real_library_hash_throughput.rs` (three `#[ignore]` operator-run measurements: candidate population, throughput, read-path ceiling).

Suggested Owner: LLM (Opus) - safety-adjacent (gates a destructive-adjacent action).

## Phase 3: Resolution policies + dedupe as a campaign group

**Goal:** four resolution policies feeding set-aside operations through the standard plan/apply/rollback pipeline. **Addresses:** AC-23, AC-24, AC-25, AC-26, AC-27.

Steps:
1. Implement the three policies in `crates/abo-core/src/dupes/` (e.g. `policy.rs`): keep-larger, keep-m4b, flag-only (default). Each takes a group and returns a proposed keeper (flag-only returns a suggestion only). Amended 2026-08-06: keep-higher-bitrate was cut as `F-1108`, which also closed `OQ-1` as moot.
2. Write these policies against BOOKS, not files, per `FD-44`. `keep-m4b` means "prefer the .m4b" against files but "prefer the copy that is one file over the copy that is twelve" against books, which is a materially different rule. This is why `P2b` (`F-1110`) is sequenced before this phase: building it after would mean writing the policies twice.
3. On user confirmation, emit Archive operations into the user-facing "Duplicates" campaign group (FD-26 (seven campaign groups), renamed by FD-46; the action is renamed by FD-42), which maps to the internal `dedupe-quarantine` F-403 plan-pass id (an internal id only, never a UI or report label), via F-403 (plan builder); they flow through F-404 (plan validation) and F-601 (executor) unchanged (AC-25). flag-only emits no operations (AC-26).
4. Ensure set-aside losers go through F-605 (quarantine) preserving relative paths and provenance, so F-603 (rollback) restores them (AC-27).

Verification:
- Table-driven policy tests over fixture groups (AC-23, AC-24). **Done**, 15 tests in `policy.rs`; the tie-break is mutation-tested (taking the LAST member at the best rank instead of the first fails two). Two defects were found by review after the first version and are worth not reintroducing: the keeper's REASON was derived from the winner's own category rather than from the axis that beat the runner-up (so two `.m4b` copies of different sizes reported "it is a .m4b and the others are not", which the reader can check against the paths and falsify), and ranking "is a book folder" above "is any other file" made a twelve-part book beat a single `.mp3`, inverting the exact comparison `FD-44` asks keep-m4b to make. A plain file counts as ONE file; being a book is not itself a merit.
- flag-only emits zero operations (AC-26). **Done.** It needs no branch in the builder: flag-only produces no confirmations, so the emission loop runs zero times. That is a stronger guarantee than a policy check, because there is no policy in scope to get wrong.
- Dedupe round-trip test on fixtures: resolve -> set aside -> rollback -> tree byte-identical (AC-27). **Done**, in `exec/rollback.rs`. It runs the REAL builder over a real two-copy library on disk and applies whatever the builder produced, rather than a hand-written `quarantine` op: the claim `AC-25` makes is about the builder, and a hand-written op would prove it about the hand-written op. The real-data-copy version is still Phase 8 / campaign log.

### P3 steps 3-4 note, 2026-08-16

**Scoped to FILE losers, deliberately.** A book-group loser is a FOLDER, and a cross-volume archive reaches `cross_volume_move` -> `copy_file`, which is file-only, so a non-empty directory is not proven to survive the round-trip. Folder losers are refused rather than half-supported. This costs nothing today: book groups cannot pass the `AC-12` gate by construction (folder members keep a NULL `content_hash`), and the `AC-13` override that would bypass it has no surface until `P5`.

**A confirmation is all-or-nothing.** If any part of one is unusable the whole confirmation is refused and that group keeps every copy. Archiving the usable subset would be an outcome nobody chose, and refusing cannot lose a book. Refused: an id not in this snapshot, the keeper listed among its own losers, an id already claimed by another confirmation (reachable, not theoretical: `F-1110`'s subsumption rule lets one file belong to two groups), a non-file loser, and a loser under a folder an earlier pass renamed or moved.

**That last one is the hard case, and it reuses the existing mechanism.** The plan records SNAPSHOT paths and the executor does not rebase them, so a loser inside a folder strip-noise renames would have its source invalidated before the archive op runs. `Builder::ancestor_relocated` already exists for exactly this reason (it is why clutter under a relocated folder is left alone), so this pass consults it rather than inventing a second mechanism in the same file. Tested, with an assertion that the fixture really does trigger the rename so the test cannot pass for the wrong reason.

**Producer 6 gained a sweep.** The builder's `rationale` sentences reach the review surface through the plan itself and nothing swept them: the TypeScript sweep walks copy objects, `report.rs` sweeps the report, `query.rs` sweeps the group cards, and none can see a string built in `builder.rs`. Adding a rationale meant adding the sweep, per the instruction the vocabulary test carries in its own header. It covers retired words and the internal terms, and is itself guarded by a `should_panic` test on "quarantine", the word most likely to leak because it is what the code calls these very operations.

### P3 finding, 2026-08-15: the policies discriminate almost nowhere resolution is permitted

Worth knowing before steps 3-4 are built, and worth `jp` seeing, because it is a property of the specified policies rather than a defect in them:

- An **exact** group is keyed on `(basename, size)`. Equal size means `keep-larger` cannot rank it; equal basename means equal extension, so `keep-m4b` cannot either. Both tie **always**, by construction of the key.
- A **fingerprint** book group requires agreement on audio count and total audio bytes (`AC-51`), so `keep-m4b` ties on it by construction too.
- Where both policies DO discriminate is title-only groups, and `AC-55` says those never auto-resolve, since choosing between one file and twelve is a preference rather than a mechanical ranking.

So on proven-identical copies **the tie-break is the de facto policy**. It is therefore first-class here: deterministic (first by path, the order members already arrive in), stated to the reader as `KeeperReason::Equivalent` rather than dressed up as a decision, and mutation-tested. This does not block anything; it does mean `P5`'s surface should expect "these were equivalent" to be the common message, not a rare one.

**One wording tension, flagged rather than silently resolved.** Step 2 above and `FD-44` give `keep-m4b` a book-level meaning ("prefer the copy that is one file over the copy that is twelve"), and `AC-55` says exactly that choice "is a preference, not a mechanical ranking" and such groups never auto-resolve. These reconcile: `AC-24` has the policy PROPOSE and the user CONFIRM, so no mechanical rule is ever the final word, and the architecture enforces it independently since a single-file copy is not a book folder and so its group can never reach `Fingerprint`, leaving the `AC-12` gate shut. Implemented under that reading. If `jp` reads `AC-55` as forbidding even a proposal, the change is small and local to `rank`.

Decision Gate: NONE remaining. `OQ-1` (keep-higher-bitrate bitrate source) was CLOSED as moot on 2026-08-05 when the policy itself was cut as `F-1108`. The three surviving policies never depended on it.

Output Artifacts: `crates/abo-core/src/dupes/policy.rs`; plan-builder `dedupe-quarantine` wiring; policy test suite.

Suggested Owner: LLM (Opus) - couples to journal/rollback.

## Phase 4: Duplicate review + report

**Goal:** the data and CSV export for group-by-group duplicate review, with GROUP as the canonical unit. **Addresses:** AC-17, AC-18, AC-19, AC-20, AC-21, AC-22.

Steps:
1. Add IPC payloads (`crates/abo-core/src/ipc.rs`) for a duplicates overview: list of groups, each with copy count, byte total, keeper suggestion, hash state. Counts are GROUPS; members are "copies" (AC-17, AC-18).
2. Implement CSV export in `crates/abo-core/src/dupes/` (one row per copy, group-key column); language and totals count groups (AC-20). Export lands in the reports folder (F-1002).
3. Bake the FD-10 guarantee copy and FD-08 register into the report strings module (centralized strings, FD-23). Primary vocabulary is **"moved to the Archive"**, never "deleted" (AC-21). *(Corrected 2026-08-16: this step said "set aside", which `FD-42` retired and which the spec's own amended `AC-21` already replaced. The step escaped the CI vocabulary gate only because that gate covers five governance files and not this one.)*
4. Ensure no sample numbers are hardcoded; counts derive from the scan (AC-22).

Verification:
- Snapshot test of the CSV over a fixture with known groups (AC-20). **Done**, plus a test that the byte column names itself an estimate rather than space saved (AC-18).
- String/register test asserting GROUP counts and the FD-10 guarantee sentence appear; assert "dedupe"/"operations"/"quarantine" absent from user-facing strings (AC-21). **Done**, and it also forbids "deleted", which is the word `AC-21` actually names.

### P4 note, 2026-08-16

**Where the data comes from, and why the join is by FILE.** Fresh detection owns the groups (the same re-detect-on-read posture the Copies card already uses, over the stored snapshot rather than the live disk); persisted rows own only the hash state, laid over it keyed by `entries.id`. A hash is a fact about a file, so it survives a change in how files are grouped, and group identity does not. `F-1110`'s subsumption rule changed grouping after hashes could already have been persisted, and under this design that is a non-event. Reading persisted GROUPS instead would have made group-id reconciliation a read-path problem; it belongs to the write path, at `P5`, where `insert_duplicate_groups` will need insert-or-reuse by `(scan_id, group_key)` because today it inserts blindly.

**Deliberately deferred to `P5`, decided rather than drifted into.** Step 1's IPC payloads and step 2's "lands in the Reports folder" both need a caller to be worth anything, and `P5` is the phase that knows what shape the surface wants. Adding `specta` types now would grow `bindings.ts` with types nothing imports, which is the same defect as a parameter nothing reads. `P5` therefore owns three things at once: the payload, the command that writes this CSV into the Reports folder, and `dupes_hash_verify`.

**Producer 7.** The CSV's headers and its plain-language labels are user-facing text generated in Rust, reachable by none of the six existing sweeps, so `review.rs` carries its own. It arrives with the code rather than after it, unlike two of the six before it.

**One defect this caught, by rendering the CSV and reading it.** The first draft had a single column headed "book (group)" carrying `Dune.m4b|900`, which promises a book name and delivers a detector artifact. It is now a readable `book` column beside a `group` join key, and naming a duplicate group moved out of `plan::report` into `dupes::review` so there is one implementation rather than two.

Decision Gate: N/A.

Output Artifacts: duplicates IPC payloads; CSV exporter; report/strings entries; dupes report test suite.

Suggested Owner: LLM (Sonnet) - mechanical, table/serde-driven.

### P5 backend note, 2026-08-19: the engine is reachable, the surface is not built

**The whole `P5` backend landed on `feat/p5-duplicate-write-path`** (four commits), and it closes the reachability finding recorded under Phase 2 above. `dupes_hash_verify` now EXISTS, and with it the chain a `scan_id` goes into and a rendered review comes out of. What remains of `P5` is the React surface (`AC-28`, `AC-29`, `AC-31`) and wiring the already-built `AC-13` control.

**What it owns, all four things the `P4` note deferred plus two that were not on the list:**

1. **`dupes_hash_verify`** as a background job (`AC-11`): its own `jobs` row (`kind = 'dupes-verify'`, filtered out of History, which reads `kind = 'apply'` only), `job:progress` per file, and cancellation through the existing `scan_cancel`, since the flag registry is keyed by `jobs.id` and job ids are unique across kinds. A cancelled pass ends as `cancelled`, never `failed`: it kept every hash it finished.
2. **`dupes_review`** and **`dupes_export_csv`**, both built from the same review so the export and the screen cannot disagree (`AC-20`).
3. **`dupes_confirm` and `dupes_clear_confirmation`**, with the confirmation tables migration 0009 adds.
4. **The plan build reads this scan's confirmations**, which is what makes a decision produce Archive operations. Until this it passed an empty slice and a confirmed resolution did nothing at all.
5. **`insert_duplicate_groups` is idempotent** (migration 0009's unique index on `(scan_id, method, group_key)`), because the verification job runs on every open of the surface.
6. **`AC-12`'s gate is now a MECHANISM rather than a convention.** `confirm_resolution_gated` refuses a resolution whose copies are not proven identical unless the caller passes an explicit override, and the override is RECORDED on the confirmation. This was found in review: the gate had been written into the command's caller, which is the same shape as precondition 3's `apply_start` taking its run mode from whoever calls it. Enforcing it in the backend means the guarantee holds for every caller, present and future.

**Three findings worth not re-deriving:**

- **`insert_duplicate_groups` had no production caller at all**, not merely a blind-insert bug. The whole duplicate persistence path was unreachable, the same shape as `P2`'s engine and `v0.5.0`'s undo machinery. That is the third instance; the cheap standing check is an import count against non-test, non-gallery files.
- **A test can pass and prove nothing.** The new integration test asserts a second scan plans clean; it passed with the `scan_id` filter deliberately removed, because `entries.id` is a global rowid so a second scan's ids never coincide with the first's. Its module doc now records which of three guards each test exercises.
- **`FD-20` snapshot retention will need to delete confirmations deliberately.** `entries.id` is a plain rowid and SQLite reuses rowids after deletion; the confirmation's foreign key currently refuses to let an entry be deleted out from under it, which is the guard retention will collide with.

**Known and accepted:** nothing stops a verification job running during an apply. The apply's single-writer lock counts `kind = 'apply'` only. The failure mode is mild (a file moved mid-hash records a read failure, which is never auto-retried) and the snapshot is stale after an apply regardless, so this is recorded rather than fixed.

### P5 surface note, 2026-08-20: the scale paid for itself immediately

**The whole screen added ZERO arbitrary values.** A route, a card component, a hook, four gallery specimens and two accessibility smokes, and the ratchet did not move: still 285 uses of 71 distinct, now across 74 files rather than 71. That is the measured answer to why the scale was ordered before `P5`. Built a week earlier, this surface would have added a dozen more inline sizes and pushed the baseline up, and the ratchet fails in both directions precisely so that could not be banked.

**Two defects found by rendering the gallery and reading it**, neither of which any test would have caught:

1. **The new fixture paths used single backslashes**, so `\B` and `\F` were consumed as escape sequences and every path rendered as `E:Books - AudioFrank HerbertDune`. The existing fixtures had it right.
2. **The file name cannot tell two copies apart.** An exact duplicate group is KEYED on basename and size, so every copy in one is called the same thing by construction. The first version rendered two identical rows of `Dune.m4b` and asked a person to choose between them. Copies now show the last three path components, with the full path in the `FD-13` disclosure.

**`AC-13` is wired.** `UnverifiedArchiveConfirm` was built on 2026-08-15 and connected to nothing; a verified group now confirms directly, and an unverified one routes through the two-step override. The card chooses the affordance and never decides whether the rule applies, because `confirm_resolution_gated` refuses regardless of what a screen believes.

**`AC-28`'s policy selector landed 2026-08-21.** Three policies, and the selector SAYS OUT LOUD that it usually changes nothing, which is the part that needed care rather than the wiring: exact groups tie under keep-larger by construction (they are keyed on size), and fingerprint book groups tie under keep-m4b because `AC-51` already requires an agreeing audio count. A person who switched it and saw nothing move would otherwise think the app was broken. Tests pin that a policy never changes whether a group may be ACTED on, since `AC-12`'s gate is a fact about content rather than a preference about keeping.

**A second lesson about tests, from the same commit.** The selector's first version hardcoded the radios' `name`, so two instances formed one native radio group and the browser drew both unselected. The obvious regression test PASSED WITH THE BUG IN, because React sets `checked` as a controlled property and jsdom never applies native radio grouping. The test now asserts the mechanism (distinct `name` per instance), which is observable and does fail when the fix is removed. **Assert the fix, not the symptom, when the symptom is only visible in a real browser.**

**Still owed for `P5`:** `AC-29`'s nav badge already reads the group count from `LibraryOverview` and needs a look once a real scan has duplicates in it, and a headed walkthrough of the whole flow. Those are the remainder rather than the phase.

### Scale proposal, 2026-08-19: ordered BEFORE P5, and why that is a measurement rather than a preference

**Proposed, not ratified**, on `design/spacing-and-type-scale-proposal`: seven type steps and seven spacing steps, rendered in both themes as the first section of the dev gallery (`src/gallery/scale.ts` plus `src/gallery/ScaleProposal.tsx`). Nothing was written into `src/styles/tokens.css`, which is normative: writing steps there is the ratification, not the proposal. Both files are deleted whichever way jp decides.

**Why before `P5` rather than after.** The review surface's components are studded with inline sizes, and PR #25 (the arbitrary-value ratchet) fails in BOTH directions precisely so that an improvement cannot bank slack for new sprawl. A whole new surface built the same way pushes the baseline UP; building `P5` first means adding to the sprawl and then redoing it.

**The measurement corrected the brief, and the correction is the transferable part.** The plan was to derive both scales from the 71 inline values the ratchet counts. That is right for type, where 161 of those uses are sizes across 18 distinct values. It is wrong for spacing: only 5 inline uses are spacing, while 292 uses of 16 standard utility steps carry the real decisions. A spacing scale derived from the inline values alone would have been derived from a sample of three numbers.

**Consequence for `P5`, worth knowing before it starts.** Every proposed spacing step is a standard step the app already uses, so ratifying that half adds no token and renames nothing: it rules 9 of the 16 steps out and leaves 7. The type half is the real work, because the app reaches type almost entirely through inline sizes. `P5` written against a ratified scale therefore costs nothing extra on spacing and lowers the ratchet on type.

**Two defects in the proposal were caught by rendering it and reading the output**, which is now four sessions running. A hand-written claim that nothing moved by more than 1.5px sat directly above a table showing two rows moving 2px, and is now computed from the table it describes. And the only uses of the 20px and 24px standard steps in the tree are the gallery's own headings rather than product UI, which had put a step in the scale that no product screen asks for; the standard-utility counts now exclude `src/gallery` and say so on the page.

**Waiting on jp:** ratify, amend or reject. Ratification means transcribing the steps into `@theme` in `tokens.css` and deleting the gallery section, then lowering the ratchet as components adopt it.

## Phase 5: Duplicates surface (F-905)

**Goal:** the React surface hosting F-703 review and F-704 policy selection. **Addresses:** AC-28, AC-29, AC-30, AC-31.

Steps:
1. Build the duplicates route under `src/` using generated bindings only (no raw `invoke`), TanStack Query for the groups list, Zustand for selection state.
2. Render group-by-group review with the FD-13 "Show file details" disclosure for copy locations; policy selector for the THREE F-704 policies (AC-28, AC-30). **Corrected 2026-08-21: this step said four.** `keep-higher-bitrate` was cut as `F-1108` and the core has read "the three F-704 policies" since `P3`; the fourth was stale wording rather than a missing feature.
3. Nav badge shows the GROUP count, updated on `job:completed` (event-driven, no polling) (AC-29).
4. Implement the F-702 override as an explicit warning-confirm using the FD-09 danger token pair (AC-30, AC-13). **The CONTROL is already built (2026-08-15): `src/components/review/UnverifiedArchiveConfirm.tsx`, with its copy, its two-step guarantee, component tests, an axe smoke on both steps, and a gallery specimen in both themes. What remains here is WIRING it to a resolve action, which does not exist until Phase 3.** Note the mechanism: spec AC-12 says "warning dialog", design-system Section 7 says confirming is never a modal, and AC-13 asks for consistency with the design system, so it is the canonical inline two-step strip (Section 4.14) rather than a modal.
5. Wire FD-04 empty ("no duplicates found") and loading ("checking copies") states (AC-31).

Verification:
- Vitest component tests for selection + policy state and for the override two-step affordance. **The override half is done**: the two-step guarantee is mutation-tested (making the first press act fails three tests).
- axe-core smoke on the surface (FD-21); contrast check of the danger token pair in both themes. **The override control's own axe smoke is done, both steps.**
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

- One short-lived feature branch per phase (or per P4-P7 surface cluster), PR into `main`. **Merging is a human decision**: the repo went public on 2026-07-31 (FD-38), which lapsed D-11's agent self-merge allowance. Green CI is still required before any merge, it is simply no longer sufficient on its own.
- Required green checks per PR: lint (fmt, clippy -D warnings, core-purity, bindings-drift), test matrix (ubuntu + windows, including the new suites), Windows build+bundle (macOS honesty-only). The RealFs rollback round-trip and, from this release, the kill/resume reconciliation tests run on every merge.
- P1-P3 merge in order; P4-P7 may merge in any order after P2/P3; P8 merges last and precedes the tag.
- Tag `v0.6.0` is cut from a green `main`; publishing the tag/release is human-only (D-10, S2 governance).

## Risks and Descope Triggers

- Hash performance unacceptable on real data -> dedupe runs flag-only; set-aside-by-hash is post-campaign (spec AC-16). Flag-only path (P3) must be complete regardless.
- F-501 (everything view) not responsive/stable by end of window -> slip it (and, first, the tree toggle) without blocking the tag (spec AC-39).
- Any executor invariant test flaky -> freeze the release until deflaked; the one accepted slippage point (S2 Section 5).
- OQ-2 (ruleset schema mismatch) unresolved -> blocks only its own phase step (import version handling), not the release. `OQ-1` (bitrate source) is closed as moot: `F-1108` cut the policy that needed it.

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
