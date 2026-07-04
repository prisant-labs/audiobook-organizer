---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 9. Effort Unit Equals Release

## Context and Problem Statement

The feature-function breakdown uses epic identifiers E-01 through E-11 as a feature taxonomy. Repo-sync-tool's governance model also uses an "effort" identifier namespace for tracked units of planning and execution work. If audiobook-organizer's epics (E-01..E-11) were also used as effort IDs for governance purposes (release folders, tracked work items), they would collide with repo-sync-tool's own effort namespace, since both projects share reviewers, conventions, and potentially cross-references.

## Considered Options

- Use the epic IDs (E-01..E-11) directly as governance effort IDs, matching how the feature-function breakdown already organizes features.
- **Define the effort unit as the release itself, with epics remaining a features-only taxonomy inside the PRD and breakdown (chosen).**

## Decision Outcome

Chosen: **effort unit = release** (FD-16 (effort equals release), 2026-07-03). Each release gets one tracked folder, `docs/internal/releases/<version>-<codename>/`, containing `spec.md` and `implementation-plan.md`. Epics E-01 through E-11 remain a taxonomy inside the PRD and feature-function breakdown only; no E-NN effort IDs are used for governance, avoiding any collision with the repo-sync effort namespace.

### Consequences

- Good, because it resolves the audit's Stream 3 finding 3 (E-NN effort/epic namespace collision) without renumbering the epics that the breakdown and release plan already agree on.
- Good, because "one release, one folder" gives a single, unambiguous unit of tracked work that maps directly onto the release ladder (v0.1.0 through v1.0.0) already defined in the release plan.
- Good, because epics stay useful purely as a feature-organizing lens (what F-xxx features belong to what area of the product) without being overloaded as a second governance axis.
- Bad, because a release folder can span many epics at once (for example v0.4.0 (seeing) touches F-901, F-902, F-903, F-906, F-803 across multiple epics), so cross-epic traceability requires reading the feature IDs inside each release's spec, not the folder name alone.
- Neutral, because this decision does not change any F-xxx feature ID, priority, or release assignment; those are already cross-checked consistent (audit Stream 5, finding 1).
