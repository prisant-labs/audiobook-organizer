---
title: "Audit: the duplicates approach, P2 through P5"
type: audit
date: 2026-08-05
status: complete
scope: v0.6.0 P2, P3, P4, P5 (F-702, F-703, F-704, F-905) plus F-1110
trigger: "UI round 2 crit pass, jp: create a prompt for auditing and finalizing how we need to approach this, so we can update the planning"
owner: jprisant
verdict: "The specs are sound for single-file books and silently wrong for multi-file ones. Reorder, do not rewrite."
---

# Audit: the duplicates approach

## Why this exists

jp, during the UI round 2 crit pass, on keeping duplicates in v1:

> *"After this document is closed, create a prompt for auditing and finalizing
> how we need to approach this, so we can update the planning."*

And, on the multi-file problem that provoked it:

> *"This is a funky reality. We can't assume the user only has m4b. Even today,
> groups of mp3s are common, and this is even more true of people who have been
> collecting audio books for many years."*

This audit re-derives the approach against the shipped code rather than
restating the specs. **The method matters:** every claim below was checked
against the repository, because the round 1 feedback document contained four
assertions I had made without checking, three of which were wrong.

## The five questions, and their answers

### 1. Does the multi-file gap change the plan? Yes, and it reorders it.

**Verified:** duplicate detection operates on individual files.
`crates/abo-core/src/dupes/detect.rs` groups by exact basename plus byte size,
and by normalised title. There is no folder-level concept anywhere in the
detector.

**The consequence, stated plainly.** A book that is twelve mp3s is twelve
unrelated files. Two copies of that book are twenty-four files that will mostly
not match each other, because part filenames (`01 - Chapter.mp3`) collide across
*different* books far more often than they identify the *same* book, and sizes
differ per part. The detector will either miss the pair entirely or produce
garbage groups of unrelated part-one files.

**This is not an edge case for this library.** It is the shape of any collection
assembled before m4b became common.

**Verdict: the specs are not wrong, they are silently narrow.** Every acceptance
criterion from `AC-10` to `AC-31` is correct *for single-file books*. None of
them says so. A reader of the spec would reasonably believe duplicates were
handled; a user with a multi-file library would find they were not.

### 2. Is candidate-only detection plus a good report enough for v1?

**No, and the reason is a promise the product already makes.**

Candidate-only means the tool says "these two are probably the same book". The
product's central promise is that it does not act on guesses, which is why
nothing is set aside today. But a report full of *probably* still leaves the user
doing the verification by hand, in Explorer, on 297 GB.

`P2` (hash verification) is what converts *probably* into *proven*. Without it,
the duplicates feature is a list of suspicions. With it, it is an answer.

**Verdict: `P2` is not optional if duplicates ships at all.** It is the phase
that makes the rest honest.

### 3. Should `P3`'s policies be written against files or books?

**Books. And this is the finding that reorders the plan.**

The four policies are keep-larger, keep-m4b, keep-higher-bitrate (now cut,
`F-1108`), and flag-only. Read them against a multi-file book:

| Policy | Against a file | Against a book |
|---|---|---|
| keep-larger | biggest file | biggest **total across the folder**. Different comparison |
| keep-m4b | prefer `.m4b` | prefer the **single-file** copy over the twelve-part one. Different meaning, and arguably the more useful of the two |
| flag-only | no action | no action. Unchanged |

**keep-m4b is the clearest case.** Against files it is a container preference.
Against books it becomes "prefer the copy that is one file over the copy that is
twelve", which is what a person actually wants and is a materially different
rule.

**Verdict: if `F-1110` is built after `P3`, the policies are written twice.**
Once against files, then again against books. That is the single strongest
argument in this audit for reordering.

### 4. What does the duplicates screen owe when it cannot compare much of the library?

**It owes an honest count of what it did not look at.**

The round 2 prototype already does this: a notice stating how many multi-file
books were not compared. That should be an acceptance criterion rather than a
mockup detail, because the failure mode without it is the worst kind: a screen
that looks complete, reports "3 groups of duplicates", and is silently ignoring
147 books.

**Proposed new criterion.** The duplicates surface states the number of books
excluded from comparison and why, whenever that number is above zero. A surface
that under-reports without saying so is a defect, not a limitation.

### 5. What is the revised phase order and size?

Recommended, with the change from the current plan marked:

| Order | Phase | Change |
|---|---|---|
| 1 | **`P2` hash verification** (`AC-10`..`AC-16`) | unchanged, still next |
| 2 | **`P2b` book-level comparison** (`F-1110`) | **NEW, inserted** |
| 3 | **`P3` resolution policies** (`AC-23`..`AC-27`) | unchanged in position, **rewritten against books**; `keep-higher-bitrate` cut per `F-1108` |
| 4 | **`P4` review and report** (`AC-17`..`AC-22`) | unchanged |
| 5 | **`P5` duplicates surface** (`AC-28`..`AC-31`) | unchanged, plus the not-compared notice |

**Why `P2b` sits after `P2` rather than before:** its third escalation tier is
pairwise content matching, which is `P2`'s hashing applied to a set. Building it
first would mean stubbing that tier and returning to it.

**Why it sits before `P3`:** question 3.

## What `P2b` actually is

Recorded so the audit produces something buildable rather than a direction.

**The unit changes from file to book folder.** A folder gets a fingerprint from
data the scan already holds:

- count of audio files in it
- total bytes across them
- the normalised title the parser already derives

Then the same escalation ladder the file path already uses:

1. **Fingerprint match.** Free: pure database work over existing scan data.
2. **Structural match.** Do the two folders hold the same ordered multiset of
   file sizes? A twelve-part book copied twice has twelve matching sizes in
   sequence. Nearly free, and very hard to trigger accidentally.
3. **Content match.** Hash pairwise, on request only. `P2` with a loop.

**The m4b-versus-mp3-folder case falls out of it.** Once the unit is a book,
"one m4b" and "a folder of twelve mp3s" are both books. They will match on
normalised title and roughly comparable total bytes, and will **fail** structural
matching, which is correct: they are not byte-identical and the tool cannot prove
they are the same recording. So it presents them as **needs-your-eye** rather
than ranking them. That is the honest outcome, and it is the case a person would
want to look at anyway, since choosing between a single file and a twelve-part
set is a preference rather than a mechanical judgement.

**Size estimate:** a detector module plus a schema addition for folder-level
groups, roughly the size of `P4`. Tier 3 is free once `P2` lands.

## What this audit did not resolve

Stated rather than glossed, because an audit that finds nothing open is not an
audit.

1. **Whether `P2b` is in scope for v0.6.0 at all**, or whether v0.6.0 ships
   single-file duplicates with an honest notice and `P2b` lands in v0.7.0. That
   is jp's call (`D-2` in the round 2 feedback doc). This audit recommends
   including it, on the write-the-policies-twice argument, but it is a real
   scope increase on a release that has already grown twice.
2. **The threshold for "roughly comparable total bytes"** in fingerprint
   matching. Too tight and re-encodes are missed; too loose and unrelated books
   group. Needs measurement against the real library, not a guess.
3. **Whether structural matching should be order-sensitive.** Same sizes in a
   different order might be the same book with different part naming. Probably
   yes, but it needs a fixture.

## Traceability

| Claim | Verified against |
|---|---|
| Detection is file-level, uses basename+size and normalised title | `crates/abo-core/src/dupes/detect.rs`, `crates/abo-core/src/dupes/mod.rs` |
| Detection is candidate-only, no hashing, no auto-set-aside | `dupes/mod.rs` module doc; `plan/builder.rs:886` |
| The group is the canonical unit, N members not 2 | `DuplicateGroup` / `DuplicateMember` in `dupes/detect.rs`; `FD-08` |
| Parallel-format only catches same-stem siblings | `plan/builder.rs:1425-1432` |
| Three policies, flag-only default (keep-higher-bitrate cut as `F-1108`) | v0.6.0 spec `AC-23` |
| `lofty` ships and already reads these files | `crates/abo-core/Cargo.toml:100-126` |

## Outputs

- `F-1110` in the PRD's E-11 section, and in `backlog/raised.md`
- `D-2` in `_local/gui/2026-08-04/round2/feedback_round2.md`, awaiting jp
- This document
