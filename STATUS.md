---
title: "Audiobook Organizer - Status and Decision Queue"
type: status
project: audiobook-organizer
created: 2026-07-20
updated: 2026-08-15
status: active
---

# Audiobook Organizer: what needs you, and where everything stands

One page. The top section is the only part that needs you. Everything below is context.

## WAITING ON YOU (nothing else is blocked on code)

Every item below leads with what it IS; the reference ID in brackets is just the lookup key. Acceptance-criteria numbers restart per release, so each one names its release.

The fuller version of this list lives in the "What needs you" section of the latest session log under `_local/_session-logs/`, which is untracked on purpose.

- [x] **Merge the open pull requests** - DONE 2026-08-15, on your instruction, **twice**. First batch of six (`#25`, `#26`, `#27`, `#28`, `#29`, `#30`); second batch of four (`#31`, `#32`, `#33`, `#34`) on "merge and continue based on your best recommendations". `EXECUTION.md` Section 6.2 requires a human decision for any merge to `main`, and each instruction was that decision, recorded here rather than only in a session log. Neither authorisation carries forward to a later batch. Tags and releases were not touched: those are on the Section 3 human-only list.
- [x] **`AC-16` (v0.6.0 hardening, hash throughput on real data): do NOT descope duplicate hashing - ACCEPTED 2026-08-15**, as part of "continue based on your best recommendations". `F-702` ships as designed. The measurement behind it: your library holds 293 exact duplicate candidates totalling 14.96 GB, 5% of its 298.72 GB, because `AC-10`'s candidates-only rule already did the narrowing; the hashing code runs at 2,765 MB/s while the drive delivers 42 to 80, so **the wait is the disk, not the software**, and flag-only would not have saved a second of it. Evidence, with its limits stated: [hash-throughput-2026-08-15.md](docs/internal/releases/v0.6.0-hardening/hash-throughput-2026-08-15.md)
- [ ] **Decide how a REAL apply is authorised** (precondition 3 of four, the last one that is not already closed or already recommended). Recorded as "code, not a decision", and it is not: `apply_start` takes the run mode as a parameter from whoever calls it, and the frontend only ever sends practice. The boundary could be a build that cannot do a real run at all, a setting that defaults off, or a one-shot permission issued separately. Those are different safety postures for the thing standing between this app and 299 GB of your books, which is why it is yours rather than mine. Recommendation: a setting that defaults off and cannot be turned on from inside the review flow, so enabling it is a separate deliberate act rather than one more click on the path you are already walking.
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
| v0.6.0 | hardening | in progress | `P0`/`P1` **complete**, **`P2` complete**, **`P2b` complete**, `P3` steps 1-2 done | no | `AC-6` walk, `AC-8` walk |
| M-1 | campaign | planned | - | - | - |
| v0.9.0 | packaged | planned | - | - | installer, signing decision |

## In flight right now

`main` is at `2f92378`, CI green, **zero open pull requests**. Ten landed on 2026-08-15 in two authorised batches: six in the morning, then four more (`#31`, `#32`, `#33`, `#34`) recorded in the second table below.

| PR | Landed as | What |
|---|---|---|
| [#25](https://github.com/prisant-labs/audiobook-organizer/pull/25) | `5b1e867` | Arbitrary-value ratchet: freezes the app's Tailwind sprawl at 285 uses of 71 distinct values and fails CI in **both** directions, so removing values cannot silently bank slack for new ones |
| [#27](https://github.com/prisant-labs/audiobook-organizer/pull/27) | `d9a312a` | `FD-48` end to end: the action is "organize" and the noun is retired rather than replaced |
| [#26](https://github.com/prisant-labs/audiobook-organizer/pull/26) | `2c2b524` | Component gallery, 29 specimens of the real components in both themes. Dev-only: `pnpm build` emits no trace of it |
| [#29](https://github.com/prisant-labs/audiobook-organizer/pull/29) | `ccc13e0` | **`P2b`** (`F-1110`, book-level duplicate comparison), all of `AC-51` to `AC-55`. Engine-only |
| [#30](https://github.com/prisant-labs/audiobook-organizer/pull/30) | `ff39a27` | The fourth real-apply precondition: refuse a forward run while the last one could not be accounted for |

**The one manual fix**, in `#26`: the gallery predated `FD-48` and referenced two renamed identifiers, plus a third line the recorded note had missed - a specimen label reading "tidy-up active", which is free text and so invisible to the type checker. Nothing sweeps `src/gallery` for retired vocabulary; that gap is now the top item under "Cleanup when convenient".

### Merged 2026-08-15, second batch

Authorised by "merge and continue based on your best recommendations". Merged in an order chosen so only one needed a hand resolution, and every pair was test-merged beforehand rather than trusted.

| Order | PR | Landed as | What |
|---|---|---|---|
| 1 | [#33](https://github.com/prisant-labs/audiobook-organizer/pull/33) | `04bc23c` | Closes two vocabulary-gate gaps: `src/gallery` is now swept (it had shipped a retired word twice), and this page plus `CHANGELOG.md` joined the CI gate. Went first because it touches nothing the others do |
| 2 | [#32](https://github.com/prisant-labs/audiobook-organizer/pull/32) | `c189ec6` | **`AC-13`**, the two-step control for Archiving copies that were never compared. Went before `#31` so its new gallery specimen would be swept by `#33`'s new gallery gate in CI, not only on my machine |
| 3 | [#31](https://github.com/prisant-labs/audiobook-organizer/pull/31) | `04f480d` | **`AC-16` measured**, plus `FsContentSource`, the product's first code that can read a real file's bytes |
| 4 | [#34](https://github.com/prisant-labs/audiobook-organizer/pull/34) | `2f92378` | **`P3` steps 1-2**, the three resolution policies as pure functions |

**The one hand resolution**, between `#31` and `#34`: both edited the phase table in the implementation plan, `#31` the `P2` row and `#34` the `P3` row, which are adjacent lines in the same hunk. Independent edits, but git cannot merge them. Resolved by keeping each side's own row, which is exactly what had been written on `#34` before the merge started, so the resolution was a lookup rather than a decision.

**Verified on the merged result, not on the parts**: CI green on `main` at `2f92378` (all four checks), and locally `cargo fmt`, clippy, 675 Rust tests, 326 JS tests, the arbitrary-value ratchet exactly at 285/71, and the dash check.

Six remote branches remain because the repo sets `delete_branch_on_merge: false`, which is your configuration to change; the local ones are deleted.

**Merged 2026-08-14:** PR #23 and PR #24, the Dependabot Rust and JavaScript dependency groups.

**Merged 2026-08-05 and 2026-08-06:**

- **PR #11** - `P1c`, the interruption recovery surface. **`P1` is now complete**; only the `AC-6` and `AC-8` hand walkthroughs are owed on it.
- **PR #12** - the `docs/internal/backlog/` structure, `F-609` (library freshness), `F-610` (open a folder in the OS file manager), `FD-40` and `FD-41`.
- **PR #13** - `FD-42` (the Archive rename), the duplicates approach audit, and the `FD-40` clutter-default implementation.
- **PR #14** - closes `D-1`, `D-2` and `D-3` as `FD-43` (keep the old name for the action, since superseded by `FD-48`), `FD-44` (book-level duplicate comparison in as `P2b`) and `FD-45` (paths display one level).
- **PR #15** - `P2`'s engine half: BLAKE3 content hashing, its persistence, a cancellable verification job, and the gate that decides whether a duplicate group may be resolved automatically. `AC-13` (the two-step override) and `AC-16` (throughput measured on real data) are the remainder of `P2`.

**One thing worth knowing about `P2`, found while measuring it.** The hash verification engine merged in August as PR #15 and **cannot be run from the app**. The plan's own step said "wire the `dupes_hash_verify` command to the job (already in the command surface)", and no command by that name exists anywhere; the job has no callers; and until PR #31 there was no code in the product that could read a file's bytes at all, only in-memory test doubles. That last part is what makes it certain rather than likely. This is the same shape as the defect that created `P0`: the audit found undo complete but unreachable. Nothing is wrong with the engine and nothing is being hidden, but "`P2` engine merged" has been quietly meaning "merged and reachable from nothing", and the ladder above now says so. The command belongs with the duplicates surface that calls it (`P5`), so building it now would repeat the mistake pointing the other way.

**Next up:** `P3` steps 3 and 4, which are the safety-critical half: turning a confirmed resolution into real Archive operations that flow through the normal plan, apply and undo path (`AC-25`), and proving that undoing one puts every copy back byte for byte (`AC-27`). Steps 1 and 2, the policies themselves, merged as `#34`. After that: `P4` (duplicate review and report) and `P5` (the duplicates surface), which is also where the `AC-13` control built in `#32` finally gets wired to something, and where the `dupes_hash_verify` command noted above belongs. Two design items are also unblocked now that the gallery is on `main` with no open work touching it: proposing the spacing and type scale rendered against real components, and the wider documentation sweep.

**UI round 2** lives in `_local/gui/2026-08-04/round2/`. Prototypes only, nothing in the tracked tree, and superseded by the gallery in PR #26. Round 1 is closed with a full traceability record, and its four follow-up decisions `D-1` to `D-4` are closed as `FD-42` through `FD-45`, with `FD-43` since reversed as `FD-48`.

## Scope change on v0.6.0 (decision recorded, vetoable)

**History and undo were pulled into the interruption-safety milestone.** The 2026-07-30 audit found that v0.5.0's undo engine was complete but unreachable: the History route was a placeholder and nothing called either rollback command. Recovering a journal correctly and then offering the user nowhere to act on it is not a finished safety story, so the two ship together. This grows v0.6.0; the alternative was keeping real changes disabled for longer, which was going to happen anyway.

## What the app can and cannot do (kept honest)

Real changes are still not reachable from the UI, by design. The engine can execute against the real filesystem; the frontend pins every run to rehearsal, and it stays that way until all four of these close. **One is now closed**: 4 landed 2026-08-15. Of the remaining three, 3 is code and unstarted, and 1 and 2 are decisions only you can make:

1. **Power-loss threat model decided** (`FD-33`, journal durability boundary)
2. **Cross-volume move policy decided** (content verification before a cross-volume real move)
3. **A mechanical authorization boundary for real applies** (today the frontend pins dry-run but the command still accepts either mode, so the gate is procedural)
4. **Forward organizing blocked while an interruption is unresolved** - **CLOSED 2026-08-15** (`ff39a27`), under the narrower of two readings: a run that could not be reconciled at all blocks the forward path, while one that was cut short and successfully reconciled does not. Widening that scope is optional and is the one open question left over from the merge; shipping it as written is safe either way. (The engine function keeps the retired word in its name on purpose: `FD-48` moved the copy a user reads, not engineering identifiers, because renaming those is a migration.)

The README states this plainly rather than implying a finished write path.

## Cleanup when convenient

- [x] **Both vocabulary-gate gaps are CLOSED** (PR `#33`, 2026-08-15). `src/gallery` is now swept, globbed rather than listed so the next gallery file is covered too, and `STATUS.md` plus `CHANGELOG.md` joined the CI governance gate. Each was proven to catch a planted offender, not merely to pass: the sweep was tested with the exact label that shipped ("tidy-up active"), and `ShelfSection` was asserted NOT to trip it, so it can never start demanding that engineering identifiers be renamed.
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
