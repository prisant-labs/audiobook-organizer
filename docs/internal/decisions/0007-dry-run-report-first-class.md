---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 7. Dry-Run HTML Report Is a First-Class, P0 Deliverable

## Context and Problem Statement

The strategy brief's amendment (2026-07-03) resolved the milestone posture as an early mini-campaign, with a hard requirement: a fully functional dry run must produce both a browsable confirmation screen and an exportable, self-contained HTML report, before anything executes against the real library. Without this, the first time a family member (or jp) sees the full consequences of a plan could be after it has already run.

## Considered Options

- Ship dry-run only as an in-app preview screen (F-903 (plan preview surface)), with report export as a later, lower-priority feature.
- **Elevate the dry-run HTML report to a P0, standalone, exportable deliverable, gating the mini-campaign (chosen).**

## Decision Outcome

Chosen: **F-506 (dry-run HTML report)** is P0 and lands in v0.3.0 (planning), ahead of any GUI release (D-04 (milestone posture), 2026-07-03). The report must be self-contained (opens with no network access, per FD-11 (zero-network fonts and assets)), include the full change-list table with no row cap, use a report-only single light paper theme distinct from the app's Day/Evening themes, and carry the FD-10 (deletion guarantee copy) guarantee block verbatim (FD-28 (report format spec)). The v0.3.0 gate explicitly requires the report to pass a non-engineer read test: a family member can say what would happen and what would not, from the report alone.

### Consequences

- Good, because the report becomes a genuinely valuable standalone artifact even if the project stopped at v0.3.0: a reviewed, exported reorganization plan a human can read without launching the app.
- Good, because it is the concrete vehicle for D-03 (family tier sets the UI bar): the report, not just the in-app preview, must clear the plain-language bar.
- Good, because building it in v0.3.0, before the GUI (v0.4.0 (seeing)), forces the plan data model and campaign-group taxonomy to be complete and stable early.
- Bad, because it adds report-specific design and copy work (a distinct theme, print rules, embedded fonts) on top of the in-app preview, rather than reusing one surface for both.
- Neutral, because the report format is re-emitted post-apply per FD-01 (pack provenance capture), so its schema must anticipate the provenance columns before they exist.
