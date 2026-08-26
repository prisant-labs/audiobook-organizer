---
title: "Audit: the Duplicates nav badge counts something the Duplicates screen does not show"
type: audit
date: 2026-08-21
status: complete
resolved: 2026-08-25
scope: AC-29 (v0.6.0 hardening, the Duplicates nav badge), AC-20 (counts agree across surfaces)
trigger: "Verifying AC-29 against a real scan without the GUI, while jp was away from the machine"
owner: jprisant
verdict: "On the real library the badge read 406 and the screen listed 300. Two implementations of 'a duplicate group' fed the two halves of one sentence. FIXED 2026-08-25: the badge now quotes the screen's detector instead of paraphrasing it. See Section 'Fixed' at the end."
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

## How the divergence was pinned (2026-08-21, superseded by the fix below)

Three tests pinned the behaviour in
`crates/abo-core/tests/nav_badge_count_agreement.rs`:

1. `the_badge_counts_tracks_where_the_screen_counts_books` - the headline, on a
   duplicated twelve-track book: badge 12, screen 1.
2. `the_gap_widens_with_every_extra_track` - the divergence is linear in track
   count, checked at 2, 5, 12 and 40.
3. `single_file_duplicates_agree_which_isolates_the_cause` - the control. Two
   single-file books that duplicate each other produced **1** from both counters,
   so the badge was not wrong in general; it was wrong exactly where a book spans
   more than one file.

Every assertion was proved to fail before being trusted: each expected value was
deliberately falsified and the failure output read, so that the passing versions
rested on measured values rather than on a test that cannot fail. The control was
probed too, because a control that passes because both sides are zero proves
nothing; both sides were 1.

That file now asserts the AGREEMENT instead, and it carries the same control for
the same reason. Its own instruction was followed: it said that if the headline
ever passed as equal, the file should be replaced rather than repaired.

## The decision this needed

Both numbers were defensible for their own surface and choosing changed what a
person sees. Two options were offered:

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

**How that call was actually settled, which is worth recording.** Offering two
options overstated what was open. `FD-08` (duplicates canonical unit) says the
nav badge counts groups where a group is one book; `AC-18` says every count
shown, nav badge included, counts groups; `AC-29` says the badge shows the group
count; and the v0.6.0 release gate requires duplicates counted as groups
everywhere, naming the badge. Option B would have meant amending a ratified
decision, three acceptance criteria and a release gate. So the outcome was
already decided and only the timing was open. The audit should have said so.

## One more thing worth knowing

The Library home's own duplicate line reads from the same `health_metrics`
number, so **it will say 406 too**. Whatever is decided about the badge applies
to the home, and they will keep agreeing with each other either way, because
`AppShell` feeds both from one call.

---

## Fixed 2026-08-25

The badge, the Library home and the after-the-fact check report now all read the
duplicate count from the detector the Duplicates screen uses. On the fixture that
made the finding, a duplicated twelve-track book, every surface says **1**.

### The mechanism, which is deliberately NOT the one this audit proposed

Option A above says to give `health_metrics` the book-folder extraction that
`detect_duplicates` performs. **That would have built a second book-aware
duplicate counter**, and a second implementation of one rule is the disease this
audit diagnosed, not the cure. Its own "Why nothing caught it" section says so:
the badge was a fourth counter, from a different module, checked against nothing.
A fifth counter that happened to agree on the day it was written would have been
the same bet placed again.

`plan::query::detected_duplicates_for_scan` already carries the warning in its
doc comment, written before any of this: calling it "rather than running a second
copy of the pipeline" is "the shape that has already drifted twice in this
repository."

So there is still exactly **one** implementation of "a duplicate group is a
book", in `dupes::detect`, and a new seam makes the health metrics quote it:

`abo_core::classify::health_metrics_for_scan(pool, scan_id, entries, classifications)`

computes the ordinary `health_metrics` and then replaces the
`duplicate-candidate-groups` metric (count and byte total) with
`detected_duplicates_for_scan(..)` filtered by `is_duplicate_candidate()`, which
is the same predicate `ensure_duplicate_groups` persists by and the review counts
by. `health_metrics` itself is **unchanged** and still counts tracks.

### Every user-facing duplicate count goes through the seam

| Surface | Path | Fixed by |
|---|---|---|
| Duplicates nav badge | `classify_overview` -> `navCountsFrom` | the seam |
| Library home duplicate line | `classify_overview` -> `build_overview` | the same call, same payload |
| After-the-fact check report ("possible duplicate copies", before/after) | `exec::verify::metrics_for_scan` | the seam |

The third one was not in this audit's blast radius and is the reason the fix is
not a one-line change to `classify_overview`. `delta_health_metrics` renders a
before/after line for every problem metric, duplicates included, into a report a
user opens. Fixing only the badge would have left that report quoting the track
count, which is the same defect moved somewhere quieter.

**Deliberately left alone:** `classify::overview::duplicate_copies_by_book_folder`,
which answers "how many copies of THIS book" for the per-book chip on the Library
home. It is a different question from "how many groups", it is already scoped to
a book folder, and it is not a duplicate of the group counter.

### Three guards, because the one way this rots is silent

The splice finds its target by string. If either side is ever retyped, the
replacement becomes a no-op that looks like it worked and quietly restores the
track count. So:

1. **One shared const**, `classify::DUPLICATE_CANDIDATE_GROUPS`, used by the
   producer, the splice, and the check report's label map.
2. **`debug_assert_eq!(replaced, 1)`** inside the splice, so a miss panics in
   tests instead of serving the wrong number.
3. **`the_duplicate_metric_is_emitted_under_the_shared_id`** in `metrics.rs`,
   pinning that `health_metrics` emits exactly one metric under that id.

### Falsification, because a passing test proves nothing on its own

Both halves of the fix were deliberately broken and the failures read:

* **Candidate filter removed** from the splice: three tests failed with badge
  **13** against screen **1** (twelve track groups plus the one book group),
  which is the arithmetic this audit predicted. The single-file control **passed**
  throughout, which is exactly its job: with no book group there is nothing to
  subsume, so the filter is not load-bearing there.
* **Metric id drifted by one character** (`duplicate-candidate-group`): the
  `debug_assert` fired with "expected exactly one duplicate-candidate-groups
  metric to replace, found 0", and all four tests failed loudly rather than one
  silently returning 406-shaped numbers.

### Cost, stated rather than assumed

One extra snapshot read and one detection pass per `classify_overview` call, on a
surface whose contract is already that it re-derives on every call and never
caches (`AC-7`). The same detection already runs on demand for the screen, over
14,799 entries. Not measured against the real library; the walkthrough is where
that gets seen rather than argued.

### What has NOT been checked

**The real number.** Everything above is proved on fixtures. The prediction is
that the badge will now read **300** beside a screen listing **300**, but nobody
has run this against `E:\Books - Audio`. That happens in the UI walkthrough, and
if the badge reads anything other than the screen's own headline, this fix is
incomplete and this section is the place to say so.
