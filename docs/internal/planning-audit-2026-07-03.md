---
title: "Planning Audit - 2026-07-03 (findings and dispositions)"
date: 2026-07-03
status: review
owner: jprisant
produced-by: "five-stream audit workflow (Opus/Sonnet), consolidated by Fable"
---

# Planning Audit - 2026-07-03 (findings and dispositions)

Five parallel auditors examined discovery docs, prototypes, the reference architecture, session history, and cross-consistency before the planning suite was authored. Every finding below carries its disposition into the suite: the D-nn or FD-nn that resolves it (see docs/internal/decision-ledger.md), or the artifact that must address it. Authors of each suite document are responsible for addressing every finding assigned to their artifact.

## Stream 1: discovery vs planning

1. [CRITICAL] Pack/award provenance destroyed at flatten time; capture was deferred to v1.1 while flatten runs in v1. DISPOSITION: D-14 + FD-01 (F-507). Lands in v0.3.0 and v0.5.0 specs, PRD, architecture (schema), report spec.
2. [IMPORTANT] Prior-work files lived only on the drive the tool reorganizes. DISPOSITION: rescued to _local\prior-work\ on 2026-07-03. PRD cites folder-structure.md as the historical naming preference source.
3. [IMPORTANT] Tag-quality probe (folder-first assumption unmeasured) never scheduled. DISPOSITION: FD-14; v0.2.0 spec gate + PRD statement.
4. [MINOR] Video/course content (52 Sales Lessons mp4s, radio plays, cbr comics) typed as audio. DISPOSITION: FD-17; v0.2.0 spec (F-103/F-201), PRD non-goals.
5. [MINOR] Codex ABS-item baselines (582/11/831) dropped from gates. DISPOSITION: FD-18; v0.2.0 spec gate.
6. [MINOR] Stale 2026-03-25 snapshot handling fine; label baselines. DISPOSITION: FD-18 labeling rule.
7. [MINOR] Count discrepancies (13,970 files / 718 vs 719 folders; dupes ~3 GB vs ~10.08 GB) unreconciled in prose. DISPOSITION: PRD adds a one-line reconciliation; dupes stay "unknown until measured".
8. [MINOR] Discovery settings dropped without note (cover.jpg standardization, maxBatchSize, intake/process/archive roots, yearPosition, archived pack shell destination). DISPOSITION: PRD records intentional cuts; FD-01 fixes pack-shell destination (quarantine, toggle leave-in-place); yearPosition stays implicit in presets.
9. [MINOR] preferSource default flipped (tags to folder-first). DISPOSITION: FD-14; PRD states the supersession explicitly.
10. [MINOR] "Fix this folder" focused workflow dropped. DISPOSITION: PRD notes campaign groups supersede per-folder flow in v1; candidate for v1.x with intake mode (F-1105).
11. [MINOR] OSS-landscape timeboxed check untracked. DISPOSITION: FD-15; roadmap pre-v0.1.0 checklist + EXECUTION.md.

## Stream 2: prototypes as design contract

1. [CRITICAL] Zero error/failure states designed anywhere. DISPOSITION: FD-04 (F-908); design-system doc defines the surfaces; v0.4.0 spec carries AC.
2. [IMPORTANT] No first-run/settings/ruleset surface; Settings is a dead link. DISPOSITION: FD-05 (F-909); v0.4.0 spec.
3. [IMPORTANT] F-501 P0 tree diff contradicts the cards+report design. DISPOSITION: D-16 + FD-06; PRD, roadmap, v0.4.0 and v0.6.0 specs.
4. [IMPORTANT] "Pause between books" has no backing feature. DISPOSITION: FD-02 (F-608) + real Stop controls; v0.5.0 spec (pause), v0.4.0 spec (stop affordances).
5. [IMPORTANT] "The old genre view lives on as tags" promises a non-goal. DISPOSITION: FD-12; design-system copy register, v0.4.0 spec.
6. [IMPORTANT] Empty/loading states undefined (already-tidy, empty library, all-excluded, plan-building, re-scan). DISPOSITION: FD-04; design-system + v0.4.0 spec.
7. [IMPORTANT] Deletion-guarantee copy overclaims ("no delete anywhere" vs 20 rmdir-empty ops). DISPOSITION: FD-10 canon copy; design-system, report spec, v0.4.0 spec.
8. [IMPORTANT] Duplicates unit ambiguity (groups/pairs/copies). DISPOSITION: FD-08; design-system, v0.6.0 spec, report spec.
9. [IMPORTANT] Cover-forward UI depends on deferred F-1101 subset. DISPOSITION: D-15 + FD-03 (F-907); v0.4.0 spec.
10. [IMPORTANT] Google Fonts <link> violates zero-network. DISPOSITION: FD-11; architecture, ci-plan (grep gate), design-system, report spec.
11. [IMPORTANT] No cancel control on scan/tidy progress screens. DISPOSITION: FD-02; v0.4.0 spec AC.
12. [IMPORTANT] F-902 "dashboard" label contradicts anti-reference. DISPOSITION: FD-07 rename "library home"; PRD, v0.4.0 spec.
13. [MINOR] Campaign-group set mismatch (7 in UI vs 8 in F-403). DISPOSITION: FD-26; v0.3.0 spec, design-system.
14. [MINOR] "Show file details" lacks pattern/confidence content (F-504). DISPOSITION: FD-13; v0.4.0 spec.
15. [MINOR] Per-op exclude (F-502), search/filter (F-503), dupes override (F-702) have no prototyped affordance. DISPOSITION: v0.4.0/v0.6.0 spec authors define minimal affordances consistent with the design system; per-op exclude lands inside the group detail, search via a simple filter box, dupes override = explicit warning confirm.
16. [MINOR] Theme id vocabulary triple (Day/Evening vs calm/evening vs prose). DISPOSITION: FD-09; design-system.
17. [MINOR] Raw paths on primary surfaces (scan line, done card). DISPOSITION: FD-13.
18. [MINOR] Demo numbers internally inconsistent on 04 home (1,022 vs 994). DISPOSITION: FD-27 sample-data rule.
19. [MINOR] --ink-3 contrast likely borderline. DISPOSITION: FD-21; design-system.
20. [MINOR] Report promises a complete change list it does not include; report type system diverges from app. DISPOSITION: FD-28; F-506 report spec.
21. [MINOR] Negated "deleted" wording policy. DISPOSITION: FD-10.

## Stream 3: reference architecture conformance

1. [IMPORTANT] docs/internal quarantine line contradicts repo-sync tracked convention. DISPOSITION: D-12; roadmap notes supersession; EXECUTION.md states tracked docs/internal.
2. [IMPORTANT] Release-governance scaffolding missing (runbook_cut-tag-release, release-checklist.yaml, program-roadmap, release folders). DISPOSITION: governance batch + roadmap + FD-16 release folders; six-gate G0-G4 ceremony adapted.
3. [IMPORTANT] E-NN effort/epic namespace collision. DISPOSITION: FD-16 (effort = release; epics stay taxonomy).
4. [IMPORTANT] Tauri capability/security model omitted. DISPOSITION: FD-29; architecture doc section + v0.1.0 spec AC.
5. [IMPORTANT] v0.1.0 hygiene set incomplete (.gitattributes, bump-version, LICENSE, templates, CHANGELOG). DISPOSITION: FD-25; v0.1.0 spec + hygiene batch (now: .gitignore/.gitattributes).
6. [IMPORTANT] F-501's "repo-sync design token discipline" line contradicts PRODUCT.md anti-reference. DISPOSITION: design-system doc owns the token canon (mechanism shared, visual language NOT inherited); PRD clarifies stack directive covers architecture, not look.
7. [MINOR] Bindings-drift gate platform placement (ubuntu vs windows). DISPOSITION: FD-24; ci-plan.
8. [MINOR] CI concurrency/permissions/LTO profiles missing. DISPOSITION: FD-24; ci-plan.
9. [MINOR] EXECUTION.md standing rules completeness (no-dash rule, Co-Authored-By, Apple enrollment in allowlist). DISPOSITION: EXECUTION.md brief.
10. [MINOR] WebKitGTK smoke non-adoption undocumented. DISPOSITION: FD-24; ci-plan records deliberate non-adoption.

## Stream 4: session history and hygiene

1. [IMPORTANT] .memsearch/ untracked and not ignored; would be swept by git add -A. DISPOSITION: hygiene batch .gitignore.
2. [IMPORTANT] No project .gitignore exists; _local/ only hidden by jp's personal global excludesfile. DISPOSITION: hygiene batch .gitignore (must work on any machine).
3. [MINOR] Square-cover (1:1) correction easy to miss. DISPOSITION: D-06 records it; design-system carries it as AC; series spine clusters keep stylized spines deliberately.
4. [MINOR] This session needs its own wrap-up log at the end. DISPOSITION: orchestrator runs /jp-wrap-session at session end.

## Stream 5: cross-consistency (orchestrator-verified)

1. Feature-ID cross-check between the breakdown and release plan: ALL F-xxx IDs, priorities, and release assignments agree. No mismatches. New IDs introduced by this suite: F-507, F-608, F-907, F-908, F-909 (see FD-01..FD-05); F-501 redefined (FD-06); F-902 renamed (FD-07).
2. Release-gate consistency with PRODUCT.md promises: gates cover undo (rollback round-trip), quarantine-only, HTML report readable by a non-engineer. Gaps closed by this suite: WCAG AA verification method (FD-21), Day/Evening theme AC (FD-09), plain-language register enforcement (design-system copy canon).
