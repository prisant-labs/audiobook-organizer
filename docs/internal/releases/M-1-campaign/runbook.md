---
title: M-1 Campaign Runbook - the real-library reorganization
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (M-1 runbook)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 M-1)
  - _local/planning/audiobook-organizer-strategy-brief_2026-07-02.md (Sections 3, 5)
  - _local/initial-discovery/audio-books-audiobookshelf_codex.md (Recommended Migration Order)
  - PRODUCT.md
---

# M-1 Campaign Runbook - the real-library reorganization

This is the operational protocol for reorganizing the real library at `E:\Books - Audio`. It is not a software spec: it is a sequence of gated steps, human decision points, evidence collection, and abort criteria. Executed with v0.6.x (hardening), or the early mini-campaign subset with v0.5.x (acting) per D-04 (early mini-campaign posture).

The load-bearing rule (D-10, scope of go): every Real (non-dry-run) apply against the actual library is HUMAN-ONLY. The agent prepares plans, reports, and verification analysis; the human presses Apply. Nothing Real runs until the backup posture is chosen and recorded in the decision log (Appendix A), per D-17 (backup posture is user-defined).

Baseline figures below are labeled "2026-03-25 baseline, pending fresh scan" per FD-18 (drift-tolerant baselines); Step 2 replaces them with fresh numbers.

## Roles at a glance

| Activity | Human-only | Agent-preparable |
|---|---|---|
| Backup posture choice (Step 1) | Yes (D-17) | Options table, verification scripts |
| Manual quick wins (Step 0) | Yes (tool-free) | - |
| Fresh scan + plan + reports (Step 2) | Approve | Scan, plan build, F-506 (dry-run HTML report) / F-507 (pack provenance report) exports |
| Per-group Real apply (Step 3) | Yes (D-10) | Dry run, report, verification analysis |
| ABS cutover (Step 4) | Yes | Sampled-shelf verification analysis |
| Post-campaign report (Step 5) | Review | Metrics delta, exports |

## Campaign map (step order and gating)

Each step is gated on the one before it. Read top to bottom; do not run a step until the prior step's gate is recorded in Appendix A.

| Step | What | Who | Gate to proceed |
|---|---|---|---|
| G-M1 | Preconditions | verify | Applicable precondition row green (full or mini) |
| 0 | Manual quick wins | human, tool-free | Staging out of scanned root; ABS interim repoint |
| 1 | Backup decision + Defender pre-check | human-only | Posture recorded in Appendix A; verification passed |
| 2 | Fresh scan, plan, F-506 + F-507 exports | agent, human-approved | Plan re-reviewed; reports exported; read test (if used) |
| 3 | Staged groups a-g, one Real apply each | human-only applies | Every group verified + logged; no red group |
| 4 | ABS cutover | human-only | New library verified on sampled shelf; old retired |
| 5 | Post-campaign report + backup disposal | agent, human-reviewed | Campaign gate held; log complete |

## Preconditions gate (G-M1)

Do not start Step 1 until the applicable row is green.

Full campaign (all groups) - requires v0.6.0 (hardening):
- [ ] v0.5.0 (acting) signature gate green: rollback round-trip byte-identical on fixtures AND on a real-data copy (release plan Section 4).
- [ ] v0.6.0 (hardening) gate green: kill-during-apply reconciles on restart both directions; hash-verified dedupe proven on a real-data copy; cancelled apply leaves a resumable state.
- [ ] Executor invariant tests all non-flaky (release plan Section 5 descope rule: a flaky invariant test freezes the campaign).
- [ ] FD-15 (OSS-landscape check) outcome recorded; FD-19 Defender pre-check understood.

Early mini-campaign (groups (a) loose-root-books and (b) strip-noise only) - D-04 ratified option, reduced preconditions, runs on v0.5.x:
- [ ] v0.5.0 (acting) signature gate green (rollback round-trip on fixtures AND a real-data copy).
- [ ] Groups (a) and (b) are rename-first, same-volume only (D-08): no cross-volume copy, minimal blast radius.
- [ ] Kill-resume (v0.6.0) NOT required, because these groups are the highest-value, lowest-risk per release plan Section 5.
- [ ] Backup posture recorded (Step 1) - required even for the mini-campaign.

Descope hook: if group (c)-(e) tooling is not ready when library pain peaks, run (a)-(b) as the mini-campaign and defer the rest (release plan Section 5).

## Step 0 - manual quick wins (human-executed, tool-free)

From strategy brief Section 5 item 1. Do these by hand before any tool runs, if not already done during the 2026 manual phase.

- [ ] Create the canonical `Library` root under `E:\Books - Audio\`.
- [ ] Move `_sort`, `_process`, `_audiobookshelf`, and raw pack archives OUT of the scanned root (into `Intake` / `Archive - Packs` / `Reports` siblings).
- [ ] Repoint ABS at `Library` only (interim; full cutover is Step 4).
- [ ] Record in Appendix A that Step 0 is complete (date, what moved).

Note: Step 0 is deliberately outside the tool. It removes the worst ABS scan pollution at zero code risk and shrinks the scanned surface before the first fresh scan.

## Step 1 - BACKUP DECISION (human-only, D-17)

Nothing Real runs until one posture below is chosen and written into Appendix A. The M-1 gate stays OPEN until that entry exists.

### 1a. Backup posture decision table

| Posture | Survives drive failure | Cost | Recovery depth | Verification method |
|---|---|---|---|---|
| External-drive copy | Yes (separate physical drive) | Hours of I/O; needs a spare drive with 297 GB+ free | Full: every byte restorable independent of the source drive | Sampled hash comparison: BLAKE3 a random 5% of files on source vs copy, plus all files in the largest pack; zero mismatches required |
| Same-drive copy (sibling folder) | No (single drive failure loses both) | Hours of I/O; consumes ~297 GB of the 1.3 TB free (strategy brief Section 3) | Full logical recovery from accidental moves/renames, not from hardware loss | Sampled hash comparison as above |
| Manifests + quarantine only | No | Near-zero (no copy) | Undo via journal + rollback plan; deleted-audio recovery relies on quarantine, never on a copy | Verify journal completeness per group and that quarantine holds set-aside copies; rollback round-trip already proven in v0.5.0 |

Trade-off summary: external-drive copy is the only posture that survives a drive failure and is the recommended default; same-drive copy protects against logical error only; manifests+quarantine leans entirely on the tool's proven rollback discipline (D-09 safety invariants) and is acceptable for the low-risk mini-campaign but weakest for the full 297 GB campaign.

### 1b. Backup execution and verification

- [ ] Posture chosen and recorded in Appendix A (operator, date, posture, reason).
- [ ] If a copy posture: copy completed; sampled hash comparison run; result recorded (files sampled, mismatches = 0).
- [ ] If manifests+quarantine only: explicit written acknowledgement in Appendix A that no byte-copy exists and recovery depends on journal + quarantine.

### 1c. Defender / Controlled Folder Access pre-check (FD-19)

Mass renames can be blocked by Windows Defender Controlled Folder Access, surfacing as access-denied mid-campaign.

- [ ] Confirm Controlled Folder Access status for `E:\Books - Audio`; if enabled, either allow the app or disable for the campaign window (record which).
- [ ] Confirm `LongPathsEnabled`; if 0, note it - the executor detects and warns when targets exceed 260 chars (FD-19), and deep Hugo nesting plus long titles gets close.
- [ ] Note executor behavior for reference: retry-once-then-halt-group on access-denied (FD-19); a halted group blocks progression (F-604 (post-apply verification) behavior).

## Step 2 - fresh scan and plan (agent-preparable, human-approved)

The 2026-03-25 snapshot is stale by definition (strategy brief open item; FD-18). Regenerate from live.

- [ ] Run a fresh live scan of `E:\Books - Audio\Library` (read-only). Record the drift delta vs the 2026-03-25 baseline as a deliverable (it is the first fresh look since the snapshot).
- [ ] Regenerate the plan against the fresh snapshot with the `abs-author-first` ruleset (D-02 (author-first default)).
- [ ] Re-review the plan (approve / reject / exclude per group).
- [ ] Export F-506 (dry-run HTML report), self-contained, opens with no network access.
- [ ] Export F-507 (pack provenance report) beside the plan (FD-01 (provenance capture)).
- [ ] Non-engineer read test: if a family member will co-review, they read F-506 and can state what would change and what would not (D-03 (family tier sets the UI bar)). Record pass/fail in Appendix A.
- [ ] Confirm the report carries the FD-10 guarantee copy verbatim: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone."

## Step 3 - staged groups, one Real apply per group

Group order mirrors the discovery migration order (codex doc, Recommended Migration Order) and release plan Section 4. Each group runs the same loop. No group starts until the prior group's verification passed and its decision-log entry exists.

### Per-group loop (repeat for every group a-g)

1. [ ] Dry run the group (agent).
2. [ ] Export/refresh the group's F-506 report (agent).
3. [ ] Human confirm: operator reads the report and approves this group (HUMAN-ONLY).
4. [ ] Real apply of this group (HUMAN-ONLY, D-10).
5. [ ] Verification report (agent): post-apply verification (F-604) compares result to plan.
6. [ ] ABS spot-check: sample a few affected books in ABS; authors/series/sequence read correctly.
7. [ ] Decision-log entry in Appendix A (step, date, operator, evidence file, outcome).

Abort criterion (every group): any verification discrepancy - a byte mismatch, an unexpected target, a halted group from access-denied, or an ABS misread - BLOCKS progression to the next group (F-604 behavior). Resolve, re-verify, and record before continuing. Never advance past a red group.

### Group order and per-group notes

| # | Group (canonical, FD-26) | What it does | Baseline (2026-03-25, pending fresh scan) | Strategy | Special gate |
|---|---|---|---|---|---|
| a | loose books (loose-root-books) | Folderize root-level singles into one-folder-per-book | 237 of 238 parse as `Title by Author` (~67.9 GB) | Rename-first, same-volume (D-08) | Safest big win; the mini-campaign's first group |
| b | messy names (strip-noise) | Strip bracket tags, bitrate, size, rank/year prefixes | ~203 bracket-tag folders (170 bitrate, 214 size, 143 rank, 116 year) | Rename-first, same-volume | Idempotent strippers (v0.2.0 proof); mini-campaign second group |
| c | box sets / bundles (split-multi-book) | Split folders holding several complete books | Harry Potter, Chronicles of Narnia, Wings of Fire, Roald Dahl, HBR Must Reads | Rename-first where same-volume | Manual-review items (FD-17: video/course, radio plays) never auto-applied |
| d | flatten packs (flatten-packs) | Move pack books into canonical book folders | Hugo (80.9 GB), Nebula (30.3 GB), Top 100 (18.1 GB), Dune Universe (16.8 GB) | Rename-first; pack shells to quarantine after (FD-01) | PROVENANCE REPORT (F-507) VERIFIED BEFORE THIS GROUP APPLIES (FD-01). Provenance is destroyed at flatten time if not captured first. |
| e | messy names (normalize series/disc) | Normalize series index width and disc folders to ABS form | Verbal Advantage 24 discs; nonconforming disc names | Rename-first | Series-index normalization folds into "messy names" for the UI, distinct internal pass (FD-26) |
| f | copies (copies set aside / dedupe) | Set aside duplicate copies of a book group | Groups pending fresh measure; ~10.08 GB conservative estimate | Quarantine only (never delete audio, D-09) | Hash-verified (BLAKE3) keeper vs copies, OR explicit human override with warning confirm. Unit is the GROUP; members are copies (FD-08) |
| g | empty folders (empty-folder sweep) | Remove empty folder skeletons | ~20 empty folders | rmdir-empty only | The ONLY delete the product performs; audio is never deleted (FD-10) |

Notes:
- All figures above are 2026-03-25 baseline, pending fresh scan (FD-18); Step 2 supplies real targets. Demo/prototype numbers are sample data and are not used here (FD-27).
- Groups (a) and (b) alone constitute the D-04 early mini-campaign on v0.5.x.
- Group (d) special gate is non-negotiable: verify F-507 provenance export exists and is correct BEFORE applying, because flatten destroys source-pack membership (D-14 (provenance in v1), FD-01).
- Group (f) sets copies aside; it never deletes. Overriding a missing hash match requires an explicit warning confirm (planning audit stream 2 finding 15 disposition, docs/internal/planning-audit-2026-07-03.md).

## Step 4 - ABS cutover (human-only)

- [ ] Create a NEW ABS library pointed at the canonical `Library` root fresh. Do not rescan the old library in place - a fresh library avoids ABS's moved-item duplication heuristics (strategy brief Section 3 concern; codex doc).
- [ ] Sampled shelf verification: pick a sample across genres and confirm authors, series, and sequence numbers land in the right ABS fields.
- [ ] Retire the old ABS library entry once the new shelf verifies.
- [ ] Record cutover outcome in Appendix A.

Pre-campaign to-do (timeboxed research spike, flag now): confirm ABS mass-move / rescan behavior on a live library (item duplication after large moves). Timebox 1 hour; record the finding before Step 4. This is the strategy brief's under-examined ABS behavior item.

## Step 5 - post-campaign (agent-preparable, human-reviewed)

- [ ] Final health-metrics delta: noisy names, mixed folders, multi-book folders, loose root books at or near zero (release plan Section 4 gate).
- [ ] Bytes set aside (quarantined) reported.
- [ ] Manual-review export: the remaining manual-review items (FD-17 video/course, radio plays, ambiguous folders) exported as the follow-up work queue.
- [ ] Quarantine review pass: review set-aside copies at least once (release plan gate).
- [ ] Backup disposal decision: if a same-drive copy was used (Step 1), decide whether to delete it to reclaim ~297 GB, or retain. Record the choice.
- [ ] Final decision-log entries complete in Appendix A.

Campaign gate (all must hold): health metrics near zero for the noisy categories; ABS displays authors/series/sequence correctly on sampled books; quarantine reviewed once; every group has a decision-log entry with outcome = passed.

## Appendix A - evidence and decision log

Fill one row per gated action during execution. Evidence file = path to the report, hash log, or screenshot that proves the outcome.

| Step | Date | Operator | Decision / action | Evidence file | Outcome |
|---|---|---|---|---|---|
| 1 backup posture | | | (posture + reason) | | |
| 1b backup verify | | | | | |
| 1c Defender pre-check | | | | | |
| 0 quick wins | | | | | |
| 2 fresh scan + reports | | | | | |
| 2 non-engineer read test | | | | | |
| 3a loose books | | | | | |
| 3b messy names (noise) | | | | | |
| 3c box sets / bundles | | | | | |
| 3d flatten packs (provenance verified first) | | | | | |
| 3e normalize series/disc | | | | | |
| 3f copies set aside | | | | | |
| 3g empty-folder sweep | | | | | |
| 4 ABS cutover | | | | | |
| 5 post-campaign report | | | | | |
| 5 backup disposal | | | | | |

## Appendix B - abort and rollback quick reference

- Any group verification discrepancy: STOP. Do not advance. Roll back the group as a plan through the same validate/preview/apply pipeline (D-09), re-verify, record.
- Access-denied mid-group: executor retries once, then halts the group (FD-19). Resolve Defender / permissions (Step 1c), re-run the group.
- Over-length path warning: expected on deep packs if `LongPathsEnabled=0`; follow the linked how-to or shorten the target; do not force (FD-19).
- Kill / crash during apply: on restart the executor reconciles the in-doubt journal entry and offers resume-or-rollback (v0.6.0 F-606 (interruption safety + resume)). Full campaign requires this proven; the mini-campaign does not use it.
- Guarantee, always true: no audiobook is ever deleted; only empty folders are removed; every change can be undone (FD-10, D-09).
