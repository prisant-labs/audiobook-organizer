---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 11. Per-Group Cards and the Report Are the P0 Review Surface, Not the Tree Diff

## Context and Problem Statement

The feature-function breakdown originally defined F-501 (before/after tree diff) as P0. The current prototypes (`_local\gui\05-review.html`, `_local\gui\07-complete-flow.html`) instead show a review surface built from per-group cards with curated examples, paired with the full HTML report. This is a direct contradiction between the formal feature definition and the design-contract-in-progress prototypes: one names the exhaustive tree as P0, the other builds and demonstrates cards plus report.

## Considered Options

- Hold F-501 (before/after tree diff) as originally defined: an exhaustive tree/everything view, P0, built for v0.4.0 (seeing).
- **Redefine the P0 review surface as per-group cards with curated examples, plus the full HTML report; demote the exhaustive tree/everything view to P1 (chosen).**

## Decision Outcome

Chosen: **per-group cards plus report is the P0 product** (D-16 (review surface), 2026-07-03), matching prototypes 05 and 07. F-501 is redefined (FD-06 (F-501 redefined), 2026-07-03) as a virtualized full change list (grouped, tree presentation optional), demoted to P1, tier-1 disclosure surface, landing in v0.6.0 (hardening) rather than v0.4.0. The prior F-501 P0 tree-diff definition is superseded.

### Consequences

- Good, because it resolves the audit's Stream 2, finding 3 (IMPORTANT): the P0 review surface now matches what the prototypes actually demonstrate and what the design contract (PRODUCT.md) implies for the family tier.
- Good, because per-group cards align with the seven campaign groups (FD-26 (campaign group canon)) that the rest of the product organizes around, giving one consistent mental model from review through the report.
- Good, because it defers the harder-to-get-right exhaustive tree/list view to v0.6.0 (hardening), after the approval workflow (the load-bearing feature per 0002 (engine-first build order)'s v0.4.0 descope trigger) has shipped and been used.
- Bad, because power users (jp) who want to audit every single planned operation in one exhaustive view will not have that surface until v0.6.0 (hardening); until then, the full detail lives in the exported report and per-group drill-down only.
- Neutral, because the underlying plan data model is unaffected; this decision changes only which view renders first, not what data the plan builder produces.
