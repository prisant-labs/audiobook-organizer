---
title: "Review: the five pull requests merged on 2026-08-20, which nobody had read"
type: audit
date: 2026-08-21
status: partial
scope: "0c09c05..a49bdf5 (PRs #40, #41, #42, #43, #39): 33 files, 3,784 insertions"
trigger: "Standing gap recorded in STATUS.md and every recent session log: five pull requests merged on instruction plus CI, and CI is not a review"
owner: jprisant
verdict: "One reachable UI defect, fixed in PR #48. One count divergence, recorded in PR #47. Four designs that looked dangerous and are correct. Coverage is PARTIAL and the unread list below is part of the finding."
---

# Review: the merged-but-unread pull requests

## Why this exists

`STATUS.md` and three session logs carry the same line: five pull requests were
merged on 2026-08-20 on jp's instruction after CI went green, and **CI is not a
review**. This is the read.

It is deliberately recorded as `status: partial`. Coverage is listed honestly
below, because a review document that lets partial coverage read as full coverage
would reproduce the exact failure this project keeps cataloguing: something true
in one place, assumed everywhere.

## Defects found

### 1. Stop left the duplicates screen stuck (FIXED, PR #48)

Pressing Stop while copies are being checked cancelled the job correctly and left
the progress bar and its Stop button on screen forever, and dead-ended the Check
control for the life of the mounted screen.

`dupes_hash_verify` passes an empty `on_cancelled` closure to
`run_job_to_terminal`, so a cancelled job marks its `jobs` row and emits **no**
`job:completed` and **no** `job:failed`. The hook's `finish()` runs only from
those two events. `stopCheck` fired the cancel and cleared nothing.

The rule was already written down one file away, in `Library.tsx`'s `stopScan`:
"the backend never emits a job:completed/failed event for a cancelled job, so
this is the one place that transitions local state to the honest 'stopped'
outcome." That rule is a property of the **shared job wrapper**, not of the scan,
so it binds every caller of it. The duplicates screen was the second caller.

**Check this whenever a third job caller appears.**

### 2. The nav badge counts something the screen does not show (RECORDED, PR #47)

Badge 406, screen 300, measured on the real library. Full write-up in
[`2026-08-21_nav-badge-count-divergence.md`](2026-08-21_nav-badge-count-divergence.md).
Not fixed: which number is correct is a product decision.

## Things that looked dangerous and are correct

Recorded because the checking is the value, and because the next reader should
not spend the same time re-deriving them.

**`confirm_resolution` is DELETE-then-INSERT, and that is safe.**
`duplicate_confirmations.id` is a plain rowid and SQLite reuses rowids, so a
re-confirmation could in principle receive the id of the row just deleted and
inherit the previous decision's orphaned loser rows, which would archive the copy
the user had just chosen to keep. It does not happen: foreign keys are enforced
on the production pool (`db/mod.rs:204`), so the `ON DELETE CASCADE` on
`duplicate_confirmation_losers` clears them inside the same transaction first.

**The plan builder validates confirmations properly.** `confirmed_duplicate_losers`
(`plan/builder.rs:1649`) refuses ids absent from the snapshot, the keeper among
the losers, ids claimed by a second confirmation, non-file losers, and anything
under a relocated ancestor or inside staging.

**The two implementations of the `AC-12` rule agree.**
`content_is_verified_identical` (in-memory, `review.rs:359`) and
`group_may_auto_resolve` (from the database, `verify.rs:136`) are two answers to
one question, which the code itself names as a risk. They apply the same three
conditions, and both return false for book groups, since a folder member has no
hash of its own. Consistent with folder-group losers not being archivable yet.

**The CSV export escapes correctly.** `to_csv` uses the `csv` crate's writer, so
commas and quotes in Windows paths are quoted properly rather than breaking the
file.

## Noted, not fixed, not urgent

**`dupes_confirm` does not check group membership.** It accepts any
`keeper_entry_id` and `loser_entry_ids` and never verifies they belong to the
group named by `(method, group_key)`. The catastrophic cases are caught
downstream by the plan builder (above), and the only real caller derives losers
correctly: `DuplicateCard`'s `losersFor` filters the rendered group's own copies.
The residual gap is narrow. Worth closing if the same command ever gains a second
caller, which is when every gap of this shape has bitten so far.

**A latent race in `check()`.** `jobId.current` is assigned only after the
`await`, so a job that reached a terminal state inside that window would have its
event dropped and leave the same stuck progress bar PR #48 just fixed. Very
unlikely on a real library, where detection over 14,799 entries takes far longer
than an IPC round trip. Reachable in principle on a very small library.

**`check()`'s in-flight guard sits before an await.** Two clicks in the same tick
would both pass it. Currently prevented by the button being conditionally
rendered on `!progress`, which is set synchronously before the await. Correct
today, fragile: the guarantee lives in the JSX rather than in the guard.

## Coverage: what was read, and what was NOT

**Read closely:** `migrations/0009`, `dupes/job.rs`, `db/dupes.rs` (the
confirmation write and read paths), `commands/dupes.rs`, `hooks/useDuplicates.ts`,
`plan/builder.rs:confirmed_duplicate_losers`, `dupes/review.rs`
(`build_review_with_policy`, `content_is_verified_identical`, `to_csv`,
`found_by_label`), `dupes/detect.rs`, `classify/metrics.rs`, `db/mod.rs` pool
configuration, `DuplicateCard.tsx`'s two-step routing.

**NOT read, and therefore NOT reviewed:**

- `dupes/review.rs`'s remaining surface: the policy proposal path (`propose`),
  `display_name`, and the module's own test block.
- `routes/Duplicates.tsx` in full. Only the Stop wiring and the check button were
  read.
- `components/duplicates/PolicySelector.tsx` (70 lines) entirely.
- `DuplicateCard.tsx` beyond the confirm routing: the disclosure, the copy list
  rendering, and its keyboard behaviour.
- `ipc.rs`, `error.rs`, `strings.ts`, `errorCopy.ts`: the new error family and
  copy, which are seven-producer vocabulary surfaces.
- `gallery/fixtures.ts` and the new gallery specimens.
- The two accessibility smokes.

Everything on that list is low risk. The one item that was NOT low risk has since
been checked and is recorded below.

## Checked after the first pass: the shipped tokens match the ratified scale

This was the one unread item where a mismatch would have mattered, because it
would mean the document and the app disagreed about the thing just ratified. They
do not. Every one of the seven steps in `src/styles/tokens.css` matches
`design-system.md` section 3.1a exactly, size and line box:

| Step | Document | `tokens.css` |
|---|---|---|
| meta | 11 / 16 | 11px / 16px |
| body | 13 / 20 | 13px / 20px |
| lead | 15 / 24 | 15px / 24px |
| heading | 18 / 24 | 18px / 24px |
| title | 22 / 28 | 22px / 28px |
| display | 26 / 32 | 26px / 32px |
| hero | 30 / 36 | 30px / 36px |

`--tracking-display: -0.01em` matches too. The scale also satisfies every rule the
document claims it was derived under, checked rather than taken on trust: whole
pixels throughout, no two steps closer than 2px (the gaps are 2, 2, 3, 4, 4, 4),
every line box a multiple of 4px, and 13px present as the anchor.

## Method note

Every assertion behind PR #48 was proved to fail before being trusted, and one
correction came out of doing so: the first draft of the hook tests mocked
`scan_cancel` as a `Result` wrapper when the real binding returns a bare boolean,
which would have let a fix that checks that boolean pass vacuously. The mock was
corrected against `bindings.ts:48` and the three tests still failed before the
fix. That is the second time in two sessions that a test needed correcting before
its passing meant anything.
