---
title: "Audiobook Organizer - Status and Decision Queue"
type: status
project: audiobook-organizer
created: 2026-07-20
updated: 2026-07-30
status: active
---

# Audiobook Organizer: what needs you, and where everything stands

One page. The top section is the only part that needs you. Everything below is context.

## WAITING ON YOU (nothing else is blocked on code)

- [ ] **Restore GitHub Actions.** Jobs are refusing to start: *"recent account payments have failed or your spending limit needs to be increased."* This is the one item blocking merges. Under D-11 (agents may self-merge green PRs while private) a PR cannot merge without green CI, so the v0.6.0 P1 PR and three Dependabot PRs are all stuck behind it. Fix at github.com/settings/billing.
- [ ] **Cut tags v0.1.0 through v0.5.0.** Five releases are built and merged; zero are tagged. Human-only. Runbook: `docs/internal/release-plans/runbook_cut-tag-release.md`
- [ ] **AC-17 (round-trip evidence): accept or extend.** Byte-identical round-trip is proven on a 3-book subset. Say "good enough" or ask for a fuller run. Scratch copies retained at `E:\tmp\abo-rt\`. Detail: `docs/internal/releases/v0.5.0-acting/spec.md`
- [ ] **FD-33 / FD-34 veto window.** Two decisions from v0.5.0 that stand unless you object: journal durability boundary, set-aside placement (beside the library, not inside). Detail: bottom of `docs/internal/decision-ledger.md`
- [ ] **G-1 walk (v0.4.0 gate): approve a plan in the app.** The installer you have predates the v0.5.0 apply surface; a fresh build can be made so you walk the whole thing.
- [ ] **G-6 (v0.3.0 gate): a non-engineer reads the dry-run HTML report** and confirms it makes sense.
- [ ] **Power-loss durability (new, from the 2026-07-30 audit).** Is the safety promise process-kill recovery only, or power-loss too? The journal uses `synchronous = NORMAL`, which survives a kill but is not proven to survive a power cut before the write reaches disk. Recommendation: include power loss for real changes and use a durable barrier. Decide before any real-library campaign.
- [ ] **Cross-volume policy (new, from the audit).** A move between drives currently copies, compares size, then deletes the source. Equal size is not equal content. Recommendation: prohibit cross-volume real moves until content hashing lands. Decide before any real-library campaign.
- [ ] **Crit the 13 UI mockups** in `_local/gui/2026-07-22/` (open `index.html`). Highest-stakes: tidy-up and resume-rollback. P1c (the resume-or-rollback surface) is parked awaiting your direction on `resume-rollback.html`.

**Do NOT flip this repo public.** 38 `refs/pull/*` refs on GitHub still hold pre-rewrite commits containing the scrubbed family name (verified 2026-07-30: commit `dce9b55` carries it in 7 files, including `PRODUCT.md` and two dropped session logs). Flipping visibility re-exposes exactly what FD-35 removed. Only GitHub Support can purge those refs. The sanctioned path is a FRESH public repo from clean history: `_local/runbooks/fresh-public-repo-runbook.md`.

## Release ladder at a glance

| Release | Codename | Built | Merged to main | Tagged | Your open gate |
|---|---|---|---|---|---|
| v0.1.0 | spine | yes | yes | no | tag |
| v0.2.0 | understanding | yes | yes | no | tag |
| v0.3.0 | planning | yes | yes | no | tag, G-6 |
| v0.4.0 | seeing | yes | yes | no | tag, G-1 |
| v0.5.0 | acting | yes | yes | no | tag, AC-17, FD-33/34 |
| v0.6.0 | hardening | in progress | no | - | see below |
| M-1 | campaign | planned | - | - | - |
| v0.9.0 | packaged | planned | - | - | public flip decision |

## In flight right now

**Branch `feat/v0.6.0-p1-interruption-safety`**, 7 commits ahead of `main` (9e8a693). Not yet pushed; no PR (blocked on CI billing above).

Landed on the branch:

- **P1a/P1b (interruption safety, F-606)** - the reconciler: after a kill, find the single in-doubt operation, verify what actually happened on disk, repair the journal, and report what can be done. Four commits, all green.
- **The reconcile-failed copy fix** - the branch was red (`pnpm typecheck` + one frontend test) because a new error code had no family-safe copy.
- **Mode-aware reconciliation** - the audit found the startup sweep probed the REAL library to classify jobs that may have been rehearsals. Because the UI pins dry-run, every stranded job in practice was a rehearsal. Now gated on `jobs.mode`, fails closed on an unreadable mode, and fails closed rather than sweeping multiple stranded jobs.
- **History and undo (new scope, see below)** - the History screen, its engine read model, and the wiring that makes undo reachable.

Still open on v0.6.0: P1c (resume-or-rollback surface, awaiting your mockup crit), then P2 hash verification, P3 policies, P4-P7 surfaces, P8 long-path and gate. OQ-1 (bitrate source) blocks P3 keep-higher-bitrate; OQ-2 (ruleset schema mismatch) blocks P6.

## Scope change on v0.6.0 (decision recorded, vetoable)

**History and undo were pulled into the interruption-safety milestone.** The 2026-07-30 audit found that v0.5.0's undo engine was complete but unreachable: the History route was a placeholder and nothing called either rollback command. Recovering a journal correctly and then offering the user nowhere to act on it is not a finished safety story, so the two ship together. This grows v0.6.0; the alternative was keeping real changes disabled for longer, which was going to happen anyway.

## What the app can and cannot do (kept honest)

Real changes are still not reachable from the UI, by design. The engine can execute against the real filesystem; the frontend pins every run to rehearsal, and it stays that way until content verification for cross-volume moves, the power-loss decision, and a mechanical authorization boundary are all closed. The README now states this plainly rather than implying a finished write path.

## Cleanup when convenient

- `E:\tmp\abo-rt\` (~1.5 GB of AC-17 round-trip copies) once AC-17 is ratified.
- `E:\tmp\abo-rewrite*.git` working mirrors (keep `abo-backup.git` until fully satisfied).
- `_local/runbooks/replacements.txt` contains the real family name; safe to delete now the scrub is done.
- `git fetch --prune` for stale local branches whose remotes are gone.

## Where the real detail lives

- **Decisions:** `docs/internal/decision-ledger.md` (D-nn = yours; FD-nn = orchestrator, vetoable)
- **Release map and gates:** `docs/internal/program-roadmap.md`
- **Governance and human-only gates:** `EXECUTION.md` (Section 3 is the stop-and-hand-off list)
- **Full external audit:** `_local/audit/2026-07-30_audit_codex-56.md` (Codex 5.6, deep dive; its decision queue is folded into the top section above)
- **Fresh-public-repo procedure:** `_local/runbooks/fresh-public-repo-runbook.md`

---
*Keep this current by refreshing it whenever a gate clears or a decision lands (a natural step to fold into `/jp-wrap-session`). Format is markdown + YAML frontmatter on purpose: it stays greppable, diff-able, and portable into a Layer 2 tool later.*
