---
title: "Audit: the Duplicates nav badge counts something the Duplicates screen does not show"
type: audit
date: 2026-08-21
status: complete
scope: AC-29 (v0.6.0 hardening, the Duplicates nav badge), AC-20 (counts agree across surfaces)
trigger: "Verifying AC-29 against a real scan without the GUI, while jp was away from the machine"
owner: jprisant
verdict: "On the real library the badge reads 406 and the screen lists 300. Two implementations of 'a duplicate group' feed the two halves of one sentence. Recorded and pinned by tests; NOT fixed, because the repair is a product decision."
---

# Audit: the Duplicates nav badge count

## The finding in one line

The nav badge beside **Duplicates** and the Duplicates screen itself compute
"how many duplicate groups are there" with two different pieces of code, and on
the real library they disagree by **106 groups**.

| | What it counts | Real library |
|---|---|---|
| **Nav badge** (`AC-29`) | Files bucketed by `(basename, size)`, every bucket with 2+ members | **406** |
| **Duplicates screen** | The same, minus groups a duplicated book folder already covers, plus the book groups themselves | **300** |

A person reads 406, clicks it, and finds 300 cards.

## How it was measured

Both pipelines were run over an export of the real completed scan of
`E:\Books - Audio` (scan 1, 2026-07-22, 14,799 entries: 719 folders and 14,080
files, about 299 GB). Nothing touched the live database: it was copied first,
and a backup of it sits at `_local/_db-backups/abo_2026-08-21_pre-migration-0009.db`.

The two paths, both reading the same scan:

* **Badge:** `get_scan_entries` then `inputs_from_snapshot` then
  `health_metrics` (`crates/abo-core/src/classify/metrics.rs:252`). Reaches the
  UI as the `duplicate-candidate-groups` problem metric, picked up by
  `navCountsFrom` in `src/hooks/useNavCounts.ts:34`.
* **Screen:** `get_scan_entries` then `plan_nodes_from_snapshot` then `extract`
  then `dupe_entries_from_plan_nodes` **and** `book_folders_from_plan_nodes`,
  then `detect_duplicates` (`crates/abo-core/src/plan/query.rs:626`). Reaches
  the UI through `dupes::review`.

The numbers reconcile exactly, which is what makes the diagnosis certain rather
than probable:

```
406   exact (basename, size) groups          <- the badge's whole answer
-113  marked subsumed_by_book_group
= 293 exact groups the screen still shows
+  7 book-level duplicate groups
= 300 candidate groups on the screen
```

306 book folders were detected in the library; 438 groups exist before the
candidate filter runs.

## Why the two differ

`detect_duplicates` does something `health_metrics` does not: after finding
file-level groups it also finds duplicated BOOK FOLDERS, and then calls
`mark_subsumed_exact_groups` (`crates/abo-core/src/dupes/detect.rs:284`). A
per-track group whose every member is owned by a book folder, which spans two or
more books, and whose books are all covered by one folder-level group, is marked
`subsumed_by_book_group`. `is_duplicate_candidate()` then returns false for it
and the review drops it.

That is correct behaviour and it is what makes the screen readable. A twelve-file
audiobook that exists twice is one duplicated book, not twelve unrelated
duplicate files.

`health_metrics` has no book concept at all. It buckets files by
`(basename, size)` and counts. So the badge counts **tracks** where the screen
counts **books**, and the gap grows linearly with track count, which means it is
worst on exactly the books most likely to be duplicated: long, many-file,
unabridged rips.

## Why nothing caught it

`review.rs` already carries a test called
`a_subsumed_group_is_excluded_just_as_the_copies_card_excludes_it`, whose comment
states the rule plainly:

> The review must count the same population the Copies card counts, or the export
> and the screen disagree, which is exactly what `AC-20` forbids.

That rule was enforced across three counters: the review, the export, and the
Copies card. **The nav badge is a fourth counter**, added later, from a different
module, and nothing checked it against that rule.

`useNavCounts.ts` documents a guarantee, and it genuinely provides it: the badge
and the Library home cannot disagree, because `AppShell` makes one
`useHealthMetrics()` call and hands the result to both. That comment is about the
badge versus the **home**. It is silent about the badge versus the **screen**,
and the reassuring tone of a nearby correct guarantee is part of why this was easy
to miss.

This is the same shape as two findings already on record: `AC-12`'s gate being a
convention rather than a mechanism, and `P2`'s engine being merged but reachable
from nothing. In each case something was true in one place and assumed everywhere.

## Status: recorded, not fixed

Three tests now pin the behaviour, in
`crates/abo-core/tests/nav_badge_count_agreement.rs`:

1. `the_badge_counts_tracks_where_the_screen_counts_books` - the headline, on a
   duplicated twelve-track book: badge 12, screen 1.
2. `the_gap_widens_with_every_extra_track` - the divergence is linear in track
   count, checked at 2, 5, 12 and 40.
3. `single_file_duplicates_agree_which_isolates_the_cause` - the control. Two
   single-file books that duplicate each other produce **1** from both counters,
   so the badge is not wrong in general; it is wrong exactly where a book spans
   more than one file.

Every assertion was proved to fail before being trusted: each expected value was
deliberately falsified and the failure output read, so that the passing versions
rest on measured values rather than on a test that cannot fail. The control was
probed too, because a control that passes because both sides are zero proves
nothing; both sides are 1.

## The decision this needs

Not fixed here, because both numbers are defensible for their own surface and
choosing changes what a person sees. Two options:

**A. Make the badge book-aware, so it matches the screen.** The badge becomes
"things you will find on that screen", which is what a badge next to a nav item
means. Cost: `health_metrics` would need the book-folder extraction that
`detect_duplicates` performs, on a path whose own comment advertises that it
"re-derives the classification and health metrics on every call". That is more
work per call on the app's most frequently rendered surface.

**B. Leave the badge as a cheap file-level count and rename what it measures.**
Keeps the home's health line honest as a file-level signal and stops the badge
implying a screen population. Cost: the badge stops answering the question a
person actually asks of it.

**Recommendation: A.** A number rendered directly beside the word Duplicates is
read as a promise about the Duplicates screen. The performance concern is worth
measuring rather than assuming: the same detection already runs on demand for the
screen, and the library it runs over is 14,799 entries, not millions.

Either way the fix is small. What it needs is the call, not the code.

## One more thing worth knowing

The Library home's own duplicate line reads from the same `health_metrics`
number, so **it will say 406 too**. Whatever is decided about the badge applies
to the home, and they will keep agreeing with each other either way, because
`AppShell` feeds both from one call.
