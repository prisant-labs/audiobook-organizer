---
title: "Backlog: raised, not yet scoped"
type: backlog
updated: 2026-08-05
---

# Raised, not yet scoped

Surfaced in review but nobody has decided anything about it. Newest first.
See [`README.md`](README.md) for what belongs here.

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
