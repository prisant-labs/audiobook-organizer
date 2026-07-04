---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 6. Family Tier Sets the UI Bar

## Context and Problem Statement

The tool has three plausible audiences: jp, household non-engineer family members (the strategy brief notes a non-technical member of the household's empty Audible profile folder as evidence family usage is at least contemplated), and eventual public users. These tiers have very different tolerance for technical vocabulary, raw paths, and exit-code-style feedback. The UI needed one bar to design to, not three competing ones.

## Considered Options

- Design primarily for jp (the power user), with simplification as a later pass if the tool is ever shared.
- Design primarily for the public/OSS audience, deferring family use as a side effect.
- **Design for the household non-engineer tier as the primary bar, with technical detail available on demand (chosen).**

## Decision Outcome

Chosen: **all three tiers are served, but Tier 2 (family) sets the UI bar** (D-03 (audience and UI bar), 2026-07-03). No paths, exit codes, or jargon appear as the primary interface; technical truth lives behind one "Show file details" disclosure. This carries a plain-language register requirement into every user-facing surface: books, shelves, copies, tidy-up, set aside are the vocabulary; operations, ops, dedupe, manifest, quarantine, and dashboard are banned from UI copy (a standing rule of the suite; see docs/internal/decision-ledger.md).

### Consequences

- Good, because a single disclosure pattern ("Show file details") gives jp and future power users the technical detail they want without a second UI to maintain.
- Good, because it forces every error, empty, and loading state (F-908 (error, empty, and loading states)) to be authored in plain language from the start, rather than retrofitted.
- Good, because it directly informs FD-13 (raw paths policy): the only allowed raw path on a primary surface is the ABS setup path on the Done screen, since Tier 2 needs it to configure ABS.
- Bad, because it constrains copy choices jp personally might prefer as a power user (for example, a raw path on the scanning progress line) in favor of a friendlier location name.
- Neutral, because it does not change engine behavior at all; it is purely a presentation-layer and copy-register constraint enforced at the GUI and report layers.
