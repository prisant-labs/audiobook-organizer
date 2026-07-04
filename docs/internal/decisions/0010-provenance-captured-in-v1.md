---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 10. Pack and Award Provenance Is Captured in v1

## Context and Problem Statement

The library's award-collection folders (Hugo, Nebula, Top 100, Dune Universe, and similar packs) record which books belong to which curated set. The original plan deferred capturing this provenance to v1.1, while the flatten operation that dissolves those pack folders into individual book folders runs in v1. This is a destroyed-before-feature-exists hazard: once flatten runs, the pack membership information is gone from the folder structure, and no later feature can recover it.

## Considered Options

- Defer provenance capture to v1.1 as originally planned, accepting that pack membership is lost once v1's flatten runs.
- **Capture provenance as durable data at plan/flatten time in v1, with an exported provenance report; defer only the ABS-side push of that data (chosen).**

## Decision Outcome

Chosen: **F-507 (pack provenance capture and report)**, epic E-05, P0 (D-14 (provenance in v1) and FD-01 (F-507 pack provenance), 2026-07-03). In v0.3.0 (planning), the plan builder records source-pack membership per book in `plan_ops`, and a provenance report exports beside the plan. In v0.5.0 (acting), the journal and manifest carry provenance, and the report is re-emitted post-apply. Pack shells (the emptied collection folders after successful extraction) go to quarantine by default, with a policy toggle to leave them in place. Only the ABS-side push (pushing provenance into ABS collections via its API) stays deferred to v1.1+ as F-1102.

### Consequences

- Good, because it closes the audit's Stream 1, finding 1 (CRITICAL): provenance is no longer destroyed before the feature that would consume it exists.
- Good, because splitting "capture data now" from "push to ABS later" means the irreversible step (flatten) never runs ahead of the data-preservation step, while still deferring the lower-value integration work.
- Good, because the provenance report gives jp a durable, human-readable record of which books came from which award collection even without ever building the ABS push.
- Bad, because it adds schema and plan-builder scope to v0.3.0 (planning) that the original plan had pushed to v1.1, increasing that release's surface area.
- Neutral, because the pack-shell quarantine default is consistent with 0004 (safety invariants): shells are set aside, not deleted, matching the quarantine-only invariant.
