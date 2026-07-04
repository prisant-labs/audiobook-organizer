---
title: Architecture Decision Records
date: 2026-07-03
status: review
owner: jprisant
produced-by: ADR-batch author agent
sources:
  - docs/internal/decision-ledger.md
  - docs/internal/planning-audit-2026-07-03.md
  - E:\Projects\prisant-labs\audiobook-organizer\_local\planning\audiobook-organizer-strategy-brief_2026-07-02.md
  - E:\Projects\prisant-labs\audiobook-organizer\_local\planning\release-plan-and-ci_2026-07-02.md
  - E:\Projects\prisant-labs\audiobook-organizer\_local\prior-work\folder-structure.md
  - E:\Projects\product-on-purpose\repo-sync-tool\docs\internal\v1-architecture-and-decisions.md
---

# Architecture Decision Records

This directory tracks architectural decisions for audiobook-organizer using the MADR v4 (Markdown Architectural Decision Records) format. Each record captures a ratified decision (D-nn) or Fable-fixed disposition (FD-nn) from the planning suite's decision ledger, `docs/internal/decision-ledger.md`, along with the options considered and the consequences accepted.

All decisions below are recorded as accepted: they document decisions already ratified by jp or resolved by the orchestrator on 2026-07-03, not proposals awaiting a choice.

## Index

| # | Title | Status | Date | Refs |
|---|---|---|---|---|
| 0001 | [Stack locked to the common stack](0001-stack-locked-to-common-stack.md) | accepted | 2026-07-03 | D-01 (stack locked) |
| 0002 | [Engine-first build order](0002-engine-first-build-order.md) | accepted | 2026-07-03 | D-07 (engine-first order) |
| 0003 | [Rename-first executor](0003-rename-first-executor.md) | accepted | 2026-07-03 | D-08 (rename-first executor) |
| 0004 | [Safety invariants: quarantine, journal, single-writer](0004-safety-invariants-quarantine-journal-single-writer.md) | accepted | 2026-07-03 | D-09 (safety invariants) |
| 0005 | [Author-first default layout](0005-author-first-default-layout.md) | accepted | 2026-07-03 | D-02 (author-first default layout) |
| 0006 | [Family tier sets the UI bar](0006-family-tier-sets-ui-bar.md) | accepted | 2026-07-03 | D-03 (audience and UI bar) |
| 0007 | [Dry-run HTML report is first-class, P0](0007-dry-run-report-first-class.md) | accepted | 2026-07-03 | D-04 (milestone posture); F-506 (dry-run HTML report) |
| 0008 | [docs/internal/ is tracked in git](0008-docs-internal-tracked.md) | accepted | 2026-07-03 | D-12 (docs/internal tracked) |
| 0009 | [Effort unit equals release](0009-effort-equals-release.md) | accepted | 2026-07-03 | FD-16 (effort equals release) |
| 0010 | [Pack and award provenance captured in v1](0010-provenance-captured-in-v1.md) | accepted | 2026-07-03 | D-14 (provenance in v1); FD-01 (F-507 pack provenance) |
| 0011 | [Cards and report over tree diff](0011-cards-and-report-over-tree-diff.md) | accepted | 2026-07-03 | D-16 (review surface); FD-06 (F-501 redefined) |
| 0012 | [Covers committed to v0.4.0 with fallback](0012-covers-committed-v0.4.0.md) | accepted | 2026-07-03 | D-15 (cover extraction v0.4.0); FD-03 (F-907 cover extraction) |
| 0013 | [Backup posture is user-defined](0013-backup-posture-user-defined.md) | accepted | 2026-07-03 | D-17 (backup posture user-defined) |
| 0014 | [Offline-first: no auto-update, no telemetry](0014-offline-first-no-update-no-telemetry.md) | accepted | 2026-07-03 | FD-11 (bundled fonts, zero network); FD-22 (unsigned installer, no auto-update) |

## Format

Each ADR follows MADR v4: frontmatter (`status`, `date`, `decision-makers`), a numbered title, Context and Problem Statement, Considered Options, and Decision Outcome with Consequences. Once accepted, an ADR is treated as immutable history; a changed circumstance gets a new ADR that supersedes the old one rather than a rewrite.

## Governance note

Epics (E-01 through E-11) are a feature-organizing taxonomy used only in the PRD and feature-function breakdown. They are not used as governance effort IDs; per 0009 (effort equals release), the tracked unit of planning and execution work is the release, in `docs/internal/releases/<version>-<codename>/`.
