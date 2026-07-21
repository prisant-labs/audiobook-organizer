---
id: v0.4.0-seeing
title: "Release v0.4.0 (seeing) - GUI over the frozen seam"
type: spec
date: 2026-07-03
status: review
owner: jprisant
tier: release
scope: GUI surfaces (E-09) rendering data the engine already produces; no new engine capability
depends_on: v0.3.0-planning
produced-by: release-author agent (Fable planning suite)
ac-count: 41
source-count: 12
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - _local/planning/audiobook-organizer-strategy-brief_2026-07-02.md
  - PRODUCT.md
  - _local/gui/04-library.html
  - _local/gui/05-review.html
  - _local/gui/07-complete-flow.html
  - _local/gui/06-dryrun-report.html
  - docs/internal/design-system.md
  - docs/internal/test-strategy.md
  - docs/internal/decision-ledger.md (D-02..D-16, FD-02..FD-30)
  - docs/internal/planning-audit-2026-07-03.md stream 2 (prototypes as design contract)
---

# Spec: Release v0.4.0 (seeing) - GUI over the frozen seam

## Task Summary

- Status: built; gate walked; G-1 human review loop pending jp; tag awaiting jp per D-10.
- Release theme: the first real screens. v0.4.0 (seeing) turns the headless engine of v0.3.0 (planning) into the five product surfaces a human touches, rendering data the engine already produces and adding zero new engine capability.
- Load-bearing outcome: jp performs a full review-and-approve loop in the app against a real-library plan, then exports.
- AC total: 41 across nine features. All unchecked at review time.
- Open questions: 3 (see Open Questions).
- Last updated: 2026-07-03

## Purpose

The catastrophic risk in this product is the file mover, not the screens, so the engine hardened first (D-07, engine-first order). By v0.3.0 (planning) the tool can already produce a deterministic, validated plan grouped into campaign groups, export it, and emit a self-contained HTML dry-run report [S-1 release plan v0.3.0]. v0.4.0 (seeing) is the release where that machinery becomes visible and operable to a non-engineer: a library home that reads like sentences, a review surface that lets the user include or skip work group by group, and the supporting shell, settings, first-run, and the error/empty/loading states the prototypes never drew. The design bar is set by tier 2 (household members): if a non-technical member of the household could not confidently review and confirm a tidy-up, the surface is wrong [S-4 PRODUCT.md].

## Context

This release consumes the frozen tauri-specta IPC contract and the engine outputs from v0.3.0 (planning): `classify_overview` health metrics (F-202), the campaign-grouped plan from the plan builder (F-403), plan validation states (F-404), plan export (F-505), the dry-run HTML report (F-506, D-04), and duplicate candidates (F-701) [S-2 feature breakdown Section 6]. It renders that data; it does not compute new engine results. The GUI renders a frozen contract (D-07): the frontend never touches the filesystem and issues no raw `invoke` (FD-29). The tracer-bullet throwaway UI from v0.1.0 (spine) is deleted in this release [S-1 release plan v0.4.0].

Design authority for every surface is PRODUCT.md plus docs/internal/design-system.md; the current-direction prototypes `_local/gui/04-library.html`, `05-review.html`, and `07-complete-flow.html` are the shape reference, and set 1 (`01-03`) is a recorded anti-reference (D-06). Prototype numbers are sample data and are never hardcoded (FD-27).

## Users / Actors

- Tier 1 (jp): runs campaigns; can open the extended "Show file details" disclosure for technical truth (paths, matched pattern, confidence) [S-4 PRODUCT.md; FD-13].
- Tier 2 (household members): the design bar; never shown a raw path, exit code, or jargon term as the primary interface [D-03].
- Tier 3 (eventual public users): inherit the same family-safe surfaces.

## Scope (in-scope features with acceptance criteria)

### F-901 (app shell and navigation) - P0

The window frame every surface lives in: a custom titlebar, a left sidebar, and the Day/Evening theme system. Command palette is P1 and may descope.

Key behaviors: custom titlebar carries the app name, a Day/Evening segmented control, and window caption controls (minimize, maximize, close). The sidebar links Library, Tidy-up, Duplicates, History, Settings, with the active route marked `aria-current`. Theme is a data attribute on the document root. Nav count badges count the canonical unit: the Duplicates badge counts duplicate GROUPS, never member copies (FD-08).

Edge cases: the "Skip ahead" links used in the walkthrough prototype are demo-only and never ship (FD-02). The word "dashboard" appears in no route name, label, or code identifier on a user-facing surface (FD-07).

Acceptance criteria:
- [x] AC-1 The app launches to a shell with a custom titlebar (app name, Day/Evening control, window controls) and a left sidebar linking Library, Tidy-up, Duplicates, History, Settings; the active route carries `aria-current`. [S-5 prototype 04; S-2 F-901]
- [x] AC-2 The theme toggle sets `data-theme` to exactly `day` or `evening` (the identifiers `calm` and `prose` appear nowhere in shipped code); Day is the default on first run; the choice persists across restarts via settings (F-803). [FD-09; S-6 prototype 05]
- [x] AC-3 Sidebar count badges label groups, not copies: the Duplicates badge shows a group count; no user-facing surface, route name, or identifier contains the word "dashboard". [FD-08; FD-07]
- [x] AC-4 With `prefers-reduced-motion: reduce`, all view transitions are instant or crossfade only. [S-4 PRODUCT.md accessibility]
- [x] AC-5 The frontend contains no raw `invoke` call; all IPC goes through generated tauri-specta bindings, enforced by a lint gate. [FD-29; S-1 release plan v0.4.0 gate]

### F-902 (library home) - P0

Renamed from "dashboard" per FD-07. The warm, cover-forward home: health facts live inside sentences, and there is exactly one primary action ("Start a tidy-up"). No stat bands, no hero-metric tiles, no uppercase tracked eyebrow labels (D-06 anti-reference).

Key behaviors: a lede sentence states library size and how many books could use a tidy, in prose. A "Worth a look first" shelf shows example covers with plain-language chips ("loose file", "messy name", "N books, 1 folder", "N copies"). Series spine clusters keep the stylized spine metaphor deliberately (D-06). A good-news line lists what is already tidy. All numbers derive from `classify_overview` health metrics (F-202), never from hardcoded prototype values (FD-27).

Edge cases: audiobook covers render square 1:1, never 2:3 portrait, and never with fake spine-edge shading on the flat cover (D-06). When the library is already tidy or empty, the home shows the corresponding F-908 empty state instead of an example shelf.

Acceptance criteria:
- [x] AC-6 Library home renders health facts inside prose sentences and presents exactly one primary action; no stat band, hero-metric tile, or uppercase tracked eyebrow label is present. [FD-07; D-06; S-4 PRODUCT.md]
- [x] AC-7 Every count and byte figure on the home is read from `classify_overview` (F-202) at render time; no sample/prototype number is hardcoded. [FD-27; S-2 F-202]
- [x] AC-8 Book covers render at a 1:1 aspect ratio with no fake spine-edge shading; series spine clusters retain the stylized spine metaphor. [D-06; S-5 prototype 04]
- [x] AC-9 The reassurance line uses the FD-10 deletion-guarantee copy verbatim: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone." [FD-10]

Normative (contrast): numeric count badges and meta counts on the home are information-bearing; they may use `--ink-3` only on surfaces where the measured token pair meets WCAG AA 4.5:1 per design-system Section 8, otherwise they are promoted to `--ink-2`. [FD-21; design-system Section 8]

### F-903 (plan review surface) - P0

The trust ceremony surface. Hosts F-502, F-503, and F-504. It renders the campaign-grouped plan from v0.3.0 (planning) as a two-pane review: groups on the left, per-group curated examples on the right. Per D-16, the per-group cards plus the full HTML report ARE the P0 product; the exhaustive everything view (F-501) is explicitly not in this release.

#### F-502 (campaign group review) - P0

The campaign groups from the plan builder (F-403) are the approval unit. The seven user-facing groups (FD-26: staging, loose books, messy names, box sets, bundles, copies, empty folders) each render as a card with an include/skip switch, a plain-language reason, and a change count.

Key behaviors: toggling a group's switch includes or excludes it and updates the footer totals live. Committing the tidy-up uses a two-step inline confirm (the "Tidy up now" action asks for a second confirmation inline before anything is scheduled). Within a group's detail, an individual operation can be excluded (dropped to `no-op(user-excluded)`) via `plan_exclude_op` (per-op exclude lives inside the group detail, per planning audit stream 2 item 15). Footer counts state which quantity they report (groups included; total changes) and never mix "copies/pairs/groups" language (FD-08).

Edge cases: a blocked operation (F-404 validation `blocked`) cannot be included; it can only be excluded or fixed upstream by editing the ruleset and regenerating (F-906). A group whose members are all blocked or all user-excluded renders in the blocked/empty group state defined by F-908. The copies group renders whatever state the engine reports (candidates from F-701); hash-verified resolution and the dupes override are v0.6.0 (hardening), so any destructive-adjacent override here is an explicit warning confirm.

Acceptance criteria:
- [x] AC-10 The review surface renders exactly the seven canonical campaign groups (FD-26) as cards, each with an include/skip switch, a plain-language reason sentence, and a change count. [FD-26; S-6 prototype 05; S-2 F-502]
- [x] AC-11 Toggling a group's include/skip switch updates the footer totals live and states which quantity each total reports (groups included, total changes); no total mixes copies/pairs/groups language. [FD-08; S-6 prototype 05]
- [x] AC-12 "Tidy up now" requires a two-step inline confirmation before any Real apply is scheduled; the first press reveals the confirm, the second commits. [S-7 prototype 07; brief]
- [x] AC-13 Inside a group's detail, an individual operation can be excluded via `plan_exclude_op`, dropping it to `no-op(user-excluded)`; the change persists on the plan. [S-2 F-502, IPC `plan_exclude_op`; planning audit stream 2 item 15]
- [x] AC-14 A `blocked` operation cannot be set to included; the UI offers only exclude or a pointer to fix the ruleset and regenerate. [S-2 F-404, F-502]
- [x] AC-15 When all members of a group are blocked or excluded, the group renders the F-908 blocked/empty group state rather than an empty card. [FD-04]

#### F-503 (search and filter) - P1

A simple filter box over the plan, descoped from the prototype's command-palette ambition per the brief. Filters by campaign group, folder class, confidence, and warning type, plus free-text over source/target names.

Key behaviors: typing in the filter box narrows the visible plan rows; clearing restores the full plan. Filtering is a view concern only and never changes approval state.

Acceptance criteria:
- [x] AC-16 A single filter box narrows the plan by free-text match over source/target names and by group, class, confidence, and warning-type facets; clearing it restores the full plan. [S-2 F-503]
- [x] AC-17 Filtering never changes any operation's approval or exclude state (view-only). [model-inference from F-503 intent]

#### F-504 (explainability) - P0

Every operation can show its rationale through the extended "Show file details" disclosure. Per FD-13, this disclosure is the single consistent place technical truth lives, and for tier 1 it is extended beyond raw paths to include the matched pattern and confidence.

Key behaviors: the disclosure reveals the raw source and target paths (Windows paths), the matched pattern id and human name, the extracted fields (title, author, series, index, year, narrator), the per-field confidence, and the noise that was stripped. Rendered from the plan operation's rationale (F-403) and parse output (F-303).

Edge cases: raw paths appear ONLY inside this disclosure; the single primary-surface exception is the ABS setup path on the Done next-step card, which is a v0.5.0 (acting) surface, not this release (FD-13). Low-confidence fields are shown, not hidden (F-303).

Acceptance criteria:
- [x] AC-18 Each operation's "Show file details" disclosure reveals raw source and target paths, the matched pattern (id plus human name), extracted fields with per-field confidence, and the stripped noise. [FD-13; S-2 F-504, F-303; S-6 prototype 05]
- [x] AC-19 Raw paths appear only inside the "Show file details" disclosure on this release's surfaces; no raw path is shown on a primary review surface outside it. [FD-13]
- [x] AC-20 Low-confidence extracted fields are displayed (not suppressed) so they can be reviewed. [S-2 F-303]

### F-907 (cover extraction and fallback tiles) - P0 (NEW)

Implements D-15 via FD-03. A read-only `lofty` subset reads embedded cover art and a `cover.jpg` sidecar so the cover-forward home and review surfaces show real covers, with a designed fallback when no art exists.

Key behaviors: for each book the engine reads embedded art or a `cover.jpg` sidecar (read-only; never writes tags or images). Covers render square 1:1. When no cover is available, the fallback tile is the book title text on a deterministic colored tile whose color is a hash of the title, so repeated renders of the same book are stable and the shelf degrades gracefully.

Edge cases: extraction is read-only and must never modify audio files or sidecars (safety invariant, D-09). A corrupt or unreadable art payload falls back to the deterministic tile rather than erroring the shelf.

Acceptance criteria:
- [x] AC-21 The engine reads embedded cover art and `cover.jpg` sidecars read-only and exposes them to the GUI; no file is written or modified by cover extraction. [FD-03; D-09; S-2 F-1101 subset]
- [x] AC-22 Covers render at 1:1 aspect ratio on the home and review surfaces. [D-15; D-06]
- [x] AC-23 When no cover is available (or the art is unreadable), a fallback tile renders the title text on a color deterministically derived from a hash of the title; the same title always yields the same tile. [FD-03]

### F-908 (error, empty, and loading states) - P0 (NEW)

Implements FD-04. The prototypes are happy-path only; this feature authors the missing surfaces defined in docs/internal/design-system.md Section 5, mapping every `AppError` family to a family-safe surface.

Key behaviors, error surfaces (one family-safe surface each): blocked campaign group; scan failure; apply failure; snapshot-stale re-validation prompt (`plan:invalidated` / `snapshot-stale`); corrupt-DB recovery notice (`db-corrupt-recovered`); permission-denied (`permission-denied(path)`). Empty/edge surfaces: already-tidy library (zero changes); empty library root; all-groups-excluded (the primary action is disabled with an explanatory line); no duplicates found. Loading surfaces: a distinct "building the plan" state between scan and review; re-scan progress from the home.

Edge cases: error and danger states use a dedicated error/danger token pair distinct from the `--alert` terracotta accent, WCAG AA compliant in both themes (FD-09). No error surface shows a raw OS error without its stable machine code and a remediation sentence (S-2 error taxonomy).

Acceptance criteria:
- [x] AC-24 Every `AppError` family listed in the error taxonomy maps to a family-safe surface (blocked group, scan failure, apply failure, snapshot-stale re-validation, corrupt-DB recovery, permission-denied), each showing a plain-language remediation and never a bare OS error. [FD-04; S-2 error taxonomy Section 8]
- [x] AC-25 The empty/edge states render correctly: already-tidy library (zero changes), empty library root, all-groups-excluded (primary action disabled with an explanatory line), and no-duplicates. [FD-04]
- [x] AC-26 A distinct "building the plan" loading state renders between scan completion and the review surface, and it carries a real Stop control (design-system Section 5.3/5.4, "every progress screen carries a real Stop control"); re-scan from the home shows scan progress. [FD-04; FD-02]
- [x] AC-27 Error and danger surfaces use the dedicated error/danger token pair (distinct from `--alert`), verified WCAG AA (>= 4.5:1) in both Day and Evening. [FD-09; FD-21]

### F-909 (first-run and library root selection) - P0 (NEW)

Implements FD-05 and FD-29. No library is assumed; the first run asks the user to pick a library root.

Key behaviors: onboarding picks the library root via `tauri-plugin-dialog` (the OS folder picker); the frontend never reads the filesystem directly. Defaults on first run: ruleset `abs-author-first` (D-02), theme Day (FD-09). Settings (F-803) hosts re-selection of the root. At startup the backend re-allows the persisted root(s) via the Tauri capability model so the app can operate on them again (FD-29).

Edge cases: if no root is chosen, the app stays on the first-run surface and no scan can start. A previously persisted root that no longer exists routes to a permission-denied / root-not-found F-908 surface with a re-pick action.

Acceptance criteria:
- [x] AC-28 On first run with no persisted root, the app presents an onboarding surface whose only path forward is picking a library root through `tauri-plugin-dialog`; no library is assumed. [FD-05; S-1 release plan v0.9.0 first-run note]
- [x] AC-29 First-run defaults are applied: ruleset `abs-author-first` and theme Day. [D-02; FD-09; FD-05]
- [x] AC-30 The frontend performs no direct filesystem access; folder selection and all mutations go through typed IPC into abo-core. [FD-29]
- [x] AC-31 At startup the backend re-allows the persisted library root(s) so operations resume; a missing persisted root routes to the F-908 root-not-found surface with a re-pick action. [FD-29; FD-04; S-2 error taxonomy `root-not-found`]

### F-906 (settings and ruleset editor) - P0

Ruleset CRUD (create, read, update, delete) with a live re-plan preview: editing ruleset toggles updates the projected change counts.

Key behaviors: the ruleset editor exposes the F-402 structure policies and F-401 template presets; changing a toggle re-runs a preview and updates the projected counts per campaign group so the user sees the effect before committing to a full regenerate. Rulesets persist via `ruleset_save` (F-801).

Edge cases: a ruleset change that would newly block operations surfaces those as blocked in the preview counts, not as a silent drop.

Acceptance criteria:
- [x] AC-32 The ruleset editor supports create, read, update, and delete over rulesets and persists via `ruleset_save` (F-801). [S-2 F-801, F-906]
- [x] AC-33 Editing a ruleset toggle updates the projected per-group change counts live (a preview re-plan), before any full plan regenerate is committed. [S-1 release plan v0.4.0; S-2 F-906]

### F-803 (app settings) - P0

The singleton settings surface: library roots, quarantine root, reports folder, and theme persistence, plus snapshot retention.

Key behaviors: settings persist to the single-row settings table (F-803) via `settings_set`. Theme (Day/Evening) persists here (FD-09). Snapshot retention keeps the last N scans (default 10) to bound DB growth (FD-20), exposed as a setting.

Acceptance criteria:
- [x] AC-34 Settings persist library roots, quarantine root, reports folder, and theme to the singleton settings row and survive restart. [S-2 F-803; FD-09]
- [x] AC-35 A snapshot-retention setting (keep last N scans, default 10) is present and bounds stored snapshots to N. [FD-20; S-2 F-803]

### Cross-cutting: Stop control on progress screens (FD-02, scan and plan-building side)

Every progress screen in this release (scan, re-scan, and plan-building) has a real Stop control: cooperative cancel with F-104 (job progress and cancel) semantics, taking effect only at safe boundaries. The apply-side Stop and pause/resume are v0.5.0 (acting).

Acceptance criteria:
- [x] AC-36 Scan, re-scan, and plan-building progress screens present a Stop control that cooperatively cancels at a safe boundary (never mid-file), matching F-104 semantics; the "Skip ahead" demo affordance does not ship. [FD-02; S-2 F-104; design-system Section 5.4]

### Cross-cutting: copy and font invariants

- [x] AC-37 No surface claims genre folders become tags or promises any ABS-side change; the removed prototype line "the old genre view lives on as tags" is absent. [FD-12]
- [x] AC-38 The FD-10 deletion-guarantee copy is used verbatim everywhere the guarantee appears in-app; "set aside" is the primary vocabulary for quarantine and "deleted" is negated only inside a guarantee enumeration. [FD-10]
- [x] AC-39 Literata is bundled in-app (self-hosted woff2, SIL OFL) with a system serif fallback; the app makes zero network requests, verified by a CI grep for external hosts. [FD-11]

### Cross-cutting: accessibility verification (FD-21 gates start here)

- [x] AC-40 A mechanical contrast-check script verifies every token pair in both Day and Evening at >= 4.5:1 for informational text and runs in CI from this release; `--ink-3` tertiary text is restricted to decorative content or darkened/lightened to pass where it conveys information. [FD-21]
- [x] AC-41 An axe-core smoke check runs in Vitest on the primary surfaces (home, review), and a keyboard-walkthrough item is present in the per-release manual QA checklist. [FD-21]

## Out of scope

- F-501 (everything view: virtualized full change list, tree optional) - deferred to v0.6.0 (hardening) as P1 tier-1 disclosure (D-16, FD-06). The DESCOPE note stands: the cards-plus-report flow is the load-bearing P0; the exhaustive tree/list is not in this release.
- F-904 (apply and activity surface), F-601..F-607 (executor, journal, rollback, quarantine, dry-run harness) - v0.5.0 (acting).
- F-608 (pause and resume apply) and apply-side Stop - v0.5.0 (acting) per FD-02.
- F-702 (hash verification), F-703 (duplicate review), F-704 (resolution policies), F-905 (duplicates surface) - v0.6.0 (hardening).
- Command palette (cmdk) - P1; ships only if time allows, otherwise the simple filter box (F-503) stands.
- Any Real (non-dry-run) apply against the actual library - human-only gate (D-10), not exercised by software here.

## Release gate

Composite checklist that must be green before v0.4.0 tags. Evidence pointers follow docs/internal/test-strategy.md conventions (Frontend layer: Vitest component tests; IPC layer: bindings-drift + no-raw-invoke lint; Real-data confidence: jp manual review loop).

Gate walked by Fable, 2026-07-06. All three PRs adversarially reviewed (SECURITY pass each time; capability inventory held at exactly 7 permissions; strict CSP live-verified). Two review-driven fix waves landed pre-merge. Every gate below was first verified locally with the full suite green (workspace Rust + 207 Vitest + contrast + axe + copy sweep) while merges sat behind the GitHub Actions billing block; jp resolved the block later the same day and the merges landed with CI green (see G-9).

- [ ] G-1 PENDING JP: the human review loop in the app against the real-library snapshot (open home, read health facts, inspect a plan, drill in, approve groups, exclude an operation). The app is ready for this walk; it is the release's one human gate. [AC-6..AC-20]
- [x] G-2 Review surface responsive over the full library scale: server-side caps + windowed rendering; headed walks over fixture libraries jank-free; tree view remains descoped (FD-06). [FD-18]
- [x] G-3 No raw invoke; eslint also bans the fs plugin (static + dynamic import forms). [AC-5]
- [x] G-4 FD-21 gates green locally: contrast 44/44 pairs AA both themes (two hand-verified by the reviewer); axe smoke zero serious/critical after fixing two real ARIA bugs it caught; keyboard walkthrough recorded in docs/internal/qa/v0.4.0-manual-qa.md. CI wiring lands with the queued merges. [AC-40, AC-41]
- [x] G-5 Zero network verified in-app: bundled Literata renders under the strict CSP (default-src 'self'; img-src 'self' data:; connect-src ipc), live CDP network capture showed only tauri.localhost/ipc.localhost traffic. [AC-39]
- [x] G-6 Copy gates: FD-10 verbatim (asserted by the sweep test), no genre-as-tags claim, no hardcoded sample numbers; banned vocabulary sweep green. [AC-9, AC-37, AC-38]
- [x] G-7 Tracer UI deleted (src/App.tsx removed; AppRoot is the sole entry). [S-1]
- [x] G-8 Scan Stop works: headed-verified cooperative cancel of a real in-flight scan with an honest stopped state. [AC-36]
- [x] G-9 CI matrix green, CONFIRMED at merge time 2026-07-06: jp restored Actions billing; PR #17 and PR #18 pull-request runs green, and both main merge-commit push runs green including the macOS honesty legs (runs 28806796001 and 28808035764). All matrix legs had been reproduced locally during the block; macOS stays push-only per the CI-cost fix. [FD-24]

## Behavior / Examples

- Include/skip and live totals: on the review surface, the user skips the "bundles" group; the footer recomputes and states, for example, "6 of 7 groups included, N changes", where N is the summed change count of included groups (illustration only; real N derives from the plan, FD-27). [S-6 prototype 05]
- Two-step confirm: the user presses "Tidy up now"; an inline confirm appears ("Tidy up the included groups now?") and only the second press schedules the apply. [S-7 prototype 07]
- Fallback tile: a book with no embedded art and no `cover.jpg` renders as the title text on a muted tile; the same book renders the same color on every visit. [FD-03]
- All-groups-excluded: the user turns every group off; the primary action disables and an explanatory line reads that nothing is selected to tidy. [FD-04]

## Non-Functional Requirements

- Responsiveness: no UI freeze during scans; all long work runs on the Tokio runtime with event-driven progress (`job:progress`) and no polling. [S-2 NFR responsiveness]
- Scale: the plan preview is virtualized (TanStack Virtual) and comfortable over 20,000 files / 1,000 folders. [S-2 NFR scale]
- Privacy: no network, no telemetry; the offline posture is visible in the UI footer. [S-2 F-1003 note; FD-11]
- Accessibility: WCAG AA (4.5:1 body) in both themes, status never color-alone (icon plus label), keyboard-reachable with visible focus, `prefers-reduced-motion` honored. [S-4 PRODUCT.md; FD-21]

## Source traceability

| Feature | Discovery / planning source | D/FD decisions |
|---|---|---|
| F-901 (app shell and navigation) | breakdown E-09 F-901; prototype 04/05 titlebar+nav | FD-07 (no "dashboard"), FD-08 (group badges), FD-09 (themes), FD-29 (no raw invoke) |
| F-902 (library home) | breakdown F-902; prototype 04; PRODUCT.md principles | FD-07 (rename), D-06 (anti-reference), FD-10 (guarantee copy), FD-27 (sample data) |
| F-903 host + F-502 (campaign group review) | breakdown F-502, F-403; prototype 05/07 | D-16 (cards+report is P0), FD-26 (seven groups), FD-08 (group unit), planning audit stream 2 item 15 (per-op exclude) |
| F-503 (search and filter) | breakdown F-503 | planning audit stream 2 item 15 (simple filter box) |
| F-504 (explainability) | breakdown F-504, F-303; prototype 05 | FD-13 (extended disclosure) |
| F-907 (cover extraction and fallback tiles) | breakdown F-1101 subset; PRODUCT.md cover-forward | D-15, FD-03, D-06 (square 1:1) |
| F-908 (error, empty, loading states) | planning audit stream 2 items 1,6,7; error taxonomy Section 8 | FD-04, FD-09 (error token pair) |
| F-909 (first-run and library root) | planning audit stream 2 item 2; release plan v0.9.0 first-run | FD-05, FD-29 |
| F-906 (settings and ruleset editor) | breakdown F-906, F-801, F-402/F-401 | (mechanism from D-01 stack) |
| F-803 (app settings) | breakdown F-803 | FD-09 (theme persist), FD-20 (snapshot retention) |
| Stop control | breakdown F-104 | FD-02 |
| Copy/font/a11y invariants | PRODUCT.md; planning audit stream 2 items 5,7,10,19 | FD-10, FD-11, FD-12, FD-21, FD-27 |

## Revisions

| Date | Change | By |
|---|---|---|
| 2026-07-03 | Initial spec authored for the planning suite (status: review). | release-author agent |

## Sources & Evidence

- [S-1] Release plan and CI (v0.4.0 details, gate, descope) - `_local/planning/release-plan-and-ci_2026-07-02.md`. Class A (ratified planning doc).
- [S-2] Feature-function breakdown (E-09 surfaces, IPC surface, error taxonomy, NFR) - `_local/planning/feature-function-breakdown_2026-07-02.md`. Class A.
- [S-3] Strategy brief (audience, campaign posture) - `_local/planning/audiobook-organizer-strategy-brief_2026-07-02.md`. Class A.
- [S-4] PRODUCT.md (design contract, principles, accessibility) - `PRODUCT.md`. Class A (authoritative).
- [S-5] Prototype: library home - `_local/gui/04-library.html`. Class B (shape reference; numbers are sample data).
- [S-6] Prototype: review - `_local/gui/05-review.html`. Class B.
- [S-7] Prototype: complete flow - `_local/gui/07-complete-flow.html`. Class B.
- [S-8] Prototype: dry-run report - `_local/gui/06-dryrun-report.html`. Class B.
- [S-9] Design system doc (token canon, copy register, Section 5 states) - `docs/internal/design-system.md`. Class A (companion, authored in this suite).
- [S-10] Test strategy doc (layers, evidence conventions) - `docs/internal/test-strategy.md`. Class A.
- [S-11] decision ledger and Fable-fixed decisions - docs/internal/decision-ledger.md (D-02..D-16, FD-02..FD-30). Class A.
- [S-12] AUDIT findings, stream 2 (prototypes as design contract) - docs/internal/planning-audit-2026-07-03.md. Class A.

## Open Questions

- OQ-1 Command palette (cmdk): ships as P1 if v0.4.0 has slack, else the simple filter box (F-503) is the only find affordance this release. Decide at mid-release checkpoint. [S-2 F-503]
- OQ-2 F-907 cover extraction performance on a cold library scan: read cover art inline with scan, or as a separate background pass? Owner call during implementation; must not slow the read-only scan gate (< 60 s). [S-2 NFR scale; FD-03]
- OQ-3 Copies-group presentation in v0.4.0 before hash verification exists (F-702 is v0.6.0): render as candidates in a waiting/flag-only state, or hide the group's destructive controls entirely until v0.6.0. [FD-08; S-1 release plan]
