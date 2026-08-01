---
id: v0.9.0
title: "Implementation Plan - Release v0.9.0 (packaged)"
type: implementation-plan
date: 2026-07-03
status: review
owner: jprisant
produced-by: release implementation-plan author agent
tier: release
scope: distribution, user documentation, first-run polish, fresh-machine verification, release ceremony
depends_on: v0.6.0 (hardening)
linked_spec: docs/internal/releases/v0.9.0-packaged/spec.md
sources:
  - docs/internal/releases/v0.9.0-packaged/spec.md
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 6.2 release.yml, Section 4 v0.9.0)
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/release-plans/runbook_cut-tag-release.md
  - docs/internal/test-strategy.md
  - docs/internal/ci-plan.md
  - docs/internal/design-system.md
executor_model_guidance: >
  Per FD-30. Fable reviews every gate and owns the AC-14 (D-13) human-gate framing and the
  release-ceremony dry-run sign-off. Opus-tier work: any change that touches the corrupt-DB
  recovery path, the uninstall data-retention behavior, and the no-network verification harness
  (safety-adjacent, must not weaken D-09 (safety invariants)). Sonnet-tier work: release.yml artifact
  wiring, bump-version script, CHANGELOG mechanics, documentation prose, and the QA checklist
  skeleton. Almost all of v0.9.0 is packaging and verification, not engine code, so it skews
  Sonnet-heavy with Fable gate reviews and a small Opus core for the recovery/retention paths.
---

# Implementation Plan: Release v0.9.0 (packaged)

## Task Summary

Take the hardened v0.6.0 build and make it installable, documented, and cuttable by a fixed ceremony, then prove it on a machine that never saw the app. Work is grouped into six phases: release artifact, documentation, first-run polish, fresh-machine app-data lifecycle, the fresh-machine end-to-end gate plus no-network proof, and the release ceremony plus the D-13 (OSS posture: license and public flip) human gate. No engine capability is added. Status: review. Last updated 2026-07-03.

## Completion Status

| Phase | Goal | Fulfills AC | Tasks | Owner | Status |
|---|---|---|---|---|---|
| P1 | Release artifact from release.yml (NSIS + MSI, draft, size) | AC-1, AC-2, AC-11 | T-01..T-06 | LLM (Sonnet); Fable reviews | Not started |
| P2 | User documentation (README, user guide, install/SmartScreen, safety) | AC-3, AC-4, AC-5, AC-9 | T-07..T-12 | LLM (Sonnet); Fable reviews copy | Not started |
| P3 | First-run experience polish on a fresh machine | AC-6, AC-7 | T-13..T-16 | LLM (Sonnet) + human verify | Not started |
| P4 | Fresh-machine app-data lifecycle (bootstrap, recovery, uninstall) | AC-8, AC-9 | T-17..T-20 | LLM (Opus) for recovery/retention | Not started |
| P5 | Fresh-machine end-to-end gate + no-network proof | AC-10, AC-12 | T-21..T-25 | human (jp/verifier) + LLM harness | Not started |
| P6 | Release ceremony + D-13 human gate | AC-13, AC-14 | T-26..T-30 | LLM (Sonnet) scripts; human decision | Not started |

## Phase 1: Release artifact from release.yml

**Goal:** produce and verify the unsigned Windows installer artifacts and the draft release. **Addresses:** AC-1, AC-2, AC-11.

Tasks:
- **T-01** (M; depends: phase entry) Promote the release plan Section 6.2 sketch into `.github/workflows/release.yml`: trigger on `push: tags: ["v*"]`; matrix `{ os: [windows-latest, macos-latest] }`; toolchain + pnpm setup mirroring `ci.yml`; `pnpm tauri build` (release, dist profile).
- **T-02** (S; depends: phase entry) Confirm `src-tauri/tauri.conf.json` bundle targets include both `nsis` and `msi` for Windows and that the WebView2 install mode is `downloadBootstrapper` (footprint per AC-11).
- **T-03** (S; depends: T-01) Add `[profile.dist]` full-LTO in the root `Cargo.toml` (FD-24) and confirm release.yml selects it (`--profile dist` or the Tauri dist wiring); keep per-push CI on thin-LTO.
- **T-04** (S; depends: T-01) Add a SHA256SUMS generation step over the bundle files, then `softprops/action-gh-release@v2` with `draft: true` and files = bundles + SHA256SUMS.
- **T-05** (S; depends: T-01) Add a bundle-size assertion step: fail the job if the Windows NSIS/MSI exceeds 30 MB, so AC-11 is mechanical, not manual.
- **T-06** (M; depends: T-01, T-02, T-03, T-04, T-05) Test the workflow against a throwaway pre-release tag on a branch (e.g. `v0.9.0-rc.test`) to confirm it builds and drafts without publishing; delete the test tag and draft afterward.

Verification: a draft release exists with NSIS, MSI, and SHA256SUMS attached; the size-assertion step is green; no publish happened. Evidence: workflow run link recorded in the release report.

Decision Gate: confirm OQ-3 (attach the unsigned macOS bundle to the same draft or omit it). Default: attach with an "unsigned, unverified on hardware" note.

Output Artifacts: `.github/workflows/release.yml`, `Cargo.toml` `[profile.dist]`, `src-tauri/tauri.conf.json` bundle config, a discarded test draft release.

Suggested Owner: LLM (Sonnet); Fable reviews the draft-not-publish boundary.

## Phase 2: User documentation

**Goal:** write the README and user guide in the plain-language register, including install/SmartScreen and the safety model. **Addresses:** AC-3, AC-4, AC-5, AC-9.

Tasks:
- **T-07** (M; depends: phase entry) Write `README.md` at repo root: what the tool does (scan, review, dry run + report, tidy-up, undo), who it is for, install pointer, safety promise. Public-ready tone (D-13) but honest about beta/unsigned.
- **T-08** (M; depends: phase entry) Write `docs/user-guide.md` (user-facing): a walkthrough of the full pipeline using the design-system copy canon (books, shelves, copies, tidy-up, set aside); zero forbidden terms (operations, ops, dedupe, manifest, quarantine, dashboard).
- **T-09** (S; depends: phase entry) Write `docs/install.md`: download, run the unsigned installer, and the SmartScreen "More info -> Run anyway" flow with exact button labels and a screenshot placeholder (AC-3).
- **T-10** (S; depends: T-07, T-08) Add a "What this tool will never do" section using the FD-10 canon string verbatim: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone." Explain set-aside and undo without "quarantine" (AC-5).
- **T-11** (S; depends: T-08) Add an "Uninstalling" section stating the retention policy: program files are removed; your saved information and any set-aside folder stay (AC-9 doc half).
- **T-12** (M; depends: phase entry) Add a copy-audit script `scripts/plain-language-grep.mjs` (or extend an existing lint) that greps user-facing docs for the forbidden word list and for the presence of the FD-10 canon string; wire it into CI as `docs: plain-language-grep`.

Verification: `docs: plain-language-grep` is green (no forbidden terms; canon string present). A non-engineer read-through (jp or a household member) confirms the guide is followable. Evidence: CI check + a note in the QA checklist.

Decision Gate: N/A.

Output Artifacts: `README.md`, `docs/user-guide.md`, `docs/install.md`, `scripts/plain-language-grep.mjs`, a CI step.

Suggested Owner: LLM (Sonnet); Fable reviews the register and the safety copy.

## Phase 3: First-run experience polish

**Goal:** verify and polish F-909 (first-run and library root selection) where no app data exists. **Addresses:** AC-6, AC-7.

Tasks:
- **T-13** (M; depends: phase entry) On a machine (or clean profile) with no `%LOCALAPPDATA%\AudiobookOrganizer\`, launch the packaged build and walk onboarding: folder-picker via tauri-plugin-dialog, default ruleset abs-author-first, default theme day, land on library home (FD-07) with one primary action.
- **T-14** (M; depends: T-13) Fix any polish gaps found (wording per design-system, the FD-13 friendly-location rule on the scan line, focus order, reduced-motion) as small frontend edits in `src/` first-run components; do not re-architect F-909.
- **T-15** (S; depends: phase entry) Confirm the capability manifest (`src-tauri/capabilities/*.json`) exposes no `fs`/`shell` to the WebView and that folder access is dialog-only (FD-29).
- **T-16** (S; depends: phase entry) Confirm the no-raw-invoke lint passes (generated bindings only) and add a first-run Vitest component test for the "no persisted root -> onboarding shown" branch if not already covered by v0.4.0.

Verification: fresh-profile launch reaches library home with a chosen root; no-raw-invoke lint green; capability review recorded; first-run component test green. Evidence: screenshot in the QA checklist + lint output.

Decision Gate: N/A (F-909 behavior is fixed by FD-05; this phase only verifies and polishes).

Output Artifacts: minor edits under `src/` first-run components, possibly `src-tauri/capabilities/*.json`, a Vitest test.

Suggested Owner: LLM (Sonnet) for edits; human verifies the fresh-profile launch.

## Phase 4: Fresh-machine app-data lifecycle

**Goal:** verify bootstrap, corrupt-DB recovery, and uninstall retention on a clean machine. **Addresses:** AC-8, AC-9.

Tasks:
- **T-17** (S; depends: phase entry) On a clean machine, confirm first run creates `%LOCALAPPDATA%\AudiobookOrganizer\` and `abo.db` (Local, not Roaming/OneDrive) - covered by `abo-core::paths` and the startup migration.
- **T-18** (M; depends: T-17) Corrupt-DB test: replace `abo.db` with garbage, relaunch, and confirm the recovery path moves it to `corrupt-backups\abo-<timestamp>.db`, recreates an empty DB, and shows the FD-04 family-safe recovery notice; confirm set-aside folders and exported reports are untouched. Harden the recovery code in `abo-core::db` only if the fresh-machine run exposes a gap (Opus-tier: safety-adjacent).
- **T-19** (M; depends: T-17) Uninstall test: run the Windows uninstaller; confirm program files are removed and `%LOCALAPPDATA%\AudiobookOrganizer\` plus any set-aside folder remain. If the NSIS/MSI uninstaller deletes app data by default, change the bundle config so it does not (retention is the documented policy, AC-9).
- **T-20** (S; depends: T-19) Cross-check the AC-9 doc section from Phase 2 matches the observed behavior.

Verification: bootstrap, recovery, and retention all observed on a clean machine and recorded. A regression test in `abo-core` covers the corrupt-DB recovery move-aside-and-recreate path if one does not already exist. Evidence: test name + QA checklist notes.

Decision Gate: N/A.

Output Artifacts: possible edits to `crates/abo-core/src/db/` (recovery) and `src-tauri/tauri.conf.json` (uninstaller retention), a recovery regression test.

Suggested Owner: LLM (Opus) for recovery/retention code; human runs the clean-machine steps.

## Phase 5: Fresh-machine end-to-end gate + no-network proof

**Goal:** run the full install-to-uninstall loop from the artifact and prove zero network use. **Addresses:** AC-10, AC-12.

Tasks:
- **T-21** (M; depends: phase entry) Prepare the canonical sample tree for the gate (OQ-2): default to a fixture export from the v0.2.0 harness so the gate needs no real library; document its location in `docs/internal/qa/v0.9.0-checklist.md`.
- **T-22** (L; depends: T-06, T-21) On a fresh Windows 11 VM: install from the NSIS/MSI artifact, scan the sample tree, generate a plan, run a dry run and export the HTML report, run a Real apply against the SAMPLE tree, roll it back, and confirm a byte-identical tree (recursive hash compare), then uninstall cleanly.
- **T-23** (M; depends: T-22) Capture network activity during the loop with an OS-level monitor (or run the whole loop on an offline VM); confirm zero outbound requests (AC-12).
- **T-24** (S; depends: T-22) Open the exported HTML report on the offline VM; confirm it renders with embedded Literata (data URI) and no external host requests (FD-11); confirm the CI external-host grep gate `ci: report-no-external-hosts` is green.
- **T-25** (S; depends: T-22, T-23, T-24) Record every step, screenshots, and the exported report in `docs/internal/qa/v0.9.0-checklist.md` under the "fresh-machine install-to-uninstall" flow.

Verification: the loop completes; rollback yields a byte-identical tree; the network capture shows nothing; the report opens offline; CI host-grep green. Evidence: QA checklist entry with attachments + CI link.

Decision Gate: if any step fails, this is a bug against the owning earlier release (executor, report, first-run); freeze the cut until fixed (no descope for a broken safety loop).

Output Artifacts: `docs/internal/qa/v0.9.0-checklist.md` (populated), a sample-tree fixture export, network-capture evidence.

Suggested Owner: human (jp or a verifier) drives the VM; LLM prepares the fixture export and the checklist skeleton.

## Phase 6: Release ceremony + D-13 human gate

**Goal:** prove the version-bump-to-draft ceremony end to end and present the D-13 decision as a human gate. **Addresses:** AC-13, AC-14.

Tasks:
- **T-26** (M; depends: phase entry) Add `scripts/bump-version.mjs` (FD-25): argument `X.Y.Z`; updates root `Cargo.toml [workspace.package]`, `src-tauri/Cargo.toml [package]`, `package.json`, `src-tauri/tauri.conf.json`; prints a confirmation that all four agree.
- **T-27** (S; depends: phase entry) Ensure `CHANGELOG.md` exists with an `[Unreleased]` section (from the v0.1.0 hygiene set, FD-25); the ceremony moves items into `## [0.9.0] - 2026-07-03` and reopens `[Unreleased]`.
- **T-28** (M; depends: T-26, T-27) Dry-run the G0-G4 runbook: G0 readiness (this spec's gate green), G1 adversarial review status, G2 bump + CHANGELOG, G2.5 commit "release: v0.9.0" and re-run the local gate, capture the sha, G3 tag on that sha + push + release.yml drafts, G4 draft edited but NOT published (human-only, D-10 autonomy boundary).
- **T-29** (S; depends: T-26) Confirm `cargo check` and `pnpm install` still pass after the bump (lockfiles updated if needed).
- **T-30** (M; depends: phase entry) Assemble the D-13 gate packet for jp: the two paths (stay private vs public flip), and if public, the license options (OQ-1: MIT vs Apache-2.0), the CONTRIBUTING/CODE_OF_CONDUCT/.github finalization step, and the Azure Trusted Signing fast-follow. Confirm public-ready drafts of LICENSE/CONTRIBUTING/CODE_OF_CONDUCT/.github exist marked pending (from the hygiene batch). Record jp's decision; do not decide it in this plan.

Verification: bump-version shows four sources agree; CHANGELOG diff correct; a draft release is produced from one captured sha with publish withheld; the D-13 packet is presented and jp's decision recorded. Evidence: bump output, CHANGELOG diff, draft link, the recorded decision in the release plan / this folder.

Decision Gate: AC-14 is a human-only decision (D-13). The tag is not cut for real (a real cut and publish are human-only per D-10) until jp signs off the whole gate.

Output Artifacts: `scripts/bump-version.mjs`, `CHANGELOG.md` update, the D-13 decision record, a (test) draft release.

Suggested Owner: LLM (Sonnet) for scripts and mechanics; human (jp) for the ceremony sign-off and the D-13 decision; Fable reviews the gate.

## Test-First Posture

- Before Phase 1: add the release.yml build to run on a test tag so the artifact path is exercised before the real cut; add the bundle-size assertion step (fails first with no build, passes once built).
- Before Phase 2: add `docs: plain-language-grep` (fails on any forbidden term or a missing canon string) before writing prose, so the docs are written to pass it.
- Before Phase 4: add the corrupt-DB recovery regression test in `abo-core` (per test-strategy.md storage layer) before touching recovery code.
- Phase 5 is itself the signature verification for this release; it names its artifacts (QA checklist flow, network capture, exported report) per the test-strategy.md evidence policy.
- Carried gates run on every merge and must be green on the release commit: rollback round-trip, kill-resume reconciliation, never-overwrite adversarial suite (test-strategy.md executor layer).

## Branch/PR Plan

Short-lived branches per phase cluster, merged into `main` via green PRs. **Merging is a human decision**: the repo went public on 2026-07-31 (FD-38), which lapsed D-11's agent self-merge allowance (EXECUTION.md governance). Green CI remains required before any merge.

- `rel/v0.9.0-release-workflow` (P1)
- `rel/v0.9.0-docs` (P2)
- `rel/v0.9.0-first-run` (P3)
- `rel/v0.9.0-appdata` (P4)
- `rel/v0.9.0-e2e-gate` (P5, mostly evidence + fixture export)
- `rel/v0.9.0-ceremony` (P6)

Required green CI per PR: lint (fmt, clippy, core-purity, typecheck, no-raw-invoke, bindings-drift), test matrix (Windows + ubuntu), Windows build+bundle, plus the new `docs: plain-language-grep` and `ci: report-no-external-hosts` checks. The tag cut itself is a human action after the gate.

## Risks and Descope Triggers

| Risk / trigger | Pre-agreed action |
|---|---|
| Unsigned installer + SmartScreen scares non-engineers | Document the flow clearly (AC-3); signing stays a fast-follow gated on the D-13 public flip and money approval (human-only). Do not block the beta on signing. |
| Bundle exceeds 30 MB | Confirm downloadBootstrapper (not embedded WebView2) and strip debug assets; the size-assertion step fails the build until fixed (AC-11). |
| macOS bundle red in CI during the cut | Per release plan Section 5: downgrade macOS to allow-fail + tracking issue; never block the Windows release on it. State the posture in release notes. |
| Fresh-machine loop (AC-10) exposes an earlier-release bug | Freeze the cut and fix forward in the owning release; a broken safety loop is not descoped. |
| D-13 decision not ready at cut time | The gate stays open; the draft release is not published. Publishing is human-only (D-10); the beta can wait on the decision without losing the built artifact. |
| macOS artifact attachment ambiguity (OQ-3) | Default: attach the unsigned macOS bundle with an "unverified on hardware" note; jp may omit it. |

## Definition of Done

The spec's release gate, restated as the exit checklist:

- [ ] release.yml produces NSIS + MSI + SHA256SUMS as a draft; bundle under 30 MB (AC-1, AC-2, AC-11).
- [ ] README + user guide + install doc pass the plain-language grep and carry the FD-10 canon copy; SmartScreen flow documented (AC-3, AC-4, AC-5).
- [ ] First-run verified on a machine with no app data; no-raw-invoke lint and capability review clean (AC-6, AC-7).
- [ ] Fresh-machine bootstrap, corrupt-DB recovery, and uninstall retention verified and documented (AC-8, AC-9).
- [ ] Fresh-machine install-to-uninstall loop completed with byte-identical rollback and zero network use; report opens offline (AC-10, AC-12).
- [ ] Version-bump ceremony proven to a draft release on one captured sha; publishing withheld for a human (AC-13).
- [ ] D-13 human gate presented and jp's decision recorded; public-ready drafts present and marked pending (AC-14).
- [ ] All carried signature gates (rollback round-trip, kill-resume, never-overwrite) green on the release commit; macOS posture stated in release notes.
