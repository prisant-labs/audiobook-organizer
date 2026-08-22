---
id: v0.6.0
title: "Release v0.6.0 (hardening): interruption safety, duplicates, everything view"
type: spec
date: 2026-07-03
status: review
owner: jprisant
tier: release-effort
scope: hardening
depends_on: v0.5.0-acting
produced-by: author agent (release spec)
sources:
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - PRODUCT.md
  - docs/internal/decision-ledger.md
  - docs/internal/planning-audit-2026-07-03.md
source-count: 5
ac-count: 41
---

# Spec: Release v0.6.0 (hardening)

## Task Summary

- Status: planned (suite approved 2026-07-03 per D-10; not yet started).
- Release theme: survive the real world - crashes, cancellations, duplicate resolution, ruleset portability, and the full-library review surface.
- Depends on: v0.5.0-acting (executor, journal + undo manifest, rollback, quarantine, dry-run harness, apply + activity surface).
- Features in scope: F-606, F-702, F-703, F-704, F-905, F-802, F-501 (redefined), plus long-path battle testing.
- Open questions: 2 (see Open Questions).
- Last updated: 2026-07-03.

### AC checklist (all unchecked at review time)

- [ ] AC-1..AC-9 F-606 (interruption safety + resume)
- [ ] AC-10..AC-16 F-702 (hash verification)
- [ ] AC-17..AC-22 F-703 (duplicate review + report)
- [ ] AC-23..AC-27 F-704 (resolution policies)
- [ ] AC-28..AC-31 F-905 (duplicates surface)
- [ ] AC-32..AC-35 F-802 (ruleset import/export)
- [ ] AC-36..AC-39 F-501 (everything view)
- [ ] AC-40..AC-41 long-path battle testing

## Purpose

v0.5.0 (acting) proved the executor and rollback are correct on fixtures and on copies of real data. It proved correctness under controlled conditions. It did not prove survival: a machine that dies mid-apply, a user who cancels, a library full of duplicate copies that must be resolved by content and not by name, a ruleset that must move between machines, and a full-library change list a tier-1 reviewer wants to read exhaustively. This release closes those gaps so the M-1 (campaign: real-library reorganization) milestone can run against the actual 297 GB tree with confidence. Nothing here adds new pipeline stages; it hardens the stages that already exist and adds the two review surfaces (duplicates, everything view) that a real campaign needs.

## Scope

In scope: F-606 (interruption safety + resume), F-702 (hash verification), F-703 (duplicate review + report), F-704 (resolution policies), F-905 (duplicates surface), F-802 (ruleset import/export), F-501 (everything view, redefined per FD-06 (F-501 redefinition)), and long-path battle testing across the full pipeline. Every feature except F-606 is P1 and carries a descope path; F-606 is P0 and blocks the tag.

## Non-Goals

- No Real (non-dry-run) apply against the actual library. That is the M-1 (campaign) milestone, human-only, and remains out of every software release (D-10 go-scope, D-11 governance).
- No ABS-side push of duplicate or provenance data (F-1102 (ABS API integration), deferred to v1.1+ per D-14 (provenance in v1)).
- No hash-everything-on-scan. Hashing is candidates-only, on demand (discovery's explicit anti-pattern honored).
- No tree drawing as the primary review surface. F-501 is a grouped virtualized list; tree presentation is optional and never blocks the release (D-16 (cards + report primary), FD-06).
- No new error/empty/loading surface families. F-606 reuses the FD-04 (F-908 error, empty, and loading states) surfaces authored in v0.4.0-seeing.

## Users / Actors

- Tier 1 (jp): runs campaigns, resolves duplicate groups, reads the everything view, imports/exports rulesets, and is the human who chooses resume-or-rollback after an interruption.
- Tier 2 (household, non-engineers): may review a duplicates result or an everything view; must never see a raw path, journal, or "operation" as the primary interface (PRODUCT.md tier-2 bar, D-03 (audience tiers)).
- The executor and startup reconciler (system actors) act on the journal without a human present until the resume-or-rollback decision.

## Requirements

Interruption safety (F-606 (interruption safety + resume)) is the load-bearing P0. The journal-before-act invariant from v0.5.0 (F-602 (journal + undo manifest)) guarantees that at most one operation is ever in doubt after a kill: an operation whose `intent` row was flushed but whose `done`/`failed` row was not. On startup the reconciler must verify the actual on-disk outcome of that one operation and repair the journal, then present the human a family-safe resume-or-rollback choice through the FD-04 (F-908 states) surface [S1, decision-ledger FD-04].

Hash verification (F-702 (hash verification)) uses BLAKE3 over candidate members only, as a background job with progress, and gates any set-aside action on a duplicate group behind verified hashes or an explicit user override with a warning confirm [S1, planning audit stream 2 item 15, docs/internal/planning-audit-2026-07-03.md]. Duplicate review (F-703 (duplicate review + report)) and resolution policies (F-704 (resolution policies)) treat the GROUP as the canonical unit: one book, N identical copies; the surface, the nav badge, and the exported report all count groups, and member files are "copies" (FD-08 (group canon)) [S4 decision-ledger FD-08]. The duplicates surface (F-905 (duplicates surface)) hosts F-703. Ruleset import/export (F-802 (ruleset import/export)) makes rulesets portable JSON validated against a versioned schema. The everything view (F-501 (everything view), redefined) is a virtualized full change list grouped by campaign group, a tier-1 disclosure surface, responsive at the full library scale [decision-ledger FD-06]. Long-path battle testing runs the full pipeline over runtime-generated paths beyond 260 characters and verifies the FD-19 detect-and-warn UX [S1, decision-ledger FD-19].

## Acceptance Criteria

### F-606 (interruption safety + resume) - P0

- **AC-1** On startup, if the journal holds `intent` rows with no matching `done`/`failed`, the reconciler identifies exactly one in-doubt operation (the single-writer rule and journal-before-act flush guarantee at most one). [S1 breakdown F-606; S2 gate v0.6.0]
- **AC-2** For a same-volume rename in doubt, the reconciler determines from disk whether the rename happened (target exists, source gone) or did not (source exists, target absent) and writes the correct terminal journal row. [S1 breakdown F-606]
- **AC-3** For a cross-volume copy+verify+delete in doubt, the reconciler distinguishes the phase reached by a target-size check and repairs the journal to a coherent terminal state without data loss. [S1 breakdown F-606, F-601]
- **AC-4** Kill between `intent` and act (operation never started): reconciliation restores the pre-operation state and the job is resumable from that operation. Proven by an automated kill-injection test. [decision-ledger AC additions; S2 gate]
- **AC-5** Kill between act and `done` (operation completed but unrecorded): reconciliation recognizes the completed operation, records `done`, and resumes from the next operation. Proven by an automated kill-injection test. [decision-ledger AC additions; S2 gate]
- **AC-6** After reconciliation, the human is offered a resume-or-rollback choice rendered through the FD-04 (F-908 states) surface in plain language, with no raw path or journal shown as the primary content (paths behind "Show file details" per FD-13). [decision-ledger FD-04, FD-13]
- **AC-7** Choosing rollback runs the inverse plan through the same F-603 (rollback) pipeline; choosing resume continues the original job from the reconciled point. Neither path bypasses validation. [S1 breakdown F-606, F-603] **Amended by FD-39 (carry-on by re-planning):** carrying on is satisfied by scanning and planning again rather than by replaying the interrupted job, because a replayed plan is validated against a snapshot the interrupted run itself invalidated. The rollback half is unchanged. See `design-p1c-interruption-surface.md`.
- **AC-8** Cancellation of an apply job takes effect only between operations (never mid-file-move); a cancelled apply leaves a coherent, resumable state, verified by an automated test and by a hand walkthrough. [decision-ledger AC additions; S2 gate; F-104 semantics]
- **AC-9** On an access-denied error during an operation, the executor retries once, then halts the affected campaign group with an `AppError` carrying the FD-19 remediation, leaving the journal coherent (retry-once-then-halt-group). [decision-ledger FD-19]

### F-702 (hash verification) - P1

- **AC-10** Hashing computes BLAKE3 over candidate members only; a full-snapshot or scan-time hash-everything path does not exist. [S1 breakdown F-702; discovery anti-pattern]
- **AC-11** Hash verification runs as a background job emitting `job:progress` events (items done, total, current file) and is cancellable at safe boundaries (F-104 semantics). [S1 breakdown F-702, F-104]
- **AC-12** A set-aside (quarantine) action on a duplicate group is permitted only when every member has a verified hash, OR when the user supplies an explicit override confirmed through a warning dialog that states the copies were not content-verified. [decision-ledger scope; planning audit stream 2 item 15]
- **AC-13** The warning-confirm override is a deliberate two-step affordance (not a default), consistent with the design-system danger token pair (FD-09), and its copy uses plain language ("Archive," never "quarantine"/"delete" as primary vocabulary; FD-10 register). Amended 2026-08-06: FD-42 renamed the user-facing term from "set aside" to "Archive"; the internal engineering term "quarantine" is unchanged and remains internal-only. [FD-09, FD-10, FD-42; PRODUCT.md]
- **AC-14** Two files with identical basename and size but different content produce distinct hashes and are NOT auto-resolved into one keep/set-aside decision (version candidates, not exact duplicates). [S1 breakdown F-701/F-702]
- **AC-15** Hash results persist on `duplicate_members` (hash state) so a re-open of the duplicates surface does not re-hash already-verified members. [S1 schema duplicate_groups/members]
- **AC-16** If hash throughput is unacceptable on real data, the release still ships: the campaign runs dedupe as flag-only and set-aside-by-hash becomes post-campaign work (descope trigger, see Release Gate). [decision-ledger AC additions; S2 Section 5]

### F-703 (duplicate review + report) - P1

- **AC-17** Duplicate candidates are presented grouped: each GROUP is one book with N copies; the surface never conflates groups with copies or "pairs" (FD-08 canon). [decision-ledger FD-08; planning audit stream 2 item 8]
- **AC-18** Every count shown - nav badge, group headline, report totals - counts GROUPS; member files are labeled "copies"; any GB figure states which quantity it refers to. [decision-ledger FD-08]
- **AC-19** Review is group-by-group: within a group the user sees each copy with its location behind "Show file details" (FD-13) and the proposed keeper. [S1 breakdown F-703; FD-13]
- **AC-20** The duplicates report exports as CSV (one row per copy, group key column) and its language counts groups, matching the surface exactly. [S1 breakdown F-703; FD-08]
- **AC-21** The report and surface use the FD-10 deletion-guarantee register: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone." Primary vocabulary for a resolved copy is "moved to the Archive," not "deleted." Amended 2026-08-06 by FD-42, which retired "set aside." [decision-ledger FD-10, FD-42]
- **AC-22** Any sample numbers used in mockups or docs are labeled sample data and are not hardcoded into the surface; real counts derive from the scan (FD-27). [decision-ledger FD-27]

### F-704 (resolution policies) - P1

- **AC-23** Three policies are selectable per duplicates run: keep-larger, keep-m4b, and flag-only; the default is flag-only. Amended 2026-08-06: keep-higher-bitrate was cut as F-1108, NOT for lack of a reader (`lofty` ships and already reads these files) but because file size is a free proxy for it that cannot be missing, and because it has no defined value for a book split across N files. [S1 breakdown F-704; S2 gate; F-1108]
- **AC-24** A non-flag-only policy proposes a keeper per group; the user must confirm before any set-aside operation is generated (no silent auto-resolution). [S1 breakdown F-703/F-704]
- **AC-25** Confirmed resolutions emit set-aside operations into the normal plan under the user-facing "Duplicates" campaign group (FD-26 (seven campaign groups), renamed by FD-46); `dedupe-quarantine` is only the internal F-403 plan-pass id and never appears as a UI or report label. Dedupe is not a special executor path, it is a campaign group through F-403 (plan builder) / F-404 (plan validation) / F-601 (executor). [S1 breakdown F-704, F-403; FD-26]
- **AC-26** flag-only produces no set-aside operations: it records the group and keeper suggestion for later review and leaves every copy in place. [S1 breakdown F-704]
- **AC-27** Losers set aside by a resolution round-trip through the standard journal/manifest, so a rollback restores every set-aside copy to its original path (part of the dedupe end-to-end gate). [decision-ledger AC additions; S1 F-603/F-605]

### F-905 (duplicates surface) - P1

- **AC-28** The duplicates surface hosts the F-703 group-by-group review and the F-704 policy selector as one screen, following the design-system register (no "dedupe"/"operations" vocabulary on the primary surface). [S1 breakdown F-905; PRODUCT.md principle 1]
- **AC-29** The navigation badge shows the GROUP count and updates when a scan or hash job completes (event-driven, no polling). [S1 events; FD-08]
- **AC-30** The surface exposes the F-702 override as an explicit warning-confirm affordance and the FD-13 "Show file details" disclosure for copy locations. [AC-12, FD-13]
- **AC-31** The surface renders correct empty and loading states from the FD-04 catalog: "no duplicates found" empty state, and a distinct "checking copies" loading state during a hash job. [decision-ledger FD-04]

### F-802 (ruleset import/export) - P1

- **AC-32** A ruleset exports to a portable JSON file carrying its `schema_version`; the file contains templates, structure policies, and cleanup toggles sufficient to reproduce the ruleset on another machine. [S1 breakdown F-802, F-801; schema rulesets]
- **AC-33** Import validates the JSON against the versioned schema; a schema-version mismatch is handled explicitly (accept with additive migration, or reject with a remediation message), never silently misparsed. [S1 breakdown F-802; S1 error taxonomy]
- **AC-34** An imported ruleset is a new row (does not overwrite an existing ruleset of the same name without explicit confirmation), preserving the immutable-plan lineage. [S1 breakdown F-801/F-802]
- **AC-35** Round-trip: export a ruleset, import it on a clean database, generate a plan from the same snapshot, and get a byte-identical plan to the original (ruleset portability proven against the F-403 determinism golden). [S1 breakdown F-403 determinism; F-802]

### F-501 (everything view) - P1 (redefined per FD-06)

- **AC-36** The everything view renders the complete change list virtualized (TanStack Virtual), grouped by campaign group, over the full library scale without UI freeze. [decision-ledger FD-06; S1 breakdown F-501; NFR scale]
- **AC-37** It is a tier-1 disclosure surface: reachable but not the default review path; the default remains the per-group cards from v0.4.0 (D-16). [decision-ledger FD-06, D-16]
- **AC-38** Each row shows source and target behind "Show file details," extended for tier 1 with matched pattern and confidence (FD-13 tier-1 content, F-504). [decision-ledger FD-13; S1 F-504]
- **AC-39** Tree presentation is optional and behind a toggle; its absence never blocks the release (its own descope trigger, see Release Gate). [decision-ledger FD-06; S2 Section 5]

### F-609 (library freshness: scan triggers and the on-entry check) - P1

Added 2026-08-05 from the UI round 2 crit pass. jp: *"This seems like a pretty important feature/function. Perhaps showing a summary popup of changes. spec this and add it to the release plan and sample mockups."*

**The problem.** A plan is built from a stored scan snapshot, not a live read of disk. Starting a run uses the scan you already have, deliberately: silently re-reading 297 GB on a button press would make the app feel broken. But it means the plan can describe a library that no longer exists, and today the interface never says how old the scan is. The app catches the mismatch at apply time and refuses, which is safe but reads as a failure at the worst possible moment.

**The shape.** Scanning is **event-triggered, never watched.** A filesystem watcher running for the life of the app is a background process, a source of bugs, and unnecessary for the actual risk (jp, same crit pass: *"there should just be key natural triggers when a folder is automatically scanned"*).

- **AC-42** The library screen displays the age of the current scan in plain language ("last looked: 6 minutes ago") beside a manual re-scan control, so staleness is visible before it becomes a surprise. [UI round 2 crit; F-902]
- **AC-43** A scan is triggered by exactly these events, and by nothing else: (a) the user asks for one; (b) the user enters the Organize flow, subject to AC-44; (c) an undo completes; (d) a run completes; (e) app start, only when the stored scan is older than a configured threshold. No filesystem watcher exists in any build. [UI round 2 crit; vocabulary per FD-48]
- **AC-44** On entering the Organize flow, the app compares the library against the stored scan using a **cheap** check (directory listing plus modification times, never a full content re-read). If nothing changed, the user is not interrupted and the flow proceeds. [UI round 2 crit; vocabulary per FD-48]
- **AC-45** If the cheap check finds changes, the user is shown a summary before the plan is built, naming what changed by count and kind ("4 books added, 1 removed, 2 renamed"), with two choices: re-scan and rebuild the plan, or proceed with the plan as it stands. Neither choice is preselected as safe; both are legitimate. [UI round 2 crit]
- **AC-46** Proceeding with a stale plan does not bypass the existing apply-time `snapshot-stale` refusal. AC-45 is an earlier and friendlier surface for the same hazard, never a replacement for the safety check. [D-09 safety invariants]

**Implementation note.** The app already has an incremental rescan used by the after-the-fact verification pass; AC-44's cheap check should build on it rather than introduce a second mechanism.

**Descope path.** AC-42 alone (scan age visible, manual re-scan) delivers most of the value and is trivial. AC-43 to AC-46 can move to v0.7.0 without blocking the tag.

### F-610 (open a folder in the OS file manager) - P1

Added 2026-08-05 from the UI round 2 crit pass. jp asked for clickable paths in three separate places, then confirmed: *"I am not trying to surface file and folder manager in this application. I just want to open the OS file manager/explorer to the clicked folder."* And: *"i still think in-line folder references throughout the tidying and review process should open to the folder."* (Quoted verbatim; the vocabulary is superseded by FD-48.)

**Why this needs a feature rather than a link.** `FD-29` gives the web layer no filesystem and no shell access; its capability allowlist is seven permissions and its own description says "no fs, and no shell". Making a path a hyperlink would mean granting shell access to the entire frontend, forever, to serve a convenience.

- **AC-47** A backend command opens the OS file manager at a given path. The frontend never opens anything itself; it asks. The capability allowlist is unchanged. [FD-29]
- **AC-48** The command **refuses any path not inside the library root or the set-aside root**, and refuses rather than silently doing nothing. The check exists because the path arrives from the untrusted half: without it, the command is a general "open any path on this machine" primitive reachable from the web layer. [FD-29]
- **AC-49** Inline folder affordances appear throughout the Organize and review surfaces wherever a path is displayed, not only in the sidebar. [UI round 2 crit, jp explicit; vocabulary per FD-48]
- **AC-50** The sidebar carries two permanent quick links, to the library root and to the set-aside root, so the action does not require finding a row that happens to show a path. [UI round 2 crit]

### F-1110 (book-level duplicate comparison) - P1

Added 2026-08-05 per `FD-44`, from the duplicates approach audit. Every criterion in `F-702` to `F-905` assumes **one book is one file**. A book split across twelve mp3s is twelve unrelated files to the detector, so two copies of it are never grouped, and one m4b versus a folder of mp3s is never compared either.

- **AC-51** A book FOLDER carries a fingerprint derived from data the scan already holds: count of audio files, total bytes across them, and the normalised title. No new scan pass and no filesystem read is required to compute it. A book folder is one the classifier calls `book` and that has no `book` ancestor; the second half of that is what keeps a disc-split title one candidate rather than one per disc. [FD-44; duplicates audit]
- **AC-52** Two folders whose fingerprints match are duplicate CANDIDATES, in exactly the sense single-file candidates already are: recorded, counted, never acted on. Counted means counted where a user can see it: the Copies card and the report count them alongside exact single-file groups, which is the silent under-reporting this feature exists to close. [FD-08 group canon]
- **AC-53** A structural match tier compares the AUDIO file sizes across two candidate folders as a **sorted** multiset: each folder's sizes are sorted and the two sequences compared elementwise. A twelve-part book copied twice matches on twelve sizes; two different books do not. This tier reads no file contents. [duplicates audit]

  **Settled 2026-08-14 by jp.** This criterion previously read "the ordered multiset of file sizes", which is self-contradictory: a multiset has no order. The ordering it implied was directory iteration order, which is not stable across two copies of the same book, so a positional comparison reports false differences on folders that are genuinely identical. Sorting makes the comparison canonical. Audio sizes rather than all file sizes, because `AC-51` already makes audio the unit and a stray sidecar should not fail a structural match.
- **AC-54** A content match tier hashes using `F-702`, on request only, never as part of detection. Two folders match when the **sorted multiset of their audio files' hashes** agrees, for the same reason `AC-53` sorts sizes: the order files come back in is not a property of the book. "Pairwise" and "all members agree" are the same statement here, because hash equality is an equivalence relation. "Never as part of detection" holds by construction rather than by discipline: detection is pure and has no content source in scope. [AC-10 candidates-only rule]
- **AC-55** A single-file copy and a multi-file copy of the same title group together but **never auto-resolve**: they fail structural matching by construction, so the surface presents them as needing a human decision. Choosing between one file and twelve is a preference, not a mechanical ranking. [FD-44]

**Ordering (FD-44).** After `P2`, because AC-54 is `P2`'s hashing applied to a set. Before `P3`, because `keep-m4b` means "prefer the .m4b" against files and "prefer the copy that is one file over the copy that is twelve" against books, and writing `P3` first means writing it twice.

**Descope path.** `AC-51` and `AC-52` alone (find and count multi-file duplicate candidates, resolve none) still remove the silent under-reporting, which is the worst property of shipping without this.

### Long-path battle testing (FD-19) - release gate item

- **AC-40** The full pipeline (scan, plan, validate, dry-run apply, rollback) runs green over runtime-generated fixture paths beyond 260 characters using extended-length (`\\?\`) semantics; these fixtures are generated at test time and never committed. [decision-ledger FD-19; S2 v0.6.0 scope; S2 CI notes]
- **AC-41** When a target path would exceed the limit and `LongPathsEnabled=0` is detected, the app warns with a linked how-to rather than failing obscurely; near-260 warnings are retained for interop. [decision-ledger FD-19]

## Behavior / Examples

- Kill-during-apply, window A (intent then kill): the journal has `intent(op=42, rename A->B)` with no terminal row. On restart the reconciler finds A present and B absent, concludes the rename never ran, writes nothing terminal for op 42 (or a `failed` marker), and offers resume from op 42. The tier-2 surface says: "The last run was interrupted. Nothing was left half-done. You can pick up where it stopped, or undo what was already done." Paths appear only under "Show file details."
- Kill-during-apply, window B (act then kill): the journal has `intent(op=42)` only, but on disk A is gone and B exists. The reconciler concludes the rename completed, records `done(op=42)`, and resumes from op 43.
- Dedupe end to end on a copy: detect candidates (basename+size groups) -> verify with BLAKE3 -> apply keep-m4b policy -> confirm keepers per group -> losers set aside into `Quarantine\<job-id>\` preserving relative paths -> rollback restores every set-aside copy to its original location, tree byte-identical.
- Everything view: 982 rows (sample data, per FD-27) grouped under seven campaign groups (FD-26), scrolled smoothly via virtualization; opening a row's "Show file details" reveals from/to plus "Matched pattern 4 (year-author-title), confidence high."

## Non-Functional Requirements

- Scale: the everything view and duplicates surface stay responsive at 20,000 files / 1,000 folders; virtualized rendering, no full-list materialization (NFR scale, S1 Section 9).
- Recoverability: kill -9 during apply leaves at most one operation in doubt, auto-reconciled on restart (NFR recoverability; the F-606 signature).
- Privacy: hashing and all dedupe work are local; no network, no telemetry (NFR privacy).
- Accessibility: the duplicates surface, everything view, and resume-or-rollback surface pass the FD-21 verification method (mechanical contrast check of both themes, axe-core smoke in Vitest, keyboard walkthrough in the manual QA checklist); the danger/override affordance uses the FD-09 error/danger token pair, WCAG AA in both Day and Evening. [decision-ledger FD-21, FD-09]
- Determinism: ruleset import/export preserves plan determinism (AC-35).

## Release Gate

The composite checklist that must be green before `v0.6.0` tags (from release plan Section 4 v0.6.0, upgraded per FD dispositions). Evidence pointers follow the conventions in `docs/internal/test-strategy.md`.

- [ ] **Kill-during-apply reconciles, both windows.** Process aborted between journal `intent` and act, and again between act and `done`, reconciles correctly on restart in both directions. Evidence: executor kill/resume reconciliation tests (test-strategy Executor layer). Blocking (P0, F-606). [AC-1..AC-5]
- [ ] **Cancelled apply is coherent and resumable.** Verified by automated test and by hand walkthrough. Blocking (P0, F-606). [AC-8]
- [ ] **Access-denied retry-once-then-halt-group** leaves the journal coherent. Evidence: adversarial executor test. Blocking (P0, F-606, FD-19). [AC-9]
- [ ] **Dedupe flow end to end on a real-data copy:** candidates detected -> hashes verified -> keeper chosen by policy -> losers set aside -> rollback restores them, tree byte-identical. Evidence: manual campaign log + rollback round-trip on a copy. [AC-10..AC-16, AC-23..AC-27]
- [ ] **Set-aside gated on verified hashes or explicit override.** Evidence: F-702 gating tests + surface warning-confirm. [AC-12, AC-13]
- [ ] **Duplicates counted as GROUPS everywhere** (surface, badge, report). Evidence: F-703 tests + report snapshot. [AC-17, AC-18, AC-20]
- [ ] **Ruleset import/export round-trip yields a byte-identical plan.** Evidence: F-802 round-trip test against the F-403 determinism golden. [AC-35]
- [ ] **Everything view responsive at library scale.** Virtualization verified over the real 2026-03-25 baseline (718 folders / 13,970 files, labeled "2026-03-25 baseline, pending fresh scan" per FD-18 (2026-03-25 baselines)) and, separately, against the 20,000-file / 1,000-folder NFR scale target; both may be exercised but are not conflated, and neither freezes. Evidence: frontend responsiveness check + manual QA. Non-blocking descope (see below). [AC-36, AC-39]
- [ ] **Long-path battle test green.** Full pipeline over runtime-generated >260-char fixtures; detect-and-warn UX verified. Evidence: long-path integration suite (test-strategy Executor/Plan layers). [AC-40, AC-41]
- [ ] **Accessibility verified** (FD-21 method) on the three new/changed surfaces in both Day and Evening themes. Evidence: contrast script output, axe-core smoke, keyboard walkthrough item. [NFR accessibility]

Descope triggers for this release (pre-committed, from release plan Section 5):

- Hash performance unacceptable on real data -> campaign runs dedupe as flag-only; set-aside-by-hash becomes post-campaign work. F-702/F-703/F-704 still ship (flag-only path). [AC-16]
- F-501 (everything view) not responsive/stable by end of window -> it slips to a later release without blocking the v0.6.0 tag (P1, tier-1 disclosure, not load-bearing). The tree-presentation toggle is descoped independently and first. [AC-39]
- Any executor invariant test flaky -> the release freezes until deflaked (the one place slippage is accepted rather than descoped, S2 Section 5).

## Source Traceability

| Feature | Priority | Discovery / planning source | D/FD decisions |
|---|---|---|---|
| F-606 (interruption safety + resume) | P0 | breakdown E-06 F-606; release plan Section 4 v0.6.0 | D-09 (safety invariants), FD-04 (resume surface), FD-19 (access-denied semantics) |
| F-702 (hash verification) | P1 | breakdown E-07 F-702; release plan Section 4 v0.6.0 | discovery (candidates-only anti-pattern), planning audit stream 2 item 15 (override) |
| F-703 (duplicate review + report) | P1 | breakdown E-07 F-703 | FD-08 (group canon), FD-10 (guarantee copy), FD-27 (sample data) |
| F-704 (resolution policies) | P1 | breakdown E-07 F-704 | FD-08 (group canon), D-09 (quarantine-only) |
| F-905 (duplicates surface) | P1 | breakdown E-09 F-905 | FD-04 (states), FD-08, FD-09 (danger token), FD-13 (disclosure) |
| F-802 (ruleset import/export) | P1 | breakdown E-08 F-802 | D-01 (locked stack); ~~OQ-2~~ **answered as FD-50, no longer blocking** |
| F-501 (everything view) | P1 | breakdown E-05 F-501 (redefined); release plan Section 4/5 | D-16 (cards + report primary), FD-06 (F-501 redefinition), FD-13 (tier-1 disclosure) |
| Long-path battle testing | gate | release plan Section 4 v0.6.0 | FD-19 (Windows path reality) |

## Revisions

- 2026-07-03: initial spec authored for the planning suite (status review).

## Sources & Evidence

- [S1] Feature-function breakdown (E-06/E-07/E-08/E-05 features, IPC, schema, error taxonomy, NFR): `_local/planning/feature-function-breakdown_2026-07-02.md`. Class A (project design doc).
- [S2] Release plan and CI (v0.6.0 scope, gate, descope triggers, test strategy): `_local/planning/release-plan-and-ci_2026-07-02.md`. Class A.
- [S3] PRODUCT.md (register, tiers, principles, accessibility): `PRODUCT.md`. Class A (design contract).
- [S4] decision ledger and FD dispositions: `docs/internal/decision-ledger.md`. Class A (ratified decisions).
- [S5] Audit findings with dispositions: `docs/internal/planning-audit-2026-07-03.md`. Class A.

## Open Questions

- OQ-1 **CLOSED 2026-08-06, moot.** Was: keep-higher-bitrate needs bitrate available per copy. Cutting the policy as F-1108 removed the question entirely; `keep-larger` captures the same preference using a number that cannot be missing. No longer blocks F-704. [F-1108]
- ~~OQ-2 Ruleset schema-version mismatch on import~~ **ANSWERED 2026-08-22 as `FD-50`: reject with a remediation message naming both versions; no automatic migration before v1.** Two corrections to the question as it was posed. The "additive-only after v1" rule it cited governs the SQLite schema, not the ruleset JSON, so that analogy was doing unearned work. And half of it was never a choice: a file NEWER than the app must always be rejected, or `AC-33` (never silently misparsed) and `AC-35` (a round-tripped ruleset reproduces a byte-identical plan) both break silently. The live question was only what to do with an OLDER file, and the answer is reject that too, because export does not exist yet so **no ruleset files exist to migrate**, and speculative conversion code that changes how books get organized would sit unexercised until the day it mattered. Migrate-forward is a v1 commitment; the check is written as a `match` so an arm slots in without restructuring. **`P6` is unblocked.** See the decision ledger.
