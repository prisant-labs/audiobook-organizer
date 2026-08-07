---
title: "Backlog: answered questions"
type: backlog
updated: 2026-08-05
---

# Answered

Questions that got a real answer, kept so they are not re-asked and so the
answer is findable. Newest first. See [`README.md`](README.md).

---

## Does the tool ever delete an audiobook?

- **Asked:** UI round 1 crit, and repeatedly before that.
- **Answer: no, never.** `PRODUCT.md` commits to "nothing deleted and everything
  reversible". Duplicates and leftovers are **moved** to a folder beside the
  library, not removed. The only thing ever deleted is an empty folder.
- **The one hole, worth knowing:** you can empty that folder yourself. If you do,
  and then undo a tidy-up that put things there, the tool cannot restore what it
  no longer has. It checks on your action and says so before moving anything.
- **The one nuance:** a cross-volume move copies then deletes the source. That
  path is currently blocked pending content verification, and that block is a
  public promise in `SECURITY.md`.

## How can the app work if the frontend cannot touch the filesystem?

- **Asked:** UI round 2 crit. jp: *"I don't understand this. How can the app
  functionally work if it cannot access the file system?"*
- **Answer: the app does access the filesystem, just not from the half that draws
  the screens.** The Rust backend has full access and does every scan, move,
  rename, and cover read. The web layer has none. They talk over a typed message
  channel: the frontend describes what it wants, the backend does it, results come
  back.
- **Why:** the web layer is the part most exposed to bugs and injection. If it
  cannot reach the filesystem, no fault in it can move, delete, or read the
  library. All the risk concentrates in the Rust half, which is smaller, typed,
  and heavily tested.
- **Consequence:** "open this folder in Explorer" is not blocked, it just needs a
  message like everything else. That is `F-610`.

## What does "candidate-only" duplicate detection mean?

- **Asked:** UI round 2 crit.
- **Answer:** the tool finds groups it **suspects** are the same book and stops.
  It groups on two signals (identical filename plus identical byte size, and
  titles that normalise to the same string), records the groups, counts them on
  the library screen, and does nothing else. No content comparison, no keeper
  chosen, nothing moved.
- **Why it stops there:** "same name, same size" is strong evidence but not proof,
  and setting a book aside on a suspicion is exactly what this product refuses to
  do. `P2` (hash verification) is what turns a candidate into a certainty.

## What does the "flag only" resolution policy do?

- **Asked:** UI round 2 crit.
- **Answer:** find the duplicates, show them, **take no action**. It is the
  default of the three policies precisely because it is the one that cannot be
  wrong. The other two pick a keeper automatically. (Was four until
  keep-higher-bitrate was cut as `F-1108`.)
- **Surfacing note (jp, 2026-08-05):** in the app this reads as "do nothing",
  which is accurate but sounds like a failure. Better user-facing framing:
  **"Just show me the duplicates"**, with the doing-nothing part stated as
  reassurance rather than as the name of the setting.

---

## Standing note

An answer here is only as good as its date. If the code changes, the answer is
stale. Each entry should be verifiable against the repo in a few minutes.
