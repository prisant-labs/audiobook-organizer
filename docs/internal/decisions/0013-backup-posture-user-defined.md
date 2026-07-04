---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 13. Pre-Campaign Backup Posture Is User-Defined

## Context and Problem Statement

Before the M-1 (campaign) milestone can run a Real apply against the actual 297 GB library, some backup posture needs to exist so that a catastrophic, unforeseen failure is recoverable outside the tool's own journal and rollback mechanisms. Free space (1.3 TB against a 297 GB library) makes a full external or same-drive copy feasible, but not free: it costs hours of I/O and a meaningful chunk of remaining headroom. No single backup posture is obviously correct for every situation the campaign might run in.

## Considered Options

- Mandate a full external-drive copy before any Real apply, unconditionally.
- Mandate same-drive copy plus the tool's own quarantine and journal, unconditionally.
- Rely on manifests and quarantine alone, with no separate backup copy, unconditionally.
- **Present all three options with their trade-offs and let the user choose at campaign time (chosen).**

## Decision Outcome

Chosen: **backup posture is user-defined** (D-17 (backup posture user-defined), 2026-07-03). The product and the M-1 (campaign) runbook present the options (external-drive copy, same-drive copy, manifests-plus-quarantine only) with trade-offs, and the user chooses at campaign time. The M-1 (campaign) gate stays open until a choice is recorded. Nothing Real runs without a recorded backup decision.

### Consequences

- Good, because it respects that the right backup posture depends on circumstances only the user can weigh at campaign time (available external media, time budget, risk tolerance), rather than the tool guessing wrong for everyone.
- Good, because "the gate stays open until a choice is recorded" makes the decision an explicit, auditable checkpoint rather than an implicit assumption that could be skipped under time pressure.
- Good, because it is consistent with the human-only gate for any Real apply (D-10 (scope of go)): the backup decision and the apply-approval decision sit together at the same human checkpoint.
- Bad, because it means the tool cannot fully automate the campaign start; a human must actively record a choice before M-1 (campaign) can proceed, which is friction by design.
- Neutral, because this decision does not pick a default; if the user is silent, the correct behavior is to block, not to assume the lightest-weight option.
