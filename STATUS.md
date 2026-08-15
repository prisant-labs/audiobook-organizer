---
title: "Audiobook Organizer - Status and Decision Queue"
type: status
project: audiobook-organizer
created: 2026-07-20
updated: 2026-08-14
status: active
---

# Audiobook Organizer: what needs you, and where everything stands

One page. The top section is the only part that needs you. Everything below is context.

## WAITING ON YOU (nothing else is blocked on code)

Every item below leads with what it IS; the reference ID in brackets is just the lookup key. Acceptance-criteria numbers restart per release, so each one names its release.

The fuller version of this list lives in the "What needs you" section of the latest session log under `_local/_session-logs/`, which is untracked on purpose.

- [ ] **Merge the open pull requests**, or say what to change. None can be self-merged: the repo is public, which lapsed `D-11`'s private-repo allowance (`EXECUTION.md` Section 6.2). They are listed with what each does in "In flight right now" below. [#25](https://github.com/prisant-labs/audiobook-organizer/pull/25) (arbitrary-value ratchet) is independent and mergeable any time. [#26](https://github.com/prisant-labs/audiobook-organizer/pull/26) (component gallery) and [#27](https://github.com/prisant-labs/audiobook-organizer/pull/27) (`FD-48`, the organize verb) can go in either order; whichever rebases second needs a known two-line fix in `src/gallery/Gallery.tsx`, verified by merging them locally.
- [ ] **Reference screenshots: three to five, one line each on what to steal.** Drop them in `_local/gui/references/`. `D-06` (anti-reference: "looks AI-generated") records what you hate; nothing on file records what you want, so every design decision is currently argued from the negative. This is the only open item nobody but you can do, and the only one that raises the ceiling rather than closing a gap.
- [ ] **Cut the release tags** (v0.1.0 through v0.5.0). Five releases are built and merged; zero are tagged. Human-only. Runbook: [runbook_cut-tag-release.md](docs/internal/release-plans/runbook_cut-tag-release.md)
- [ ] **Round-trip evidence: accept or extend** (`AC-17`, v0.5.0 acting). Byte-identical SHA-256 before and after is proven on a 3-book subset of two real folders. Say "good enough" or ask for a fuller run. Scratch copies at `E:\tmp\abo-rt\` (~1.5 GB) can be deleted once you decide. Detail: [v0.5.0 spec, AC-17](docs/internal/releases/v0.5.0-acting/spec.md)
- [ ] **Interrupted-run hand walkthrough** (`AC-6`, v0.6.0 hardening, the resume-or-rollback offer). About five minutes. Start a practice run, kill the app from Task Manager, reopen it. Confirm the recovery notice appears with the sidebar still navigable, that it says nothing in your library was touched, and that every control is reachable by Tab alone. Checklist item 1 in [v0.6.0-manual-qa.md](docs/internal/qa/v0.6.0-manual-qa.md)
- [ ] **Cancel-mid-run hand walkthrough** (`AC-8`, v0.6.0 hardening, the cancel walkthrough). Cancel a run in the app and confirm it stops between changes, never mid-file-move, and stays resumable. The automated half passes; this is the by-hand half. Together with `AC-6` above, these are the last two open `P1` items: the resume surface itself merged as PR #11. Checklist item 2 in [v0.6.0-manual-qa.md](docs/internal/qa/v0.6.0-manual-qa.md)
- [ ] **Approve a plan in the app** (`G-1`, the v0.4.0 seeing gate). The installer you have predates the v0.5.0 apply surface; a fresh build can be made so you walk the whole flow. Detail: [v0.4.0 spec](docs/internal/releases/v0.4.0-seeing/spec.md)
- [ ] **A non-engineer reads the dry-run report** (`G-6`, the v0.3.0 planning gate) and confirms it makes sense without help. Detail: [v0.3.0 spec](docs/internal/releases/v0.3.0-planning/spec.md)
- [x] **Crit the UI round 2 prototypes** - DONE 2026-08-05, with your notes in `_local/gui/2026-08-04/round2/feedback_round2.md`. It produced seven decisions, `FD-39` to `FD-45`. The prototypes themselves are now superseded by the component gallery in PR #26, which renders the app's real components rather than a parallel copy of them; do not build from the round 2 HTML.
- [x] **Crit the UI mockups** - DONE. Round 1 closed 2026-08-05 with a full closing record in `_local/gui/2026-08-04/feedback_round1.md`. Four decisions closed there (duplicates stays in v1, keep-higher-bitrate cut, "practice run", `reveal_in_folder`). **The four follow-ups `D-1` to `D-4` are all now closed too**, on 2026-08-05: `D-1` (the name of the action) as `FD-43` and then reversed as `FD-48`, `D-2` (book-level duplicate comparison) as `FD-44`, `D-3` (path display depth) as `FD-45`, and `D-4` (the Archive folder name) as `FD-42`.
- [ ] **Power-loss durability: decide the threat model** (from the [2026-07-30 audit](_local/audit/2026-07-30_audit_codex-56.md)). Is the promise process-kill recovery only, or power loss too? The journal runs `synchronous = NORMAL`, which survives a process kill (now proven by the kill tests) but is not proven to survive a power cut before the write reaches the platter. Recommendation: include power loss for real changes, using a durable barrier on the journal connection only. **Blocks any real-library run.**
- [ ] **Cross-volume move policy: decide** (from the same audit). A move between drives copies, compares byte length, then deletes the source. Equal length is not equal content. Recommendation: prohibit cross-volume real moves until content hashing lands in `P2` (hash verification). **`P2`'s hashing engine merged as PR #15 on 2026-08-06**, so this is now a decision to make rather than a capability to wait for. **Blocks any real-library run**, and is now a public promise in [SECURITY.md](SECURITY.md).
- [ ] **Veto window: journal durability and Archive placement** (`FD-33`, `FD-34`, from v0.5.0). Both stand unless you object: the journal durability boundary, and the Archive living beside the library rather than inside it. Detail: [decision ledger](docs/internal/decision-ledger.md)
- [ ] **Veto window: History scope, mode-gated recovery, public flip** (`FD-36`, `FD-37`, `FD-38`, from 2026-07-31). History and undo pulled into v0.6.0; startup recovery gated on the recorded run mode and failing closed; the early public flip via a fresh repo with MIT ratified. Detail: [decision ledger](docs/internal/decision-ledger.md)
- [ ] **Veto window: the seven decisions from the UI round 2 crit** (`FD-39` to `FD-45`, from 2026-08-05). `FD-39` (carry on after an interruption by re-planning from a fresh scan, never replaying), `FD-41` (five scan triggers, never a filesystem watcher), `FD-44` (book-level duplicate comparison in as `P2b`) and `FD-45` (paths display one level) are still vetoable; `FD-40` (non-audio clutter defaults to staying put) and `FD-42` (the Archive rename) were your own calls. **`FD-43` (the name of the action) is closed**: it was flagged in its own entry as the one most worth vetoing, you exercised that on 2026-08-14, and `FD-48` supersedes it. Detail: [decision ledger](docs/internal/decision-ledger.md)
- [x] **`AC-53` (v0.6.0 hardening, folder-level duplicate size comparison) - ANSWERED 2026-08-14.** The spec said "ordered multiset", which is self-contradictory; you settled it as sort the sizes and compare canonically, because directory iteration order is not stable across two copies of the same book. **It unblocked `P2b`**, which is now implemented in full and waiting for review as PR #29. The corrected spec wording travels with it.
- [ ] **Answer one open question that blocks a later v0.6.0 phase.** `OQ-2` (ruleset schema-version mismatch handling) blocks ruleset import/export in `P6`. `OQ-1` (where per-copy bitrate comes from) **is now moot**: cutting the keep-higher-bitrate policy as `F-1108` on 2026-08-05 closed it, because `keep-larger` already captures the same preference using a number that cannot be missing. Detail: [v0.6.0 spec](docs/internal/releases/v0.6.0-hardening/spec.md)

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
| v0.6.0 | hardening | in progress | `P0`/`P1` **complete**, `P2` engine merged | no | `AC-6` walk, `AC-8` walk |
| M-1 | campaign | planned | - | - | - |
| v0.9.0 | packaged | planned | - | - | installer, signing decision |

## In flight right now

`main` is at `62684bf`. **Every pull request below is open and unmerged**, because merging is a human decision now that the repo is public. They are the first item in the queue above.

| PR | Branch | What it does |
|---|---|---|
| [#25](https://github.com/prisant-labs/audiobook-organizer/pull/25) | `chore/arbitrary-value-ratchet` | Freezes the app's Tailwind arbitrary-value sprawl at its measured count (285 uses of 71 distinct values) and fails CI in **both** directions, so removing values cannot silently bank slack for new ones |
| [#26](https://github.com/prisant-labs/audiobook-organizer/pull/26) | `feat/component-gallery` | A dev-only gallery of 29 specimens rendering the real components in both themes at once. It cannot ship: `pnpm build` emits no trace of it |
| [#27](https://github.com/prisant-labs/audiobook-organizer/pull/27) | `feat/fd-48-organize-verb` | `FD-48` end to end: the action is "organize" and the noun is retired rather than replaced. Also carries the ledger entry, so `FD-48` is only citable in the tree once this merges |
| [#28](https://github.com/prisant-labs/audiobook-organizer/pull/28) | `docs/status-and-changelog-refresh` | This page and `CHANGELOG.md`, both of which drift silently because the CI vocabulary gate covers three governance files and neither of them |
| [#29](https://github.com/prisant-labs/audiobook-organizer/pull/29) | `feat/p2b-book-level-duplicates` | `P2b` (`F-1110`, book-level duplicate comparison), all of `AC-51` to `AC-55`. Engine-only, no UI, no IPC change |

**Merged 2026-08-14:** PR #23 and PR #24, the Dependabot Rust and JavaScript dependency groups.

**Merged 2026-08-05 and 2026-08-06:**

- **PR #11** - `P1c`, the interruption recovery surface. **`P1` is now complete**; only the `AC-6` and `AC-8` hand walkthroughs are owed on it.
- **PR #12** - the `docs/internal/backlog/` structure, `F-609` (library freshness), `F-610` (open a folder in the OS file manager), `FD-40` and `FD-41`.
- **PR #13** - `FD-42` (the Archive rename), the duplicates approach audit, and the `FD-40` clutter-default implementation.
- **PR #14** - closes `D-1`, `D-2` and `D-3` as `FD-43` (keep the old name for the action, since superseded by `FD-48`), `FD-44` (book-level duplicate comparison in as `P2b`) and `FD-45` (paths display one level).
- **PR #15** - `P2`'s engine half: BLAKE3 content hashing, its persistence, a cancellable verification job, and the gate that decides whether a duplicate group may be resolved automatically. `AC-13` (the two-step override) and `AC-16` (throughput measured on real data) are the remainder of `P2`.

**Next up:** `P2b` (`F-1110`, book-level duplicate comparison) is implemented and open as PR #29, so the queue moves to the fourth real-apply precondition (block forward organizing while an interruption is unresolved), then the rest of `P2` (`AC-13`, the two-step override, and `AC-16`, throughput on real data), then `FD-42`'s code rename with its migration path, then `P3` policies written against books rather than files.

**UI round 2** lives in `_local/gui/2026-08-04/round2/`. Prototypes only, nothing in the tracked tree, and superseded by the gallery in PR #26. Round 1 is closed with a full traceability record, and its four follow-up decisions `D-1` to `D-4` are closed as `FD-42` through `FD-45`, with `FD-43` since reversed as `FD-48`.

## Scope change on v0.6.0 (decision recorded, vetoable)

**History and undo were pulled into the interruption-safety milestone.** The 2026-07-30 audit found that v0.5.0's undo engine was complete but unreachable: the History route was a placeholder and nothing called either rollback command. Recovering a journal correctly and then offering the user nowhere to act on it is not a finished safety story, so the two ship together. This grows v0.6.0; the alternative was keeping real changes disabled for longer, which was going to happen anyway.

## What the app can and cannot do (kept honest)

Real changes are still not reachable from the UI, by design. The engine can execute against the real filesystem; the frontend pins every run to rehearsal, and it stays that way until all four of these close:

1. **Power-loss threat model decided** (`FD-33`, journal durability boundary)
2. **Cross-volume move policy decided** (content verification before a cross-volume real move)
3. **A mechanical authorization boundary for real applies** (today the frontend pins dry-run but the command still accepts either mode, so the gate is procedural)
4. **Forward organizing blocked while an interruption is unresolved** (added 2026-08-04 by the P1c interruption-surface design; the engine pattern already exists as `ensure_forward_tidying_allowed`, and P1c has since merged without it, so this remains genuinely open. The function name keeps the retired word on purpose: `FD-48` moved the copy a user reads, not engineering identifiers, because renaming those is a migration)

The README states this plainly rather than implying a finished write path.

## Cleanup when convenient

- `E:\tmp\abo-rt\` (~1.5 GB of AC-17 round-trip copies) once AC-17 is ratified. The only one still outstanding.
- `E:\tmp\abo-tracer`, left over from the v0.1.0 tracer-bullet work and almost certainly dead weight.

Done 2026-08-02: the four `E:\tmp\abo-*.git` mirrors and the FD-35 scrub input were deleted after their contents were checked against the backup, and the stale local branches were pruned. The one surviving copy of the pre-rewrite history is `_local/backups/archived-repo/`.

## Where the real detail lives

- **Decisions:** `docs/internal/decision-ledger.md` (`D-nn` = your product decisions; `FD-nn` = decisions taken during a working session, usually an orchestrator disposition you can veto, sometimes your own call recorded on the day. Each entry names which it is)
- **Release map and gates:** `docs/internal/program-roadmap.md`
- **Governance and human-only gates:** `EXECUTION.md` (Section 3 is the stop-and-hand-off list)
- **Full external audit:** `_local/audit/2026-07-30_audit_codex-56.md` (Codex 5.6, deep dive; its decision queue is folded into the top section above)
- **Fresh-public-repo procedure:** `_local/runbooks/fresh-public-repo-runbook.md`

---
*Keep this current by refreshing it whenever a gate clears or a decision lands (a natural step to fold into `/jp-wrap-session`). Format is markdown + YAML frontmatter on purpose: it stays greppable, diff-able, and portable into a Layer 2 tool later.*
