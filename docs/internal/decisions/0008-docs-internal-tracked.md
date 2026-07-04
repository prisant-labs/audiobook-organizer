---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 8. docs/internal/ Is Tracked in Git

## Context and Problem Statement

The release plan (`_local\planning\release-plan-and-ci_2026-07-02.md`, Section 2) states that "`_local/` and any future `docs/internal/` stay quarantined from public history via `.gitignore` before any public flip." This contradicts the repo-sync-tool convention, where `docs/internal/` is deliberately tracked in git (it holds architecture decisions, release plans, and governance documents meant to travel with the repo, distinct from `_local/`, which is genuinely per-machine scratch).

## Considered Options

- Follow the release plan's Section 2 line literally: gitignore `docs/internal/` alongside `_local/` until any public flip.
- **Follow repo-sync-tool convention: `docs/internal/` is tracked from day one; only `_local/`, `.memsearch/`, and tool caches are gitignored (chosen).**

## Decision Outcome

Chosen: **`docs/internal/` is tracked in git** (D-12 (docs/internal tracked), 2026-07-03), matching the repo-sync convention. This corrects and supersedes the release plan's Section 2 line. Only `_local/`, `.memsearch/`, and tool caches are gitignored. Both are private-repo directories today; the distinction is not about public visibility now but about what travels with the repo's real history (`docs/internal/`) versus per-machine scratch that must never be assumed present on another machine (`_local/`).

### Consequences

- Good, because it conforms to the repo-sync-tool template exactly, avoiding a second, divergent convention across the two projects.
- Good, because ADRs, release plans, and specs in `docs/internal/` become part of the repo's durable, cross-machine history immediately, rather than living only where the current agent session happens to run.
- Good, because it resolves the audit's Stream 3 finding 1 (docs/internal quarantine line contradicts repo-sync tracked convention) cleanly, by picking the more conservative (tracked, reviewable) option.
- Bad, because `docs/internal/` content must therefore be written with the assumption it may eventually go public at the D-13 (OSS posture) flip; nothing sensitive can be casually dropped there.
- Neutral, because this decision does not itself decide the public flip (D-13); it only fixes what is tracked while the repo remains private.
