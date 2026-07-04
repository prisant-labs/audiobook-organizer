---
title: Release Plans - README
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (governance batch)
sources:
  - docs/internal/decision-ledger.md
  - docs/internal/planning-audit-2026-07-03.md
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/release-plans/README.md
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/program-roadmap.md
  - docs/internal/program-roadmap.md
  - C:/Users/jpris/.claude/plugins/cache/jp-library/jp-library/1.5.0/skills/jp-release-plan/SKILL.md
---

# Release Plans

This folder holds the CROSS-RELEASE ceremony machinery for Audiobook Organizer: the six-gate tag-cutting runbook and the doc-update checklist that every release runs through. It does not hold the releases' content.

## Where release content actually lives

Per FD-16 (effort = release, see docs/internal/decision-ledger.md), Audiobook Organizer's unit of tracked work is the RELEASE, not a sub-release effort. Each rung of the ladder in `docs/internal/program-roadmap.md` (the roadmap) gets one self-contained folder:

```
docs/internal/releases/<version>-<codename>/
├─ spec.md                  the contract: scope, AC, sources
└─ implementation-plan.md   the how: ordered steps, test strategy, definition of done
```

Example: `docs/internal/releases/v0.3.0-planning/spec.md`. M-1 (campaign) is the one exception: it is an operational milestone, not a software version, so its folder holds `runbook.md` instead of a spec and plan pair.

This is a DELIBERATE difference from the repo-sync-tool convention this batch adapts from, where a release folder (`release-plans/plan_vX.Y.Z/`) contains one subfolder per numbered effort (`E-NN-slug/`) promoted in from `_unassigned/`. Audiobook Organizer has no effort layer under a release: the release folder's `spec.md` IS the unit that carries acceptance criteria, and there is nothing to promote or demote. FD-16 exists specifically to avoid colliding with the repo-sync `E-NN` effort namespace, since this project already uses `E-01..E-11` for a different thing (epics inside the PRD and feature-function breakdown, taxonomy only, never a tracking id).

## What lives here versus what the jp-release-plan skill creates

Two distinct things share the word "release plan":

1. **The release folders** at `docs/internal/releases/<version>-<codename>/` (above). These are authored directly by spec and implementation-plan authors as execution reaches each rung; they carry the AC. They already exist for v0.1.0 through v0.9.0 as of this planning suite (drafted, not yet built).
2. **A per-release plan document** that the `/jp-release-plan` skill (`--create vX.Y.Z`) can generate at execution time. The skill's own default convention writes that document to `docs/internal/release-plans/plan_vX.Y.Z/plan_vX.Y.Z.md`, aggregating readiness checks, hygiene gates, and the doc-update checklist for that release. Because this project has no per-effort subfolders to promote (item 1's own folder already is the whole release), running `--promote` is not applicable here; `--gate vX.Y.Z` (the read-only readiness report) is the useful subcommand, checked against the release's own `spec.md` and `implementation-plan.md` directly rather than against promoted effort folders.

In short: `docs/internal/program-roadmap.md` sequences the ladder; `docs/internal/releases/<version>-<codename>/spec.md` (and its implementation plan) owns the acceptance criteria; this folder (`release-plans/`) owns the cut ceremony (`runbook_cut-tag-release.md`) and the doc-update checklist (`release-checklist.yaml`) that every release runs through, whether or not a `/jp-release-plan` plan document is ever generated for it. The ceremony aggregates and gates; it never authors AC.

## Layout

```
release-plans/
├─ README.md                      this file
├─ runbook_cut-tag-release.md     the 6-gate tag-cutting ceremony (G0-G4)
└─ release-checklist.yaml         doc-update checklist rows, mirrored per release
```

## Two checklists, two jobs

1. The readiness checks (the release's own spec status, AC addressed, implementation plan's definition of done met, not stale against the 2026-03-25 baseline per FD-18 baseline-labeling rule) answer "is this release ready to ship?" They live in the release's own folder, not here.
2. The 6-gate cut runbook (G0 through G4) here is "how we actually cut and publish the tag." G0 consumes the readiness checks from item 1.

## Relationship to other docs

- `docs/internal/program-roadmap.md`: the cross-release execution ledger, dependency graph, and scope ledger (D-01 through D-17, FD-01 through FD-30 applied). The roadmap sequences; the release folders own AC; this folder governs the cut.
- PRODUCT.md (repo root): the design contract; stays authoritative for look, tone, and principles. Not modified by this batch.
- EXECUTION.md (repo root): governance, autonomy boundary (D-11), human-only gate list (D-10), model-tiering (FD-30).
- CHANGELOG.md (repo root, landing at v0.1.0 per FD-25 hygiene set): the user-facing notes layer; the GitHub Release body is derived from it. Kept separate from this internal governance.

## Status of this convention

Provisional, same posture as the repo-sync original: `/jp-release-plan` is useful automation, not a hard dependency. Nothing in this repo's governance breaks if the skill is never invoked; the runbook and checklist work by hand.
