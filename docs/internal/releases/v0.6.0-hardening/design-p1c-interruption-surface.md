---
title: "Design: P1c, the interruption recovery surface"
type: design
release: v0.6.0-hardening
phase: P1c
feature: F-606
date: 2026-08-04
status: awaiting-review
owner: jprisant
satisfies: AC-6, AC-7 (v0.6.0 hardening, as amended by FD-39)
depends_on: P1a/P1b (reconciler, merged 2026-07-31), P0 (History and undo, merged 2026-07-31)
sources:
  - docs/internal/releases/v0.6.0-hardening/spec.md
  - docs/internal/design-system.md
  - _local/gui/2026-07-22/resume-rollback.html (option B, selected 2026-08-04)
  - _local/gui/2026-07-22/feedback.md
---

# Design: P1c, the interruption recovery surface

This is a design, not acceptance criteria. `AC-6` and `AC-7` (v0.6.0 hardening)
in [spec.md](spec.md) remain the only acceptance criteria for this work; `FD-39`
in the [decision ledger](../../decision-ledger.md) amends `AC-7`.

## What this closes

`startup_interruption` has existed and been tested since P1a/P1b (interruption
safety) merged on 2026-07-31, and **no frontend code calls it**. If a tidy-up is
killed mid-run, the app reconciles the journal correctly on the next launch and
then says nothing at all. This phase builds the surface that speaks.

## Findings that shaped the design

Four things in the shipped code determined this design more than the mockups did.

**1. The single-writer lock is released before the user sees anything.**
`reclaim_stranded_apply_jobs` ([exec/lock.rs:135](../../../../crates/abo-core/src/exec/lock.rs))
runs at startup immediately after reconciliation:

```sql
UPDATE jobs SET state = 'failed', finished_at = ?,
       error_code = COALESCE(error_code, 'interrupted')
 WHERE kind = 'apply' AND state = 'running'
```

Its comment states the intent plainly: the stranded row must never block a fresh
apply. Nothing therefore stops the user starting a new tidy-up while an
interruption is unresolved. If the decision is meant to come first, the surface
has to be the gate. That is what makes option B (full-screen takeover) the
correct choice rather than a stylistic one.

**2. The offer is a process-lifetime snapshot.** `startup_interruption` is
computed once before `manage` ([lib.rs:293](../../../../src-tauri/src/lib.rs)) and
the command clones the stored value
([commands/mod.rs:310](../../../../src-tauri/src/commands/mod.rs)). It never
changes during the session, so the frontend owns "handled" state; the surface
reads it once on mount and clears its own copy when the user acts.

**3. The engine already resolves what can be done.** `UndoOffer`
([exec/history.rs:46](../../../../crates/abo-core/src/exec/history.rs)) has
exactly the five cases this surface needs, including `NeedsALook` for the
ambiguous outcome and `PracticeRun` for a rehearsal. `FD-36` (History and undo
pulled into v0.6.0) established that the undo offer is resolved in the engine
rather than derived in the shell. This surface honours that: it renders the
offer, it never computes one.

**4. The pipeline is idempotent by re-scanning.** An already-tidy book produces
no operation
([classify/metrics.rs:389](../../../../crates/abo-core/src/classify/metrics.rs),
`already_tidy_yields_zero_problems`), and an empty ops slice is a legal plan
([db/plans.rs:567](../../../../crates/abo-core/src/db/plans.rs)). Books moved
before the crash simply do not appear in the next plan. This is what makes
carrying on a matter of re-planning rather than replaying.

## The decision: re-plan rather than replay (FD-39)

`AC-7` as written says resume "continues the original job from the reconciled
point". No command does that, and building one would mean replaying operations
validated against a snapshot that the interrupted run itself invalidated. The
codebase already treats that as a hazard: `F-908` carries a dedicated
snapshot-stale re-validation state.

Carrying on is therefore satisfied by scanning and planning again, which runs the
whole validated pipeline instead of replaying a frozen plan. `AC-7`'s second
sentence, "Neither path bypasses validation", is satisfied more strongly this way
than by a resume command. Recorded as `FD-39` (carry-on by re-planning) in the
decision ledger.

**Consequence: this phase adds no backend command and no migration.** Every
capability it needs already ships.

## The three states

One component, driven by `ReconcileResult` plus the matching `HistoryEntry`.

### State 1: a practice run stopped early

Condition: `mode === "dry-run"`. This is the only state reachable today, because
`AppShell.tsx:107` pins every run to rehearsal, and `close_interrupted_rehearsal`
([exec/reconcile.rs:440](../../../../crates/abo-core/src/exec/reconcile.rs))
returns `resume_offered: false` on all three of its exits.

A rehearsal's effects lived in a `MemFs` that died with the process. Nothing on
the real shelves was touched, so there is nothing to carry on and nothing to put
back. The honest surface says so and gets out of the way.

Tone: `--warn`. One calm primary action: back to the library.

### State 2: a real tidy-up stopped early, outcome decisive

Condition: `mode === "real" && resume_offered === true`.

The reconciler verified from disk what happened to the single in-doubt operation
and repaired the journal. `done_count` changes landed and every one is recorded
for undo.

Tone: `--warn`. This is "needs you", not "something broke", which is what the
design system reserves `--warn` for (Section 2.2). Primary action: carry on
tidying up, which routes to the library for a fresh scan. Secondary: the undo
action the engine's `UndoOffer` resolved, normally `PutRecentChangesBack`.

### State 3: a real tidy-up stopped early, outcome ambiguous

Condition: `mode === "real" && resume_offered === false`.

The reconciler could not decide whether the last step finished. Carrying on is
genuinely unsafe here, and the reason is worth stating because it is not obvious:
a cross-volume copy killed mid-write leaves a target file that exists but may be
truncated. A fresh scan would see a book there and call it tidy. Ambiguity has to
block carrying on, exactly as `resume_offered` already says.

Tone: `--danger`, matching mockup C and the design system's failure register.
Actions come entirely from the `UndoOffer`; when it is `NeedsALook` there is no
automatic reversal and the only action is to open History.

### A nuance, not a fourth state

`interrupted === false` with a `ReconcileResult` present means the run was
stranded but nothing was in doubt: it died between its last operation and its
terminal row. This renders as state 2 without the in-doubt detail line.

## Architecture

### Where it mounts

`AppShell`, as a third short-circuit ahead of `activeJob` and `startError`. It
replaces the main screen area; the sidebar stays visible and navigation stays
live, exactly as the Apply screen already behaves.

```
AppShell
  interruption ? InterruptionNotice
  : activeJob  ? Apply
  : startError ? ErrorCallout
  : RouteContent
```

**Why not a hard gate before the shell.** An earlier draft mounted this in
`AppRoot`, ahead of `AppShell`, so the notice blocked the whole app until
answered. That was rejected on three grounds.

First, it is disproportionate to the states that will actually occur. Carrying
on is dangerous in exactly one of the three states, the ambiguous one, and it is
the only state that is unreachable today. In state 1 a hard gate traps a
non-technical reader in a screen they cannot leave in order to tell them nothing
happened. In state 2 carrying on is precisely what the app wants to encourage,
and browsing is useful, because History is where the undo lives.

Second, the dangerous action is starting a new tidy-up, not using the app.
Blocking every route to prevent one action is a blunt instrument aimed at the
wrong target.

Third, and decisively: a navigation block is a **procedural** gate. It holds only
while the surface is the sole way in, and it stops nothing an IPC caller can
reach. This project already carries one open finding of exactly that shape, the
real-apply mode that the frontend pins and the command still accepts. If an
unresolved interruption must stop a new tidy-up, that belongs in the engine.
See "The gate that belongs in the engine" below.

### Components

| File | Purpose |
|---|---|
| `src/hooks/useStartupInterruption.ts` | NEW. Fetches `startup_interruption` once, pairs it with the matching `history_list` entry, exposes `dismiss()` |
| `src/components/states/InterruptionNotice.tsx` | NEW. The presentational surface; three states, offer-driven actions |
| `src/lib/strings.ts` | Add `STRINGS.interruption` (FD-23: all user-facing copy centralized) |
| `src/components/shell/AppShell.tsx` | Add the branch ahead of `activeJob` |
| `docs/internal/design-system.md` | Add the state to Section 5 |

`InterruptionNotice` takes data and callbacks only, no IPC. That keeps it
directly testable and matches how `EmptyState` and `ErrorCallout` are built.

### Data flow

```
mount
  |
  +-> commands.startupInterruption()   -> ReconcileResult | null
  |     (synchronous backend read of a value captured at startup)
  |
  +-> commands.historyList(1)          -> HistoryEntry[]
        find entry where jobId === result.jobId  -> UndoOffer

  null result            -> render AppShell, no surface
  result present         -> render InterruptionNotice(result, entry)

user acts
  carry on     -> dismiss(), navigate("library")
  put back     -> rollbackPreparePartial(jobId, opIds)
                  -> dismiss(), openUndoPlan(planId)
  open History -> dismiss(), navigate("history")
  back         -> dismiss(), navigate("library")
```

Every action ends in `dismiss()` plus a normal navigation, so the surface adds no
new routing concept. The undo path reuses the machinery `History` already uses:
prepare a partial rollback, then open the prepared plan on the same review
surface a forward tidy-up uses (`D-09`). `AppShell.openUndoPlan` exists for
exactly this and is reused rather than duplicated.

Because the sidebar stays live, the user can also simply navigate away without
choosing. That is deliberate; see the limitations section for what is and is not
lost when they do.

### Error handling

| Failure | Behaviour |
|---|---|
| `startup_interruption` throws | Treat as no interruption, log, render the shell. A recovery offer that cannot be read must not block the app. |
| `history_list` throws or has no matching row | Render the narrative states with no undo action, and the "open History" action instead. Never guess an offer. |
| `rollback_prepare_partial` fails | `ErrorCallout` with the mapped family copy, dismissible back to the notice. |
| Reconciliation failed closed (`FD-37`) | **No surface at all.** See limitations. |

### Testing

- Vitest unit tests for `InterruptionNotice`: one per state, asserting heading,
  tone token, and which actions render. Includes the assertion that state 3
  never renders a carry-on action.
- Vitest tests for `useStartupInterruption`: null result, result with a matching
  History row, result with no matching row, `history_list` failing.
- `AppShell` routing test: an interruption preempts the route content and takes
  precedence over `activeJob`; dismissing restores the route; the sidebar stays
  rendered throughout.
- axe-core smoke on the surface in all three states, and a mechanical contrast
  check on the `--warn` and `--danger` pairs in both themes (`FD-21`).
- Keyboard walkthrough added to the manual QA checklist (`FD-21`).
- Manual walkthrough covers state 1 only, because it is the only state reachable
  while apply is pinned to rehearsal. States 2 and 3 are covered by tests until
  real applies are enabled.

## The gate that belongs in the engine

Carrying on is unsafe in exactly one state: the ambiguous one, where a
cross-volume copy killed mid-write can leave a target file that exists but may be
truncated, and a fresh scan would read it as a tidy book. The surface refuses to
offer carry-on there, but a surface refusing to offer something is not a gate.

The engine already has the right pattern. `ensure_forward_tidying_allowed`
([exec/verify.rs:787](../../../../crates/abo-core/src/exec/verify.rs)) blocks a
forward apply when a previous run left an unacknowledged discrepancy, exempts
inverse plans explicitly ("Undo is the remedy for a discrepancy"), and returns
`AppError::TidyingBlocked`; `acknowledge_check` is the un-blocking half. An
unresolved ambiguous interruption is the same shape of problem and belongs in the
same gate.

**This is deliberately NOT built in this phase.** The ambiguous state cannot occur
while apply is pinned to rehearsal, so building the gate now would be building for
an unreachable condition. Instead it joins the list of preconditions that must all
close before real changes are enabled, recorded in `STATUS.md`:

1. Power-loss threat model decided
2. Cross-volume move policy decided
3. A mechanical authorization boundary for real applies
4. Forward tidying blocked while an interruption is unresolved (new, from this design)

Putting it on that list rather than in this phase keeps the requirement where it
will be enforced instead of where it would merely look enforced.

## Copy

Plain-language register throughout (`PRODUCT.md`, design system Section 6). The
word "interrupted" is retained from the mockups because it is ordinary English;
"reconcile", "journal", "operation", and "dry run" never appear.

The standing reassurance line follows `FD-10`'s guarantee-enumeration rule: the
negated "deleted" wording is permitted only inside the full enumeration, so the
footer line is used verbatim or not at all.

## Limitations, recorded rather than hidden

1. **A fail-closed reconciliation produces no surface.** When `FD-37`'s gate
   refuses (unreadable `jobs.mode`, or more than one stranded job),
   `reconcile_stranded_apply_jobs` returns an error, `lib.rs:303` swallows it to
   `None` as best-effort, and the user sees nothing. The job is still visible in
   History carrying `reconcile-failed`, so the state is not lost, but the app
   does not raise it. Worth a follow-up: History should distinguish that row.

2. **Leaving without deciding loses the prominent offer.** The user can navigate
   away (the sidebar stays live) or quit. Either way the notice does not return:
   the surface clears its own copy on navigation, and the startup reclaim already
   moved the row out of `running`, so the next launch's reconciler will not find
   it. This is much smaller than it first appears: `list_history`
   ([exec/history.rs:99](../../../../crates/abo-core/src/exec/history.rs)) filters
   on `kind = 'apply'` with no state filter, so the run still appears in History
   with its change count and a resolved undo offer. The user loses prominence,
   not the ability to act. Whether the notice should persist for the session
   rather than clear on navigation is a reasonable refinement, deferred until
   there is a reachable state where it matters.

3. **States 2 and 3 cannot be walked by hand yet.** They need a real apply, which
   is gated behind the power-loss threat model, the cross-volume move policy, and
   a mechanical authorization boundary. They ship tested and unreachable, which
   is deliberate and is the same posture the executor already holds.

## Out of scope

- Any change to `AppShell.tsx:107`'s dry-run pin. Enabling real applies is a
  separate, human-gated decision.
- Per-operation drill-down on the interruption surface. `FD-36` already put that
  out of scope for History, and the same reasoning applies here.
- The `plan_exclude_op` control appearing in the mockups. That capability ships
  and works ([Review.tsx:145](../../../../src/routes/Review.tsx)); the mockups
  simply do not draw it. It is a mockup correction, not product work.
