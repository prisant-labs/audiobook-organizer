---
title: "Audiobook Organizer - Status and Decision Queue"
type: status
project: audiobook-organizer
created: 2026-07-20
updated: 2026-08-21
status: active
---

# Audiobook Organizer: what needs you, and where everything stands

One page. The top section is the only part that needs you. Everything below is context.

## WAITING ON YOU (nothing else is blocked on code)

Every item below leads with what it IS; the reference ID in brackets is just the lookup key. Acceptance-criteria numbers restart per release, so each one names its release.

The fuller version of this list lives in the "What needs you" section of the latest session log under `_local/_session-logs/`, which is untracked on purpose.

- [x] **Merge the open pull requests** - DONE on your instruction, **four times**. First batch of six on 2026-08-15 (`#25`, `#26`, `#27`, `#28`, `#29`, `#30`); second of four (`#31`, `#32`, `#33`, `#34`); third of two on 2026-08-16 (`#35`, `#36`); fourth of five on 2026-08-20 (`#40`, `#41`, `#42`, `#43`, `#39`). `EXECUTION.md` Section 6.2 requires a human decision for any merge to `main`, and each instruction was that decision, recorded here rather than only in a session log. **No authorisation carries forward**: each covered only the pull requests open at that moment. Tags and releases were not touched: those are on the Section 3 human-only list.
- [x] **`AC-16` (v0.6.0 hardening, hash throughput on real data): do NOT descope duplicate hashing - ACCEPTED 2026-08-15**, as part of "continue based on your best recommendations". `F-702` ships as designed. The measurement behind it: your library holds 293 exact duplicate candidates totalling 14.96 GB, 5% of its 298.72 GB, because `AC-10`'s candidates-only rule already did the narrowing; the hashing code runs at 2,765 MB/s while the drive delivers 42 to 80, so **the wait is the disk, not the software**, and flag-only would not have saved a second of it. Ratified as `FD-49` in the [decision ledger](docs/internal/decision-ledger.md); evidence, with its limits stated: [hash-throughput-2026-08-15.md](docs/internal/releases/v0.6.0-hardening/hash-throughput-2026-08-15.md)
- [ ] **Look at the app running, which nobody has done.** Every visual judgement to date was made in the component gallery against fixtures. The gallery is the right review surface for a component and is NOT the app: it cannot show navigation, screen-to-screen rhythm, whether the Duplicates screen belongs beside Library and Organize, or how any of it reads at your window size with your 14,799 books. Two questions are yours and block the next chunk of UI work: **does the ratified scale look right in place** (only three files use it, so the honest question is whether Duplicates looks right BESIDE screens that have not adopted it), and **when do the other screens adopt it**. Also unwalked: the `AC-29` nav badge against a scan that actually has duplicates in it, and the whole duplicates flow end to end. Agenda in the 2026-08-21 session log.
- [ ] **Decide whether the merged code gets a human read.** Seventeen pull requests have merged on your instruction plus CI. CI is not a review, and this is recorded rather than implied away. Either read the four from 2026-08-20 (`#40` to `#43`, plus `#39`), or say "no review needed" so it stops being an open loop. An independent read is scheduled either way; see the audits folder.
- [ ] **Decide how a REAL apply is authorised** (precondition 3 of four, the last one that is not already closed or already recommended). Recorded as "code, not a decision", and it is not: `apply_start` takes the run mode as a parameter from whoever calls it, and the frontend only ever sends practice. The boundary could be a build that cannot do a real run at all, a setting that defaults off, or a one-shot permission issued separately. Those are different safety postures for the thing standing between this app and 299 GB of your books, which is why it is yours rather than mine. Recommendation: a setting that defaults off and cannot be turned on from inside the review flow, so enabling it is a separate deliberate act rather than one more click on the path you are already walking.
- [ ] **Reference screenshots: three to five, one line each on what to steal.** Drop them in `_local/gui/references/` (currently empty). `D-06` (anti-reference: "looks AI-generated") records what you hate; nothing on file records what you want, so every design decision is currently argued from the negative. This is the only open item nobody but you can do, the only one that raises the ceiling rather than closing a gap, and **the ceiling-raiser for STYLE**, which is the axis nothing else can supply. Note what it does and does not gate: the scale below is a separate, measurable axis that can move without it, and only `shadcn` is strictly sequenced (scale first, then behaviour components).

  **Where the UI actually stands, so the chain above is legible.** The guardrails are strong, and half the design language is now ratified. Two crit rounds are closed (`FD-39` to `FD-48`); the component gallery is on `main` with 53 specimens rendering the REAL components in both themes; the arbitrary-value ratchet freezes Tailwind sprawl at 285 uses of 71 distinct values and fails in both directions; seven separate sweeps gate user-facing vocabulary. **The spacing and type scale is ratified** into `src/styles/tokens.css` (PR `#42`), which settles CONSISTENCY and is measurable. What is still missing is the positive direction on STYLE, which no measurement can supply: what good looks like here has never been written down, because the only artifact that would establish it is the one above.

  *Why this paragraph was wrong the day it landed, which explains the pattern rather than just this instance.* It entered the tree in PR `#39` saying 47 specimens and an unratified design language. It was written before `#42` and `#43` existed and merged after both, so it was stale on arrival. A documentation pull request that waits behind code pull requests describes the tree as it was when the author last looked, and merging it does not re-check that. Rebase and re-read before merging a docs change, or it lands already false.

- [x] **Ratify a spacing and type scale - DONE 2026-08-20** (PRs `#40` proposed, `#42` ratified). Seven type steps live in `src/styles/tokens.css` under `@theme` (`text-meta` 11px, `text-body` 13px, `text-lead` 15px, `text-heading` 18px, `text-title` 22px, `text-display` 26px, `text-hero` 30px), each with a line box on the 4px grid so type and spacing compose. Setting a size inline is now a defect. **Spacing deliberately got no tokens**: measurement showed only 5 inline spacing values against 292 uses of 16 standard steps, so the app was already on the 4px grid and what it lacked was a rule about which steps exist. Seven are allowed (0.5, 1, 2, 3, 4, 6, 8), nine retired, and adopting the rule renames nothing. **The measurement corrected the brief**, which is the part worth keeping: the plan was to derive both scales from the same inline values, right for type and wrong for spacing.

  **What this does NOT yet mean.** Only three files use the scale (`DuplicateCard.tsx`, `PolicySelector.tsx`, `Duplicates.tsx`). **161 inline text sizes of 18 distinct values remain** in the rest of the product code, untouched on purpose. Sweeping them is a real chunk of work that changes how every screen looks slightly, so it wants your go-ahead and probably your eyes screen by screen. The ratchet falls as components adopt, one visible diff at a time.
- [x] **`design-system.md`'s type and spacing sections - CORRECTED 2026-08-20** (PRs `#42`, `#39`). It used to state type as RANGES ("h1 26-30px") and say nothing at all about spacing, which was exactly the dimension that drifted. Sections 3.1a and 3.1b are now normative and ratified, and 3.1c keeps the old ranges deliberately, as the evidence the drift was measured against rather than as guidance. Its vocabulary was separately 34 instances out of date and was corrected in the same pass.

  **One gap stays open and is recorded rather than forgotten**: `design-system.md` is NOT in the CI vocabulary gate. That is a reasoned deferral, not an oversight. The gate strips double-quoted spans to tell a mention from a use, which is not enough for this file, because its legitimate uses of the retired words are CSS token and class names ("--shelf-rail", ".shelfhead") and a bookshelf-edge visual metaphor. This very sentence is the demonstration: it had to switch those two names from backticks into double quotes to get past the gate, because backticks are not what the gate strips. Gating it needs a pattern that understands the component-versus-copy distinction, which is a larger change than the sweep it would protect.
- [ ] **`shadcn/ui` is configured with zero adoption and is now UNBLOCKED.** No Radix or Base UI dependency is installed; `components.json` is complete and `src/components/ui` does not exist. It was last in the chain on purpose, because the settled order is scale first THEN shadcn, and **the scale is now ratified**, so the only thing holding it is your go-ahead. shadcn is for BEHAVIOUR only (select, combobox, dialog, popover, tooltip, tabs, checkbox, switch), never domain components. Note for whoever picks it up: check current documentation first, because shadcn moved off Radix onto Base UI.
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
| v0.6.0 | hardening | in progress | `P0`/`P1`, `P2`, `P2b`, `P3`, **`P4` and `P5` all complete** | no | `AC-6` walk, `AC-8` walk, the UI walkthrough |
| M-1 | campaign | planned | - | - | - |
| v0.9.0 | packaged | planned | - | - | installer, signing decision |

## In flight right now

`main` is at `a49bdf5`, CI green, **zero open pull requests and zero local branches**. Seventeen have landed across 2026-08-15 to 2026-08-20 in four authorised batches: six, then four (`#31` to `#34`), then two (`#35`, `#36`), then five (`#40` to `#43` plus `#39`). The last three batches are recorded below, newest first.

### Merged 2026-08-20, fourth batch

Authorised on your "go" once CI was green, then **re-verified locally on the merged result** rather than trusted from five separate pull request runs. **No human has read this code.**

| Order | PR | Landed as | What |
|---|---|---|---|
| 1 | [#40](https://github.com/prisant-labs/audiobook-organizer/pull/40) | `ad3c013` | The spacing and type scale PROPOSED, rendered against real components so it could be reacted to rather than read as a table of numbers |
| 2 | [#41](https://github.com/prisant-labs/audiobook-organizer/pull/41) | `ef96d51` | **`P5` backend.** `dupes_hash_verify` finally reachable, as a background job with progress and a Stop; the review and export commands; confirmations reaching the plan builder; migration 0009 making the duplicate write path idempotent; and `AC-12`'s gate moved into `abo_core` |
| 3 | [#42](https://github.com/prisant-labs/audiobook-organizer/pull/42) | `af20a1e` | **The scale RATIFIED** into `src/styles/tokens.css` and `design-system.md` |
| 4 | [#43](https://github.com/prisant-labs/audiobook-organizer/pull/43) | `d396130` | **`P5` surface.** The Duplicates screen, the duplicate card, the policy selector, four gallery specimens, two accessibility smokes. Built entirely on the ratified scale, so the ratchet did not move |
| 5 | [#39](https://github.com/prisant-labs/audiobook-organizer/pull/39) | `a49bdf5` | The planning documents, open since 2026-08-16 |

**Two findings from this batch outlast its code.** First, **`AC-12`'s gate was a convention rather than a mechanism**, enforced in whichever caller remembered it; it now lives in `abo_core`. That is the same shape as precondition 3 above, one layer down on the path that archives files, which is why the recommendation there is a mechanism and not a habit. Second, **a clean auto-merge was wrong**: `dupes/mod.rs` merged with no conflict marker and produced a duplicated re-export block that only the compiler caught. Stacked pull requests and squash merges collide, so the whole sequence was simulated in a throwaway branch first and each file verified byte-identical to its pre-merge state.

**Verified on the merged result, not on the parts** (re-measured 2026-08-21 on `a49bdf5`): `cargo fmt`, clippy with warnings denied, **720 Rust tests**, **340 JS tests across 43 files**, the token-contrast check, the arbitrary-value ratchet exactly at its 285/71 baseline across 75 files, the dash check across 367 tracked text files, and the retired-vocabulary gate on all five governance documents. All green.

| PR | Landed as | What |
|---|---|---|
| [#25](https://github.com/prisant-labs/audiobook-organizer/pull/25) | `5b1e867` | Arbitrary-value ratchet: freezes the app's Tailwind sprawl at 285 uses of 71 distinct values and fails CI in **both** directions, so removing values cannot silently bank slack for new ones |
| [#27](https://github.com/prisant-labs/audiobook-organizer/pull/27) | `d9a312a` | `FD-48` end to end: the action is "organize" and the noun is retired rather than replaced |
| [#26](https://github.com/prisant-labs/audiobook-organizer/pull/26) | `2c2b524` | Component gallery, 29 specimens of the real components in both themes. Dev-only: `pnpm build` emits no trace of it |
| [#29](https://github.com/prisant-labs/audiobook-organizer/pull/29) | `ccc13e0` | **`P2b`** (`F-1110`, book-level duplicate comparison), all of `AC-51` to `AC-55`. Engine-only |
| [#30](https://github.com/prisant-labs/audiobook-organizer/pull/30) | `ff39a27` | The fourth real-apply precondition: refuse a forward run while the last one could not be accounted for |

**The one manual fix**, in `#26`: the gallery predated `FD-48` and referenced two renamed identifiers, plus a third line the recorded note had missed - a specimen label reading "tidy-up active", which is free text and so invisible to the type checker. Nothing sweeps `src/gallery` for retired vocabulary; that gap is now the top item under "Cleanup when convenient".

### Merged 2026-08-16, third batch

| PR | Landed as | What |
|---|---|---|
| [#36](https://github.com/prisant-labs/audiobook-organizer/pull/36) | `9b288ef` | **`P3` steps 3-4, completing `P3`.** A confirmed duplicate resolution becomes an ordinary Archive move, so undoing a run puts every copy back byte for byte. Scoped to single files: a whole folder cannot be archived across drives yet, and nothing today can ask it to |
| [#35](https://github.com/prisant-labs/audiobook-organizer/pull/35) | `58fb4f2` | This page's own refresh, plus the reclassification of precondition 3 |

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

**Twenty-one remote branches remain** (verified 2026-08-21), because the repo sets `delete_branch_on_merge: false`, which is your configuration rather than a mistake to correct. All of them are merged. Local branches are deleted after every batch. Left alone deliberately: pruning them is a one-line call and it is yours, since the setting says you may want the history.

**Merged 2026-08-14:** PR #23 and PR #24, the Dependabot Rust and JavaScript dependency groups.

**Merged 2026-08-05 and 2026-08-06:**

- **PR #11** - `P1c`, the interruption recovery surface. **`P1` is now complete**; only the `AC-6` and `AC-8` hand walkthroughs are owed on it.
- **PR #12** - the `docs/internal/backlog/` structure, `F-609` (library freshness), `F-610` (open a folder in the OS file manager), `FD-40` and `FD-41`.
- **PR #13** - `FD-42` (the Archive rename), the duplicates approach audit, and the `FD-40` clutter-default implementation.
- **PR #14** - closes `D-1`, `D-2` and `D-3` as `FD-43` (keep the old name for the action, since superseded by `FD-48`), `FD-44` (book-level duplicate comparison in as `P2b`) and `FD-45` (paths display one level).
- **PR #15** - `P2`'s engine half: BLAKE3 content hashing, its persistence, a cancellable verification job, and the gate that decides whether a duplicate group may be resolved automatically. `AC-13` (the two-step override) and `AC-16` (throughput measured on real data) are the remainder of `P2`.

**One thing worth knowing about `P2`, and how it closed.** The hash verification engine merged in August as PR #15 and **could not be run from the app** for two weeks. The plan's own step said "wire the `dupes_hash_verify` command to the job (already in the command surface)", and no command by that name existed anywhere; the job had no callers; and until PR #31 there was no code in the product that could read a file's bytes at all, only in-memory test doubles. That last part is what made it certain rather than likely. It was the same shape as the defect that created `P0`: an audit found undo complete but unreachable. **PR `#41` closed it on 2026-08-20**, building the command with the duplicates surface that calls it, which is where it always belonged. The lesson is kept because it recurs: "engine merged" and "reachable from the product" are different claims, and only the second one is worth anything to a user.

**Next up.** `P4` (review and report) and `P5` (the duplicates screen) are both **complete**, so the duplicates line of v0.6.0 is finished end to end: the engine is reachable, the screen exists, and `AC-13`'s override is wired to a real action for the first time since it was built on 2026-08-15. What remains on v0.6.0 is `P6` (ruleset import and export), **blocked on `OQ-2`** (how a ruleset schema-version mismatch is handled), plus `P7` (the everything view), `P8`, `P9` (library freshness) and `P10` (open a folder in the file manager), none of them started and none of them blocked.

**The critical path is no longer code.** Every remaining precondition for a real run against your library is a decision rather than an implementation: precondition 3 above, the power-loss threat model, and the cross-volume move policy. Nothing has ever run for real against your library.

**UI round 2** lives in `_local/gui/2026-08-04/round2/`. Prototypes only, nothing in the tracked tree, and superseded by the gallery in PR #26. Round 1 is closed with a full traceability record, and its four follow-up decisions `D-1` to `D-4` are closed as `FD-42` through `FD-45`, with `FD-43` since reversed as `FD-48`.

## Scope change on v0.6.0 (decision recorded, vetoable)

**History and undo were pulled into the interruption-safety milestone.** The 2026-07-30 audit found that v0.5.0's undo engine was complete but unreachable: the History route was a placeholder and nothing called either rollback command. Recovering a journal correctly and then offering the user nowhere to act on it is not a finished safety story, so the two ship together. This grows v0.6.0; the alternative was keeping real changes disabled for longer, which was going to happen anyway.

## What the app can and cannot do (kept honest)

Real changes are still not reachable from the UI, by design. The engine can execute against the real filesystem; the frontend pins every run to rehearsal, and it stays that way until all four of these close. **One is now closed**: 4 landed 2026-08-15. **All three that remain are decisions only you can make**, which is the single most important line on this page: there is no longer any code standing between this app and a real run against your library, only unmade calls.

*(Corrected 2026-08-21: this paragraph used to say "3 is code and unstarted", contradicting the top section of this same page, which reclassified precondition 3 as a decision when PR `#35` landed. The reclassification was written into one section and not the other. Both now say the same thing.)*

1. **Power-loss threat model decided** (`FD-33`, journal durability boundary)
2. **Cross-volume move policy decided** (content verification before a cross-volume real move)
3. **A mechanical authorization boundary for real applies** (today the frontend pins dry-run but `apply_start` still accepts either mode from whoever calls it, so the gate is a convention rather than a mechanism). **`AC-12` had the identical defect** on the path that archives files and was fixed on 2026-08-20 by moving the gate down into `abo_core`, where a caller cannot forget it. That is the shape this one still needs, plus your call on which posture: a build that cannot do a real run at all, a setting that defaults off, or a one-shot permission issued separately.
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
