---
title: "Backlog: raised, not yet scoped"
type: backlog
updated: 2026-08-21
---

# Raised, not yet scoped

Surfaced in review but nobody has decided anything about it. Newest first.
See [`README.md`](README.md) for what belongs here.

---

## The single-writer lock does not know about verification jobs

- **Raised:** 2026-08-21, reviewing the pull requests merged on 2026-08-20.
- **The gap:** `F-601`'s single-writer lock counts `kind = 'apply'` only
  (`crates/abo-core/src/exec/lock.rs:91`, `SELECT COUNT(*) FROM jobs WHERE kind =
  'apply' AND state = 'running'`). That narrowness is deliberate and documented:
  it stops a stranded scan job being mistaken for an apply lock. The side effect
  is that **a duplicate verification job and an apply can run at the same time**,
  over the same files, while the apply is moving them.
- **What actually happens today:** probably nothing bad. The verification job
  reads; a file moved out from under it produces a read error, which the job
  already records as `hash_error` on that member rather than failing the pass.
  The worst realistic outcome is a hash recorded against a path that no longer
  holds that file, which a re-scan invalidates anyway because hashes hang off
  per-scan member rows.
- **Why it is still worth deciding:** "probably nothing bad" is an argument from
  the current shape of two subsystems, and it is exactly the kind of argument
  that stops being true when one of them changes. This is also the only remaining
  concurrency question in a product whose whole promise is that it does not touch
  a file it cannot put back.
- **Three postures, and this is the decision:**
  1. **Verification blocks and is blocked** - widen the lock to count both kinds.
     Simplest, and it makes the invariant say what a reader assumes it says.
     Costs: opening the duplicates screen during an apply would refuse rather
     than degrade, and the reclaim path would need to learn the second kind.
  2. **Apply cancels a running verification** - the apply is the operation the
     user is waiting on, and a cancelled pass keeps every hash it finished, so
     the cost of cancelling is genuinely low. More moving parts.
  3. **Leave it, and write down why** - record the read-only argument above as
     the reason, so the next reader does not rediscover it as a defect.
- **Recommendation: 1**, on the grounds that a safety invariant should be
  mechanical rather than a chain of reasoning about what two subsystems currently
  do. It is also the smallest change of the three.
- **Not started.** No code was written for this; the last session recorded it as
  "mild, recorded rather than fixed" and that judgement is not being reversed
  here without jp.

---

## The documentation vocabulary sweep is 375 instances, and mostly must NOT be swept

- **Raised:** 2026-08-21, measuring the sweep that has been carried in session
  logs as "~72 instances, mechanical, unblocked".
- **The measurement:** **375** instances of the retired words
  (`shel(f|ves|ved|ving)`, `tid(y|ies|ied|ying)`, `set...aside`) across **36
  files** in `docs/internal/`. The "~72" figure is exactly the count in
  `decision-ledger.md` alone.
- **It is not mechanical, and a blind sweep would do damage.** Three classes:
  - **PRESERVE, as historical record.** `decision-ledger.md` (72) and the specs
    of shipped releases. `FD-02` records a decision about "scan and tidy" and
    `FD-34` about "per-tidy-up provenance"; rewriting those falsifies the record
    of what was decided when. A ledger that has been edited to use today's words
    can no longer show that the words changed.
  - **SKIP, as engineering identifiers.** Out of scope by `FD-48`'s own carve-out,
    the same rule that spared `ensure_forward_tidying_allowed`.
  - **SWEEP, as living description of the current product.** The candidates:
    `functionality.md` (25), `product-requirements.md` (22), `architecture.md`
    (6), `program-roadmap.md` (3), `executive-summary.md` (3).
- **`design-system.md` (23) is already done and is not a candidate.** PR `#39`
  swept its 34 vocabulary instances on 2026-08-20 and the remainder are the
  legitimate ones: CSS token and class names, and the bookshelf-edge visual
  metaphor. Verified independently 2026-08-21 rather than taken on trust.
- **The CI gate cannot be extended to cover these** for the same reason `#39`
  recorded: it strips double-quoted spans to tell a mention from a use, and
  backticks are not double quotes, so a backticked token name trips it. Confirmed
  the hard way: a sentence written this session explaining that gap tripped the
  gate on the token name it cited as an example.
- **Proposed shape:** sweep the five living documents only, in one reviewable
  pass, roughly 59 instances. Leave the ledger and the shipped-release specs
  alone, and say so in a line at the top of each so the next reader does not
  re-raise it.

---

## Multi-file book duplicate comparison

- **Raised:** 2026-08-05, UI round 2 crit pass. jp: *"we can't assume the user
  only has m4b. Even today, groups of mp3s are common, and this is even more true
  of people who have been collecting audio books for many years."*
- **Feature id:** `F-1110` in the PRD's E-11 section.
- **The gap:** every part of `F-701` to `F-704` assumes **one book is one file**.
  A book split across twelve mp3s is twelve unrelated files to the detector, so
  two copies of it are twenty-four files that mostly will not match each other on
  filename and size. The same gap means one m4b versus a folder of mp3s is never
  compared either; the `F-205` parallel-format rule only catches same-stem
  siblings (`Dune.mp3` beside `Dune.m4b`).
- **Why it matters here specifically:** this library was collected over many
  years and has a lot of this shape. The duplicates feature would ship covering
  single-file books well and this case not at all.
- **Proposed shape:** compare book **folders** rather than files. Fingerprint a
  folder on audio-file count, total bytes, and normalised title, then escalate:
  cheap fingerprint match, then structural match (same ordered set of file sizes),
  then content match (`F-702` hashing applied pairwise, only on request). The
  m4b-versus-mp3-folder case falls out for free once the unit is a book, and the
  tool presents it as needs-your-eye rather than ranking it, because choosing
  between a single-file m4b and a twelve-part mp3 set is a preference rather than
  a mechanical judgement.
- **Recommended slot:** between `P2` and `P3`, because `P3`'s resolution policies
  should be written against books rather than files. Doing it the other way means
  writing them twice.
- **Status:** jp, 2026-08-05: *"I will leave this to our followup research,
  planning, scoping, etc."* Feeds the duplicates audit prompt.

## Non-audio clutter should default to leaving files in place

- **Raised:** 2026-08-05, UI round 2 crit pass. jp: *"this should be a setting
  defaulted to off, so non audio files can optionally stay where they are."*
- **Current behaviour:** the per-type setting already exists
  (`ClutterPolicy` in `crates/abo-core/src/ruleset.rs`), with `ebook` and `cover`
  defaulting to `Keep` and **`nfo`, `sfv`, `playlist`, `weblink` defaulting to
  `Quarantine`**. So the control is built; only the default is wrong for what jp
  wants.
- **Change:** flip those four defaults to `Keep`.
- **Why it is not a one-line change:** the default ruleset drives plan output, so
  golden fixtures and any test asserting clutter set-aside move with it. Small,
  but it touches the engine and its tests rather than a document.
- **Decision:** recorded as `FD-40` in the decision ledger.

## Nothing in the app shows what is in the Set Aside folder

- **Raised:** 2026-08-05, UI round 2 crit pass, out of the sidebar icon question.
- **The gap:** every removed copy and every set-aside leftover goes to that folder,
  and no screen lists what is in it. The user's only view is Explorer.
- **Partly answered** by the sidebar quick link (`F-610`), which opens the real
  folder. That may be sufficient: the OS file manager is a better folder browser
  than anything this app would build.
- **Open question:** does it need a screen, or is the link enough?

---

## Standing note

An item here has **not** been decided. Do not treat anything in this file as
planned work. If it becomes planned, it moves to a release spec and the entry is
deleted.
