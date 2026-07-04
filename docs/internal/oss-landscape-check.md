---
title: "OSS landscape check (FD-15 pre-flight)"
date: 2026-07-03
status: recorded
owner: jprisant
produced-by: "research agent (Opus, timeboxed), reviewed by Fable"
gate: "v0.1.0-spine AC-1 / G-7; decision gate result: proceed (no tool materially subsumes the plan)"
---

# OSS landscape check - 2026-07-03 (FD-15, v0.1.0 pre-flight)

Timeboxed survey (one focused hour) answering: does an existing tool already do classify, plan, preview, apply, rollback over audiobook folder STRUCTURE, safely enough for a non-technical user, well enough that building is unjustified?

## Verdict: BUILD JUSTIFIED (narrowly)

No single existing tool delivers the full loop for the target user: classify to plan to preview to apply to rollback over folder structure, packaged as a native Windows desktop app safe for a non-technical person. The two closest projects each cover much of the mechanics but miss the combination that defines this product: structure-only scope, structural-problem classification (award-pack nesting, multi-book folders, loose files), quarantine-instead-of-delete, an append-only journal with exportable manifest, the exportable dry-run HTML report, and a double-click desktop app. The strongest competitors are developer-oriented (CLI, Docker/web, beta). The gap is real but thin: differentiation leans on safety semantics and non-technical UX, not on "nobody organizes audiobooks."

Reviewed by Fable against the Phase 0 decision gate: nothing subsumes the plan; proceed to scaffold. jp can veto at any release report (D-10, non-blocking reports).

## Candidates examined (build / adapt / ignore per tool)

Legend: (a) structural-problem classification, (b) plan/preview before touching files, (c) rollback/undo manifest, (d) non-technical safety.

| Tool | What it does | a | b | c | d | Conclusion |
|---|---|---|---|---|---|---|
| jeeftor/audiobook-organizer (Go; github.com/jeeftor/audiobook-organizer) | Structure-first Author/Series/Title; CLI + web + TUI | partial | yes (--dry-run) | yes (.abook-org.log + --undo) | partial | Closest competitor; borrow journal-file precedent; do not adopt (no classification, no quarantine, no desktop app) |
| deucebucket/library-manager (Py/Docker) | 4-layer AI+API classify, web dashboard, moves folders | yes | yes (approval queue) | yes (per-rename undo) | no | Borrow approval-queue + drastic-change protection ideas; do not adopt (Docker/web, beta, API keys, AGPL) |
| beets-audible (Neurrone; seanap config) | Tag-first metadata via Audible/Audnex; path templates | no | pretend only (-p) | no | no | Ignore for structure work (tag-first, expert tool, no undo) |
| Advanced Renamer (commercial, Windows) | General batch rename/move to folder patterns | no | yes (preview) | yes (Undo Batch) | no | Borrow the Undo Batch UX affordance; not an audiobook solution |
| BadaBoomBooks (Py) | Browser-assisted move to {author}/{title}, writes .opf | no | no | copy-flag only | no | Ignore; note copy-instead-of-move as a paranoia option |
| zettaivanugen/audiobook-organizer (Py) | Auto Author/Series/Title, series DB, online lookups | partial | no | no | no | Ignore (script, no safety model) |
| presswizards/abs-renamer "Shelfarr" | Template rename preview against an ABS library | no | yes (preview) | no | no | Ignore; confirms ABS-community demand |
| Assorted scripts (lincolnep, tHeCh0s3n0n3, fxsth, jamesbrindle) | Rename/move scripts, some tag/convert | no | mostly no | no | no | Ignore |

## Worth borrowing (design inputs for later releases)

- library-manager: pending-approvals queue and "drastic change protection" gating high-risk moves for human review; maps to the F-502 (campaign group review) confirm flow.
- jeeftor/audiobook-organizer: per-operation journal files driving an undo command; lightweight precedent validating the F-602 (journal + undo manifest) shape.
- Advanced Renamer: the Undo Batch window and verify-before-execute preview are UX affordances non-technical users already trust; echoes F-501/F-603 surfaces.
- BadaBoomBooks: copy-instead-of-move as a paranoia switch; consider as a future policy toggle alongside quarantine (not v1 scope).

Nobody in this set offers quarantine-instead-of-delete or an exportable dry-run report; both are genuine differentiators, not table stakes.
