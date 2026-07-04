---
id: v0.4.0-seeing-plan
title: "Implementation Plan - Release v0.4.0 (seeing)"
type: implementation-plan
date: 2026-07-03
status: review
owner: jprisant
tier: release
scope: GUI surfaces (E-09) over the frozen v0.3.0 seam; no new engine capability
depends_on: v0.3.0-planning
linked-spec: docs/internal/releases/v0.4.0-seeing/spec.md
produced-by: release-author agent (Fable planning suite)
phase-count: 8
ac-coverage: complete
executor-model-guidance: >
  Per FD-30. Opus-tier: F-907 cover extraction (read-only safety, must never write),
  the Tauri capability re-allowance path (F-909/FD-29), and the F-908 error-family
  mapping (safety-adjacent correctness). Sonnet-tier: React/shadcn component
  scaffolding, table-driven Vitest, copy-register wiring, the token-contrast script.
  Fable reviews every phase decision gate and owns the G-1 human review loop and the
  final release-gate verification.
sources:
  - docs/internal/releases/v0.4.0-seeing/spec.md
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - docs/internal/design-system.md
  - docs/internal/test-strategy.md
  - _local/gui/04-library.html
  - _local/gui/05-review.html
  - _local/gui/07-complete-flow.html
---

# Implementation Plan: Release v0.4.0 (seeing)

## Task Summary

- Status: review (pending jp approval of the planning suite).
- Goal: build the five product surfaces on the frozen tauri-specta seam, delete the tracer-bullet UI, and pass the FD-21 accessibility and FD-11 zero-network gates.
- Phase count: 8. AC coverage: complete (every spec AC mapped to at least one phase).
- Test-first posture: the no-raw-invoke lint, token-contrast script, axe-core smoke, and Vitest approval-state tests are authored before or alongside the components they guard.
- Last updated: 2026-07-03.

## Completion Status

| Phase | Goal | Fulfills AC | Tasks | Owner | Status |
|---|---|---|---|---|---|
| P1 | App shell, theme system, navigation | AC-1, AC-2, AC-3, AC-4, AC-5 | T-01..T-05 | LLM (Sonnet) | Not started |
| P2 | First-run, root selection, capability re-allowance, settings | AC-28, AC-29, AC-30, AC-31, AC-34, AC-35 | T-06..T-10 | LLM (Opus for capability path) | Not started |
| P3 | Cover extraction subset and fallback tiles | AC-21, AC-22, AC-23 | T-11..T-14 | LLM (Opus) | Not started |
| P4 | Library home | AC-6, AC-7, AC-8, AC-9 | T-15..T-18 | LLM (Sonnet) | Not started |
| P5 | Plan review surface (groups, filter, explainability) | AC-10..AC-20 | T-19..T-25 | LLM (Sonnet) | Not started |
| P6 | Ruleset editor with live re-plan; scan Stop control | AC-32, AC-33, AC-36 | T-26..T-28 | LLM (either) | Not started |
| P7 | Error, empty, loading states + error token pair | AC-15, AC-24, AC-25, AC-26, AC-27 | T-29..T-32 | LLM (Opus for AppError mapping) | Not started |
| P8 | Copy/font/a11y gates; delete tracer UI; release gate | AC-37, AC-38, AC-39, AC-40, AC-41 | T-33..T-38 | Fable + LLM | Not started |

## Phase 1: App shell, theme system, navigation

**Goal:** stand up the window frame, Day/Evening theme, and sidebar every surface renders inside. **Addresses:** AC-1, AC-2, AC-3, AC-4, AC-5.

Tasks:
- **T-01** (M; depends: phase entry) Create the React shell in `src/App.tsx` and a layout in `src/components/shell/AppShell.tsx`: custom titlebar (`Titlebar.tsx` with app-name, `ThemeToggle.tsx`, window caption buttons via `@tauri-apps/api/window`) and left sidebar (`Sidebar.tsx`) linking Library, Tidy-up, Duplicates, History, Settings with `aria-current` on the active route (router in `src/routes.tsx`).
- **T-02** (M; depends: T-01) Implement the theme system in `src/lib/theme.ts`: set `document.documentElement.dataset.theme` to `day` or `evening` only; read/write via the settings binding (Phase 2). Add token CSS in `src/styles/tokens.css` matching docs/internal/design-system.md (data-theme="day"/"evening", FD-09). Grep the tree to prove `calm`/`prose` identifiers are absent.
- **T-03** (S; depends: T-01) Wire sidebar badges to `classify_overview` group counts (Duplicates badge = duplicate GROUP count, FD-08); never label copies.
- **T-04** (S; depends: T-02) Add `prefers-reduced-motion` handling in `tokens.css` (collapse transitions).
- **T-05** (S; depends: phase entry) Add the no-raw-invoke ESLint rule (`.eslintrc` custom rule or `no-restricted-imports` blocking `@tauri-apps/api/core` `invoke`); all IPC imports come from `src/lib/bindings.ts` (tauri-specta generated).

Verification: `pnpm typecheck && pnpm lint` green (no-raw-invoke rule passes); Vitest render test asserts sidebar links and `aria-current`; manual: theme toggle flips `data-theme` and only `day`/`evening` occur.

Decision Gate: confirm the design-system token names and the Day-default before Phase 4 renders health facts. Fable reviews.

Output Artifacts: `src/components/shell/*`, `src/lib/theme.ts`, `src/styles/tokens.css`, `src/routes.tsx`, eslint no-raw-invoke rule.

Suggested Owner: LLM (Sonnet).

## Phase 2: First-run, root selection, capability re-allowance, settings

**Goal:** no library assumed; pick a root via the dialog plugin, persist settings, and re-allow the root at startup. **Addresses:** AC-28, AC-29, AC-30, AC-31, AC-34, AC-35.

Tasks:
- **T-06** (M; depends: phase entry) Backend: add `settings_get`/`settings_set` command wrappers in `src-tauri/src/commands/settings.rs` over `abo-core` F-803 (singleton settings row), exposing library roots, quarantine root, reports folder, theme, snapshot-retention N (default 10, FD-20). Regenerate bindings.
- **T-07** (M; depends: T-06) Frontend first-run: `src/routes/FirstRun.tsx` uses `@tauri-apps/plugin-dialog` `open({ directory: true })` to pick the root; on select, call `settings_set` and default ruleset to `abs-author-first` (D-02) and theme to Day (FD-09). No path forward without a root.
- **T-08** (M; depends: T-06) Backend startup re-allowance (Opus): in `src-tauri/src/main.rs` setup, read persisted roots and re-add them to the fs scope via the Tauri capability/asset scope API so backend operations resume (FD-29); if a persisted root is missing, surface `root-not-found` to the F-908 route (Phase 7).
- **T-09** (M; depends: T-06) Settings surface `src/routes/Settings.tsx`: edit roots, quarantine root, reports folder, theme, snapshot-retention; persists via `settings_set`.
- **T-10** (S; depends: T-05) Assert the frontend never calls fs APIs directly (covered by the no-raw-invoke lint plus a grep for `@tauri-apps/plugin-fs` in `src/`).

Verification: Vitest: first-run blocks progress until a root is chosen; settings round-trip test (set then get) via a mocked binding; Rust test: startup re-allowance adds the persisted root to scope; snapshot-retention prunes to N (unit test in `abo-core`).

Decision Gate: capability/scope re-allowance API shape on Tauri v2 confirmed against the reference architecture (FD-29). Opus + Fable review.

Output Artifacts: `src-tauri/src/commands/settings.rs`, `src/routes/FirstRun.tsx`, `src/routes/Settings.tsx`, startup re-allowance in `main.rs`.

Suggested Owner: LLM (Opus for the capability path; Sonnet for the forms).

## Phase 3: Cover extraction subset and fallback tiles

**Goal:** read real covers read-only and render a deterministic fallback when none exist. **Addresses:** AC-21, AC-22, AC-23.

Tasks:
- **T-11** (M; depends: phase entry) Add the `lofty` cover subset in `crates/abo-core/src/scan/cover.rs`: read embedded art and a sibling `cover.jpg` read-only; return bytes + mime, or `None`. Never open files write-mode; add a test asserting no write occurs (open-count / read-only handle assertion).
- **T-12** (S; depends: T-11) Expose via `folder_detail` (or a dedicated `cover_get(scan_id, entry_id)`) binding; regenerate bindings.
- **T-13** (M; depends: T-12) Frontend `src/components/Cover.tsx`: render 1:1 aspect ratio; on `None` or decode error, render `FallbackTile.tsx` = title text on a color from `hashTitleToHsl(title)` in `src/lib/coverHash.ts` (deterministic).
- **T-14** (S; depends: T-11) Golden test in `abo-core`: a fixture with embedded art returns bytes; a fixture with neither returns `None`.

Verification: Rust: read-only assertion test + extraction golden; Vitest: same title yields same fallback color across renders; visual: covers are square.

Decision Gate: OQ-2 (inline-with-scan vs background pass) - must not push read-only scan past the < 60 s gate. Opus + Fable review.

Output Artifacts: `crates/abo-core/src/scan/cover.rs`, `src/components/Cover.tsx`, `src/components/FallbackTile.tsx`, `src/lib/coverHash.ts`.

Suggested Owner: LLM (Opus - safety-critical read-only guarantee).

## Phase 4: Library home

**Goal:** the warm, cover-forward home with facts in sentences and one primary action. **Addresses:** AC-6, AC-7, AC-8, AC-9.

Tasks:
- **T-15** (M; depends: phase entry) `src/routes/Library.tsx`: lede sentence composed from `classify_overview` (F-202) via a TanStack Query hook `useHealthMetrics()`; no hardcoded numbers (FD-27). Copy strings live in `src/lib/strings.ts` (FD-23 central strings module).
- **T-16** (S; depends: T-13, T-15) "Worth a look first" shelf: example covers (Phase 3 `Cover`) with plain-language chips ("loose file", "messy name", "N books, 1 folder", "N copies"); a single primary action "Start a tidy-up" routing to review. No stat band / hero tile / tracked eyebrow (D-06) - assert via a component test that forbids those classes.
- **T-17** (S; depends: T-15) Series spine clusters component `src/components/SpineCluster.tsx` (stylized spines retained, D-06).
- **T-18** (S; depends: T-15) Reassurance line uses the FD-10 verbatim string from `strings.ts`.

Verification: Vitest: home renders numbers from a mocked `classify_overview` and none are literals; a test asserts absence of stat-band markup; copy test asserts the FD-10 string is present verbatim.

Decision Gate: N/A.

Output Artifacts: `src/routes/Library.tsx`, `src/components/SpineCluster.tsx`, `src/lib/strings.ts`.

Suggested Owner: LLM (Sonnet).

## Phase 5: Plan review surface (groups, filter, explainability)

**Goal:** the review two-pane: seven group cards with include/skip, per-op exclude, two-step confirm, simple filter, and the extended "Show file details". **Addresses:** AC-10..AC-20.

Tasks:
- **T-19** (M; depends: phase entry) `src/routes/Review.tsx` two-pane layout. Left: `GroupCard.tsx` for the seven canonical groups (FD-26) from `plan_get`, each with an include/skip switch (`plan_set_group_approval`), reason sentence, and change count.
- **T-20** (S; depends: T-19) Footer totals `ReviewFooter.tsx`: live "N of 7 groups included, M changes" from included groups; label states which quantity (FD-08); recompute on toggle.
- **T-21** (S; depends: T-19) Two-step inline confirm on "Tidy up now" (`ConfirmInline.tsx`): first press reveals confirm, second commits (no Real apply this release; wires to the v0.5.0 apply entry, disabled/stubbed).
- **T-22** (M; depends: T-19) Group detail (right pane) `GroupDetail.tsx`: curated examples with Now/After in words; per-op exclude via `plan_exclude_op` (AC-13); blocked ops offer only exclude or fix-ruleset pointer (AC-14).
- **T-23** (S; depends: T-19) Filter box `PlanFilter.tsx` (AC-16): free-text over source/target + group/class/confidence/warning facets; view-only, never mutates approval (AC-17).
- **T-24** (M; depends: T-22) Explainability `FileDetails.tsx` (AC-18, AC-19, AC-20): `<details>`-style disclosure with raw paths, matched pattern id+name, extracted fields + per-field confidence, stripped noise from the op rationale (F-403/F-303). Raw paths only inside this disclosure.
- **T-25** (M; depends: T-19) Virtualize the plan rows with TanStack Virtual (G-2 responsiveness).

Verification: Vitest approval-state suite (per test-strategy Frontend layer): toggle updates totals; exclude persists; blocked op cannot be included; filter does not change approval; disclosure shows pattern+confidence. Manual: responsive over the real 718+ folder plan.

Decision Gate: OQ-3 copies-group presentation (candidates/waiting vs hidden destructive controls). Fable review.

Output Artifacts: `src/routes/Review.tsx`, `src/components/review/*`.

Suggested Owner: LLM (Sonnet).

## Phase 6: Ruleset editor with live re-plan; scan Stop control

**Goal:** ruleset CRUD with live preview counts and a working scan Stop. **Addresses:** AC-32, AC-33, AC-36.

Tasks:
- **T-26** (M; depends: phase entry) `src/routes/Rulesets.tsx` (hosted in Settings, F-906): CRUD over rulesets via `ruleset_list/get/save/delete` (F-801); expose F-401 presets + F-402 policy toggles.
- **T-27** (M; depends: T-26) Live re-plan: on toggle change, call a lightweight preview (a `plan_generate` in preview mode or a count-only variant) and show projected per-group counts before a full regenerate (AC-33); newly blocked ops show as blocked in the preview.
- **T-28** (M; depends: phase entry) Scan Stop control: `ScanProgress.tsx` calls `job_cancel` (F-104) with cooperative cancel at safe boundaries (AC-36); confirm the demo "Skip ahead" affordance is not present.

Verification: Vitest: toggling a policy updates projected counts from a mocked preview; Rust/integration: `job_cancel` stops a scan at a safe boundary and leaves a coherent job row (F-104 semantics).

Decision Gate: preview-count mechanism (dedicated count endpoint vs full preview plan) confirmed with the engine owner. Fable review.

Output Artifacts: `src/routes/Rulesets.tsx`, `src/components/ScanProgress.tsx`, any preview-count command wrapper.

Suggested Owner: LLM (either).

## Phase 7: Error, empty, loading states + error token pair

**Goal:** author every missing state from design-system Section 5 and the dedicated error token pair. **Addresses:** AC-15, AC-24, AC-25, AC-26, AC-27.

Tasks:
- **T-29** (S; depends: T-02) Add the error/danger token pair to `tokens.css` (distinct from `--alert`), WCAG AA in both themes (FD-09); the Phase 8 contrast script covers it.
- **T-30** (M; depends: T-29) Map each `AppError` family (Section 8 taxonomy) to a surface in `src/components/states/`: `BlockedGroup`, `ScanFailure`, `ApplyFailure`, `SnapshotStale` (from `plan:invalidated`/`snapshot-stale`, with a re-validate action), `CorruptDbRecovered`, `PermissionDenied` - each rendering a plain-language remediation, never a bare OS error (AC-24).
- **T-31** (M; depends: T-29) Empty/edge states: `AlreadyTidy` (zero changes), `EmptyLibraryRoot`, `AllGroupsExcluded` (primary disabled + explanatory line), `NoDuplicates` (AC-25); wire `AllGroupsExcluded` into Review footer (AC-15).
- **T-32** (M; depends: T-28) Loading states: `BuildingThePlan` between scan and review, and re-scan progress reuse of `ScanProgress` (AC-26).

Verification: Vitest per state renders the right copy for each AppError code; a test asserts `AllGroupsExcluded` disables the primary action; axe-core smoke (Phase 8) covers these surfaces; contrast script covers the error token pair (AC-27).

Decision Gate: N/A (mapping is enumerated by the taxonomy). Opus authors the AppError mapping; Fable reviews.

Output Artifacts: `src/components/states/*`, error token additions to `tokens.css`.

Suggested Owner: LLM (Opus for the AppError mapping; Sonnet for markup).

## Phase 8: Copy/font/a11y gates; delete tracer UI; release gate

**Goal:** land the cross-cutting gates and clear the release checklist. **Addresses:** AC-37, AC-38, AC-39, AC-40, AC-41.

Tasks:
- **T-33** (S; depends: phase entry) Copy sweep in `src/lib/strings.ts`: FD-10 guarantee verbatim (AC-38); remove any genre-as-tags claim (AC-37); confirm no hardcoded sample numbers on any surface (AC-7 re-check).
- **T-34** (M; depends: phase entry) Bundle Literata: self-hosted woff2 (SIL OFL) in `src/assets/fonts/`, `@font-face` in `tokens.css`, system-serif fallback; remove any Google Fonts `<link>`. Add a CI grep gate (`scripts/check-no-external-hosts.mjs`) over the app and report template for external hosts (AC-39, FD-11).
- **T-35** (M; depends: T-29) Token-contrast script `scripts/check-contrast.mjs`: parse `tokens.css`, verify every informational token pair >= 4.5:1 in both themes; wire into `ci.yml` lint job (AC-40); restrict `--ink-3` to decorative use.
- **T-36** (S; depends: phase entry) axe-core smoke in Vitest on home and review (`src/__tests__/a11y.test.tsx`) (AC-41); add the keyboard-walkthrough item to `docs/internal/manual-qa-checklist.md`.
- **T-37** (S; depends: phase entry) Delete the v0.1.0 tracer-bullet UI files (the throwaway scan button/JSON dump component) (G-7).
- **T-38** (L; depends: T-33, T-34, T-35, T-36, T-37) Run the full release gate G-1..G-9; Fable drives the G-1 human review loop with jp against the real-library snapshot.

Verification: `pnpm test` (Vitest incl. axe smoke) green; contrast + external-host scripts green in CI; grep confirms tracer UI removed; jp completes G-1.

Decision Gate: G-1 human review loop sign-off (jp) - release-blocking. Fable owns.

Output Artifacts: `scripts/check-contrast.mjs`, `scripts/check-no-external-hosts.mjs`, `src/assets/fonts/*`, `docs/internal/manual-qa-checklist.md`, deletion of tracer UI.

Suggested Owner: Fable (gate) + LLM (Sonnet, scripts).

## Test-first posture (by layer, per docs/internal/test-strategy.md)

- IPC contract: no-raw-invoke lint (Phase 1) and tauri-specta bindings-drift gate authored before components consume bindings.
- Frontend: Vitest approval-state suite (Phase 5) written against the mocked seam before/alongside the review components; axe-core smoke (Phase 8).
- Engine (abo-core): cover read-only assertion + extraction golden (Phase 3) and snapshot-retention unit test (Phase 2) precede their wiring.
- CI scripts: contrast and external-host gates land with the assets they check (Phase 8).

## Branch/PR plan

- One short-lived feature branch per phase (or per tight cluster), PR into `main`; agent self-merges green PRs while the repo is private (D-11; EXECUTION.md merge policy).
- Required green checks per PR: lint (fmt, clippy, core-purity, no-raw-invoke, bindings-drift), test matrix (Windows + macOS build; Vitest), and from this release the contrast + external-host + axe gates (FD-21, FD-11).
- Merge policy reference: EXECUTION.md (trunk-based, short-lived branches, CI substitutes for code review).

## Risks and descope triggers

- F-501 (everything view) is already out of this release (FD-06). If the grouped virtualized list itself is not responsive over the full library by end of window, the load-bearing flow is still the group cards plus the HTML report (D-16) - ship those and file the list perf issue.
- Command palette (cmdk) is P1 (OQ-1): descope to the simple filter box (F-503) if time is short.
- Tauri v2 capability re-allowance friction (FD-29): budget review time in Phase 2; if the scope API fights, fall back to re-prompting for the root on startup and file the persistence gap (never let the frontend touch fs to compensate).
- Cover extraction perf (OQ-2): if inline extraction risks the < 60 s scan gate, move it to a background pass.
- macOS bundle red > 1 week: downgrade the macOS CI job to allow-fail with a tracking issue; never block this Windows release on it (release plan Section 5).

## Definition of done

The v0.4.0 (seeing) release gate, restated as the exit checklist:

- [ ] G-1 jp completes the full review-and-approve loop in the app against the real-library snapshot.
- [ ] G-2 Grouped, virtualized plan preview responsive over the full library; tree view NOT required.
- [ ] G-3 No raw `invoke` in the frontend (lint green).
- [ ] G-4 FD-21 gates green: token-contrast script + axe-core smoke in CI; keyboard walkthrough in the manual QA checklist.
- [ ] G-5 FD-11 zero-network verified: bundled Literata loads; external-host grep gate green.
- [ ] G-6 Copy gates: FD-10 verbatim, FD-12 no genre-as-tags, FD-27 no hardcoded sample numbers.
- [ ] G-7 Tracer-bullet UI deleted.
- [ ] G-8 Scan/re-scan Stop control works (cooperative cancel).
- [ ] G-9 CI matrix green (lint, test, Windows + macOS build).
