---
title: "Audiobook Organizer - Status and Decision Queue"
type: status
project: audiobook-organizer
created: 2026-07-20
updated: 2026-08-04
status: active
---

# Audiobook Organizer: what needs you, and where everything stands

One page. The top section is the only part that needs you. Everything below is context.

## WAITING ON YOU (nothing else is blocked on code)

Every item below leads with what it IS; the reference ID in brackets is just the lookup key. Acceptance-criteria numbers restart per release, so each one names its release.

The fuller version of this list lives in the "What needs you" section of the latest session log under `_local/_session-logs/`, which is untracked on purpose.

- [ ] **Cut the release tags** (v0.1.0 through v0.5.0). Five releases are built and merged; zero are tagged. Human-only. Runbook: [runbook_cut-tag-release.md](docs/internal/release-plans/runbook_cut-tag-release.md)
- [ ] **Round-trip evidence: accept or extend** (`AC-17`, v0.5.0 acting). Byte-identical SHA-256 before and after is proven on a 3-book subset of two real folders. Say "good enough" or ask for a fuller run. Scratch copies at `E:	mpbo-rt\` (~1.5 GB) can be deleted once you decide. Detail: [v0.5.0 spec, AC-17](docs/internal/releases/v0.5.0-acting/spec.md)
- [ ] **Cancel-mid-tidy-up hand walkthrough** (`AC-8`, v0.6.0 hardening). Cancel a run in the app and confirm it stops between operations, never mid-file-move, and stays resumable. The automated half passes; this is the by-hand half. Last open P1 item besides the resume surface. Detail: [v0.6.0 spec, AC-8](docs/internal/releases/v0.6.0-hardening/spec.md)
- [ ] **Approve a plan in the app** (`G-1`, the v0.4.0 seeing gate). The installer you have predates the v0.5.0 apply surface; a fresh build can be made so you walk the whole flow. Detail: [v0.4.0 spec](docs/internal/releases/v0.4.0-seeing/spec.md)
- [ ] **A non-engineer reads the dry-run report** (`G-6`, the v0.3.0 planning gate) and confirms it makes sense without help. Detail: [v0.3.0 spec](docs/internal/releases/v0.3.0-planning/spec.md)
- [x] **Crit the UI mockups** - DONE 2026-08-04, captured in `_local/gui/2026-07-22/feedback.md` (11 of 13 files). Round 2 is being built from it in `_local/gui/2026-08-04/`; the collaboration surface is that folder's `MASTER.md`. Two things still need you there: one truncated feedback line on `history.html`, and the scope call on per-action opt-out.
- [ ] **Power-loss durability: decide the threat model** (from the [2026-07-30 audit](_local/audit/2026-07-30_audit_codex-56.md)). Is the promise process-kill recovery only, or power loss too? The journal runs `synchronous = NORMAL`, which survives a process kill (now proven by the kill tests) but is not proven to survive a power cut before the write reaches the platter. Recommendation: include power loss for real changes, using a durable barrier on the journal connection only. **Blocks any real-library run.**
- [ ] **Cross-volume move policy: decide** (from the same audit). A move between drives copies, compares byte length, then deletes the source. Equal length is not equal content. Recommendation: prohibit cross-volume real moves until content hashing lands in P2 (hash verification). **Blocks any real-library run**, and is now a public promise in [SECURITY.md](SECURITY.md).
- [ ] **Veto window: journal durability and set-aside placement** (`FD-33`, `FD-34`, from v0.5.0). Both stand unless you object: the journal durability boundary, and set-aside living beside the library rather than inside it. Detail: [decision ledger](docs/internal/decision-ledger.md)
- [ ] **Veto window: History scope, mode-gated recovery, public flip** (`FD-36`, `FD-37`, `FD-38`, from this session). History and undo pulled into v0.6.0; startup recovery gated on the recorded run mode and failing closed; the early public flip via a fresh repo with MIT ratified. Detail: [decision ledger](docs/internal/decision-ledger.md)
- [ ] **Answer two open questions that block later v0.6.0 phases.** `OQ-1` (where bitrate comes from) blocks the keep-higher-bitrate policy in P3; `OQ-2` (ruleset schema-version mismatch handling) blocks ruleset import/export in P6. Detail: [v0.6.0 spec](docs/internal/releases/v0.6.0-hardening/spec.md)

**This repo is now PUBLIC** (FD-38, executed 2026-07-31). It is a NEW repository built from verified-clean history rather than a visibility change, because the original could not safely be flipped. That original was renamed aside and **deleted on 2026-08-02**, which removed the last server-side copy of the refs that made flipping unsafe. A full local backup of it, including its pull request record, is kept under gitignored `_local/backups/`. Verified at publication: 0 pull refs on this repo carrying anything from before it, and 0 name-bearing blobs across all published commits.

Licence is now MIT (ratified with FD-38); the previous LICENSE file carried a "not a final license grant" header that would have published a licence disclaiming itself.

## Release ladder at a glance

| Release | Codename | Built | Merged to main | Tagged | Your open gate |
|---|---|---|---|---|---|
| v0.1.0 | spine | yes | yes | no | tag |
| v0.2.0 | understanding | yes | yes | no | tag |
| v0.3.0 | planning | yes | yes | no | tag, G-6 |
| v0.4.0 | seeing | yes | yes | no | tag, G-1 |
| v0.5.0 | acting | yes | yes | no | tag, AC-17, FD-33/34 |
| v0.6.0 | hardening | in progress | P0/P1 yes | no | P1c round-2 crit, AC-8 walk |
| M-1 | campaign | planned | - | - | - |
| v0.9.0 | packaged | planned | - | - | installer, signing decision |

## In flight right now

**UI round 2** in `_local/gui/2026-08-04/`, built from your 2026-08-04 crit. Prototypes only, nothing in the tracked tree, no code changes. It exists to settle the `P1c` resume-or-rollback surface and the tidy-up interaction model before either gets built.

Everything below merged to `main` on 2026-07-31 (PRs #1 to #4), all four checks green on each.

Landed in v0.6.0 so far:

- **P1a/P1b (interruption safety, F-606)** - the reconciler: after a kill, find the single in-doubt operation, verify what actually happened on disk, repair the journal, and report what can be done next.
- **Mode-aware reconciliation (FD-37)** - the startup sweep was probing the REAL library to classify jobs that may have been rehearsals. Because the UI pins dry-run, every stranded job in practice WAS a rehearsal. Now gated on `jobs.mode`, failing closed on an unreadable mode and on more than one stranded run.
- **P0: History and undo (FD-36)** - the screen, its engine read model, and the wiring that makes v0.5.0's undo machinery reachable at last.
- **Kill-process recovery tests** - a real binary that runs a real apply and then calls `abort()` mid-operation, so recovery is proven against a genuinely killed process rather than a hand-built journal state. Covers AC-4 (intent-then-kill) and AC-5 (act-then-kill).
- **Four review-found defects fixed**, including probes that turned I/O failures into evidence of completion, and fail-closed paths that erased their own retry condition.
- Three dependency groups updated. TypeScript is held at 6.x because typescript-eslint 8.x refuses to run against TS 7 and takes the whole lint gate down; `.github/dependabot.yml` records the constraint and its removal condition.

Still open on v0.6.0: **P1c** (resume-or-rollback surface, awaiting your mockup crit) and the **AC-8 hand walkthrough**, then P2 hash verification, P3 policies, P4-P7 surfaces, P8 long-path and gate. OQ-1 (bitrate source) blocks P3 keep-higher-bitrate; OQ-2 (ruleset schema mismatch) blocks P6.

## Scope change on v0.6.0 (decision recorded, vetoable)

**History and undo were pulled into the interruption-safety milestone.** The 2026-07-30 audit found that v0.5.0's undo engine was complete but unreachable: the History route was a placeholder and nothing called either rollback command. Recovering a journal correctly and then offering the user nowhere to act on it is not a finished safety story, so the two ship together. This grows v0.6.0; the alternative was keeping real changes disabled for longer, which was going to happen anyway.

## What the app can and cannot do (kept honest)

Real changes are still not reachable from the UI, by design. The engine can execute against the real filesystem; the frontend pins every run to rehearsal, and it stays that way until all four of these close:

1. **Power-loss threat model decided** (`FD-33`, journal durability boundary)
2. **Cross-volume move policy decided** (content verification before a cross-volume real move)
3. **A mechanical authorization boundary for real applies** (today the frontend pins dry-run but the command still accepts either mode, so the gate is procedural)
4. **Forward tidying blocked while an interruption is unresolved** (added 2026-08-04 by the P1c interruption-surface design; the engine pattern already exists as `ensure_forward_tidying_allowed`)

The README states this plainly rather than implying a finished write path.

## Cleanup when convenient

- `E:\tmp\abo-rt\` (~1.5 GB of AC-17 round-trip copies) once AC-17 is ratified. The only one still outstanding.
- `E:\tmp\abo-tracer`, left over from the v0.1.0 tracer-bullet work and almost certainly dead weight.

Done 2026-08-02: the four `E:\tmp\abo-*.git` mirrors and the FD-35 scrub input were deleted after their contents were checked against the backup, and the stale local branches were pruned. The one surviving copy of the pre-rewrite history is `_local/backups/archived-repo/`.

## Where the real detail lives

- **Decisions:** `docs/internal/decision-ledger.md` (D-nn = yours; FD-nn = orchestrator, vetoable)
- **Release map and gates:** `docs/internal/program-roadmap.md`
- **Governance and human-only gates:** `EXECUTION.md` (Section 3 is the stop-and-hand-off list)
- **Full external audit:** `_local/audit/2026-07-30_audit_codex-56.md` (Codex 5.6, deep dive; its decision queue is folded into the top section above)
- **Fresh-public-repo procedure:** `_local/runbooks/fresh-public-repo-runbook.md`

---
*Keep this current by refreshing it whenever a gate clears or a decision lands (a natural step to fold into `/jp-wrap-session`). Format is markdown + YAML frontmatter on purpose: it stays greppable, diff-able, and portable into a Layer 2 tool later.*
