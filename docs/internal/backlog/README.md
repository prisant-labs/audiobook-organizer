---
title: "Backlog: what it is, what goes here, how things leave"
type: backlog-index
created: 2026-08-05
status: active
---

# Backlog

Created 2026-08-05 because "I'll add it to the backlog" was being said when there
was nowhere to add it. If a thing is worth deferring, it is worth deferring to a
named place.

## The three files

| File | Holds | Test for "does it go here" |
|---|---|---|
| [`deferred.md`](deferred.md) | Work that was **in scope and got cut**, with the reason and the trigger to revisit | Someone decided not to do it *now* |
| [`raised.md`](raised.md) | Work **surfaced but never scoped**: gaps found in review, ideas from a crit pass, engineering debt | Nobody has decided anything about it yet |
| [`answered.md`](answered.md) | Questions that got a real answer, kept so they are not re-asked | It was a question, and it is now settled |

## Relationship to the PRD

`docs/internal/product-requirements.md` section **E-11 (v2 candidates, explicitly
deferred)** is the **feature registry** view and stays authoritative for anything
with an `F-nnnn` id. It is a numbered list of product features, and it is
referenced by specs.

This backlog is broader: it also holds settings changes, copy decisions, UX
gaps, and engineering debt, none of which earn an `F-nnnn`.

**The rule:** if a backlog item is a product feature, it gets an `F-nnnn` in
E-11 and the backlog entry links to it rather than restating it. Everything else
lives here only.

## Relationship to the decision ledger

`docs/internal/decision-ledger.md` records **what was decided and why**. The
backlog records **what has not been decided or done**. An item that gets decided
moves out of the backlog and into the ledger, or into a release spec if it is
being built.

## How things leave

An item leaves the backlog exactly three ways, and it should be obvious which:

1. **Scheduled.** It goes into a release spec with acceptance criteria. The
   backlog entry is deleted, not annotated, because the spec is now the record.
2. **Decided against.** It moves to the decision ledger with the reasoning. Not
   deleted silently, because "we thought about this and said no" is worth as much
   as "we said yes".
3. **Answered.** A question moves to `answered.md`.

**Nothing leaves by being forgotten.** If an item has gone stale, decide against
it explicitly and record that.

## How to add

Newest first within each file. Each entry carries: **what**, **why it is not
being done now**, **what would make it worth doing**, and the **date and source**
(which review, crit pass, or session raised it).
