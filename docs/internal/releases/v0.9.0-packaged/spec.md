---
id: v0.9.0
title: "Release v0.9.0 (packaged) - distribution and docs (beta)"
type: spec
date: 2026-07-03
status: review
owner: jprisant
produced-by: release-spec author agent
tier: release
scope: distribution, user documentation, first-run polish, fresh-machine verification, release ceremony
depends_on: v0.6.0 (hardening) - executor, rollback, interruption safety, dedupe, and ruleset portability all landed and green
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.9.0, Section 6.2 release.yml, Section 9 NFRs)
  - _local/planning/feature-function-breakdown_2026-07-02.md (F-909 via FD-05, F-803, NFR table Section 9)
  - PRODUCT.md (register, safety model, plain-language principles)
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/release-plans/runbook_cut-tag-release.md (G0-G4 ceremony)
  - docs/internal/test-strategy.md (evidence policy, manual QA checklist convention)
  - docs/internal/design-system.md (copy canon, plain-language register)
  - docs/internal/decision-ledger.md decisions: D-10, D-11, D-13, FD-05, FD-11, FD-22, FD-25, FD-29
---

# Spec: Release v0.9.0 (packaged)

## Task Summary

Package the hardened engine and GUI into something a person who has never seen the project can install and trust. This release produces an unsigned Windows installer from the release workflow, a plain-language README and user guide covering the whole pipeline and the safety model, first-run polish verified on a machine that never saw the app, and a proven end-to-end release ceremony. It also reaches the D-13 (license and public-flip) decision point, which this spec presents as a human-only gate and does not decide. Nothing here adds engine capability; v0.9.0 proves that everything already built survives contact with a clean machine and a non-engineer.

Status: review (pending jp approval of the suite). AC checklist:

- [ ] AC-1 installer artifact builds from release.yml (NSIS + MSI, unsigned)
- [ ] AC-2 draft GitHub Release created with artifacts + SHA256SUMS; publish stays human-only
- [ ] AC-3 SmartScreen "More info, then Run anyway" flow documented in the install doc
- [ ] AC-4 README + user guide cover the full pipeline in the plain-language register
- [ ] AC-5 safety model (nothing deleted, everything undoable) documented with the FD-10 canon copy
- [ ] AC-6 first-run on a fresh machine: pick a root, default ruleset, default theme, no library assumed
- [ ] AC-7 frontend never touches the filesystem; folder access via tauri-plugin-dialog only
- [ ] AC-8 fresh %LOCALAPPDATA% bootstrap and corrupt-DB recovery verified on a clean machine
- [ ] AC-9 uninstall data-retention policy stated and verified (app data + set-aside retained)
- [ ] AC-10 fresh-machine end-to-end gate: install, scan, plan, dry-run + report, Real-apply on sample, roll back, uninstall
- [ ] AC-11 bundle size under 30 MB (NFR)
- [ ] AC-12 packaged build makes zero network requests (FD-11)
- [ ] AC-13 version-bump ceremony works end to end (bump-version + G0-G4)
- [ ] AC-14 D-13 human gate presented (license + public flip); public-ready drafts in place, marked pending

Open questions: 3. Last updated: 2026-07-03.

## Purpose

Everything before this release proved safety on fixtures, copies, and the developer's own machine. v0.9.0 answers a different question: can a household member (tier 2 in PRODUCT.md) download one file, install it, and complete a real tidy-up without ever seeing a path, an exit code, or a stack trace, and can the maintainer cut and publish that file through a repeatable ceremony. The release also forces the D-13 (OSS posture) decision, because the license and the public-flip choice change what ships in the installer's neighborhood (LICENSE, CONTRIBUTING, signing).

This is a beta: unsigned, Windows-only as the human-validated bar, no auto-update. It is the last release before v1.0.0 (ga), which is stabilization only.

## Scope

In scope for v0.9.0 (release-scoped deliverables, each with per-deliverable acceptance criteria below):

- DEL-1 (installer artifact): NSIS + MSI bundles produced by release.yml, unsigned per FD-22.
- DEL-2 (user documentation): README plus a user guide covering the pipeline and safety model in the plain-language register, including the SmartScreen install flow.
- DEL-3 (first-run experience polish): F-909 (first-run and library root selection) verified and polished on a machine that never saw the app.
- DEL-4 (fresh-machine app-data lifecycle): %LOCALAPPDATA% bootstrap, corrupt-DB recovery, and the uninstall data-retention policy verified fresh.
- DEL-5 (release ceremony): scripts/bump-version plus the G0-G4 runbook proven end to end to a draft release, with publishing human-only.
- DEL-6 (D-13 decision gate): present license and public-flip options to jp as a human gate; keep public-ready drafts in place; do not decide.

## Non-Goals

- No new engine or GUI capability. If a surface is missing, it is a bug against an earlier release, not new scope here.
- No code signing. Azure Trusted Signing is a fast-follow that unlocks only if the D-13 gate flips public and money is approved (human-only, D-10 autonomy boundary). See v1.x.
- No auto-update. v1 is fully offline; new versions are manual downloads (FD-22). Revisit post-1.0.
- No macOS GA. macOS stays compiles-and-bundles-in-CI honesty only; the release notes state the posture (release plan Section 4 v1.0.0, runbook G4).
- No v1.0.0 (ga) work and no M-1 (campaign) operational steps. v1.0.0 is covered by the roadmap; M-1 has its own runbook (docs/internal/releases is not where either lands).
- The D-13 license text and public flip are not decided in this spec. This spec only frames the gate.

## Users / Actors

- Fresh installer (tier 2, non-engineer): downloads one file, clicks through SmartScreen, picks a folder, runs a tidy-up. Sets the bar for AC-3, AC-4, AC-6.
- Maintainer (jp, tier 1): cuts the tag, runs the ceremony, decides D-13, publishes the draft release. Sets the bar for AC-2, AC-13, AC-14.
- Verifier (agent or jp on a clean VM): runs the fresh-machine end-to-end gate (AC-10) and records evidence.

## Requirements

The release workflow already exists in sketch form (release plan Section 6.2): on a `v*` tag, build Windows and macOS with the dist profile and create a draft GitHub Release with artifacts plus SHA256SUMS [S1]. v0.9.0 promotes that sketch to a verified artifact: the NSIS and MSI bundles must actually install on a clean Windows 11 machine, and the draft-then-human-publish boundary is enforced by D-10 (autonomy boundary) and D-11 (governance) [S6].

User documentation must speak the plain-language register the design system fixes (books, shelves, copies, tidy-up, set aside) and must never use the forbidden vocabulary (operations, ops, dedupe, manifest, quarantine, dashboard) on user-facing pages [S3, S4]. The safety-model section uses the FD-10 canon copy verbatim: "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone." [S6].

First-run is F-909 (first-run and library root selection), which first landed in v0.4.0 per FD-05: onboarding picks a library root via tauri-plugin-dialog with no library assumed, defaults the ruleset to abs-author-first and the theme to day, and Settings (F-803, app settings) hosts re-selection; the frontend never touches the filesystem directly [S2, S6]. v0.9.0 does not re-implement F-909; it verifies and polishes it on a machine that has never run the app, where the "no persisted root" path is real rather than simulated. The Tauri capability model (FD-29) forbids fs/shell exposure to the WebView, so folder access flows through the dialog plugin and the backend re-allows persisted roots at startup [S6].

App-data behavior on a fresh machine: the database lives at `%LOCALAPPDATA%\AudiobookOrganizer\abo.db` (Local, never Roaming, never OneDrive-synced) and the corrupt-DB startup recovery (log, move aside to `corrupt-backups\`, recreate, notify) must be observable on a clean install, not just a developer box [S2]. Uninstall must leave a stated, documented policy: application data and any set-aside (quarantine) folder are retained, because a user who uninstalls mid-campaign must not lose their undo trail [S6, model-inference from D-09 safety invariants].

Non-functional constraints that become checkable only when packaged: bundle size under 30 MB using the evergreen WebView2 downloadBootstrapper (NFR table) [S2], and zero network requests at runtime and in the exported report per FD-11 (Literata self-hosted, report fonts embedded as data URIs, CI greps for external hosts) [S6].

The release ceremony is the repo-sync G0-G4 runbook adapted for this repo: `node scripts/bump-version.mjs X.Y.Z` makes four version sources agree (root `Cargo.toml [workspace.package]`, `src-tauri/Cargo.toml [package]`, `package.json`, `src-tauri/tauri.conf.json`), CHANGELOG.md moves `[Unreleased]` into a dated section, the tag goes on one captured sha, release.yml produces the draft, and a human publishes [S5, S6].

The D-13 decision point: OSS posture is private now, with the license and the public flip decided at v0.9.0+ as a human-only action; docs and hygiene are written public-ready, and LICENSE/CONTRIBUTING/CODE_OF_CONDUCT/.github land as drafts marked pending until that decision [S6]. This spec presents the options; it does not choose.

## Acceptance Criteria

DEL-1 (installer artifact):

- AC-1: release.yml, triggered by a `v*` tag, produces both an NSIS installer and an MSI for Windows, unsigned, using the dist (full-LTO) profile. [S1, FD-22, FD-24]
- AC-2: the same run creates a draft GitHub Release with both Windows artifacts and a SHA256SUMS file attached; the release is never auto-published (publishing is human-only). [S1, D-10, D-11]
- AC-11: the produced Windows bundle is under 30 MB (measured on the release artifact, WebView2 downloadBootstrapper). [S2 NFR]

DEL-2 (user documentation):

- AC-3: the install doc walks a non-engineer through the SmartScreen "More info, then Run anyway" flow for the unsigned installer, with a screenshot or exact button labels. [FD-22]
- AC-4: README plus a user guide (in `docs/`, user-facing) describe the full pipeline (scan, review, dry run + report, tidy-up, undo) entirely in the plain-language register, with zero forbidden-vocabulary terms on user-facing pages (verified by a copy grep against the design-system word list). [S3, S4, design-system copy canon]
- AC-5: the safety-model section states the FD-10 canon guarantee verbatim ("No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.") and explains set-aside and undo without the word "quarantine". [FD-10]

DEL-3 (first-run experience polish):

- AC-6: on a machine with no prior app data, first launch presents onboarding that picks a library root via the folder dialog (no library assumed), sets the ruleset to abs-author-first and the theme to day, and lands the user on the library home with one primary action. [FD-05, FD-07]
- AC-7: no filesystem access originates in the frontend; all folder selection uses tauri-plugin-dialog and all mutations go through typed IPC (lint-enforced no-raw-invoke; capability manifest exposes no fs/shell to the WebView). [FD-29]

DEL-4 (fresh-machine app-data lifecycle):

- AC-8: on a clean machine, the app creates `%LOCALAPPDATA%\AudiobookOrganizer\` and its database on first run; a deliberately corrupted DB triggers the recovery path (moved to `corrupt-backups\`, recreated, user notified with the FD-04 corrupt-DB family-safe surface) without data loss to set-aside or reports. [S2, FD-04]
- AC-9: uninstalling the app leaves application data and any set-aside folder in place; the uninstall behavior and the retention policy are documented in the user guide. [S6, D-09]

DEL-5 (release ceremony) and cross-cutting fresh-machine gate:

- AC-10: on a fresh Windows 11 machine or clean VM, a verifier completes the full loop from the artifact: install, scan a sample tree, generate a plan, run a dry run and export the HTML report, run a Real apply against the SAMPLE tree, roll it back to a byte-identical tree, then uninstall cleanly. Evidence recorded per the manual QA checklist. [S1, S6]
- AC-12: the packaged build makes zero network requests during the AC-10 loop (verified by an OS-level network monitor or offline-machine run) and the exported report opens with no network access. [FD-11]
- AC-13: `node scripts/bump-version.mjs X.Y.Z` makes all four version sources agree, `cargo check` and `pnpm install` still pass, CHANGELOG.md rolls `[Unreleased]` into a dated section, and the G0-G4 runbook runs to a draft release on one captured sha with publishing withheld for a human. [S5, FD-25]

DEL-6 (D-13 decision gate):

- AC-14: the release gate presents jp with the D-13 options (stay private vs public flip; if public, the license choice and the CONTRIBUTING/CODE_OF_CONDUCT/.github finalization plus the signing fast-follow) as a recorded human decision; public-ready LICENSE/CONTRIBUTING/CODE_OF_CONDUCT/.github drafts exist in the repo marked pending; the spec does not choose. [D-13, FD-22]

## Behavior / Examples

First-run on a clean machine (AC-6): the app opens with no snapshot and no root. The onboarding screen says, in the day theme, something like "Point me at your audiobooks to get started" with a single "Choose folder" button that opens the OS folder picker. After a choice, the app persists the root through settings (F-803) and shows the library home (FD-07), never a raw path on the primary surface (the friendly location name is shown; raw path lives behind "Show file details" per FD-13).

Corrupt-DB recovery on a fresh machine (AC-8): a tester replaces `abo.db` with garbage bytes and relaunches. The app detects the unreadable database, moves it to `corrupt-backups\abo-<timestamp>.db`, recreates an empty database, and shows the family-safe recovery notice from FD-04 ("We could not read your saved information, so we started fresh. Your books and files were not touched."). No set-aside folder or exported report is affected.

Uninstall (AC-9): the user runs the Windows uninstaller. Program files are removed; `%LOCALAPPDATA%\AudiobookOrganizer\` and any `E:\Books - Audio\Quarantine\` set-aside folder remain. The user guide states this plainly so a mid-campaign uninstall does not read as data loss.

Ceremony (AC-13): the maintainer runs `node scripts/bump-version.mjs 0.9.0`, confirms the four sources match, moves CHANGELOG `[Unreleased]` items into `## [0.9.0] - 2026-07-03`, commits "release: v0.9.0", captures the sha, tags `v0.9.0` on it, pushes, and watches release.yml build the draft. The draft is left unpublished pending the AC-14 decision and jp's manual publish.

## Non-Functional Requirements

- Footprint: Windows bundle under 30 MB (AC-11), evergreen WebView2 downloadBootstrapper (NFR table, S2).
- Privacy: no network, no telemetry, verified on the packaged build (AC-12, FD-11).
- Platform: Windows 11 is the human-validated bar; macOS is compiles-and-bundles-in-CI only, stated in release notes (S1, S2).
- Accessibility: first-run and recovery surfaces meet the WCAG AA bar carried from FD-21 (both themes, status never color-alone); verified via the axe-core smoke and the manual keyboard walkthrough in the QA checklist.
- Distribution honesty: unsigned installer; SmartScreen behavior documented rather than hidden (FD-22).

## Release Gate

Composite checklist that must be green before `v0.9.0` tags (release plan v0.9.0 gate, upgraded per FD dispositions). Evidence pointers follow docs/internal/test-strategy.md conventions (each item names its artifact); the manual items live in docs/internal/qa/v0.9.0-checklist.md.

- [ ] AC-1, AC-2, AC-11: release.yml run link + attached artifacts + measured bundle size recorded in the release report.
- [ ] AC-10 fresh-machine end-to-end loop completed on a clean VM; screenshots + the exported HTML report attached (docs/internal/qa/v0.9.0-checklist.md, flow "fresh-machine install-to-uninstall").
- [ ] AC-12 no-network verification: network-monitor capture or offline-machine confirmation attached; CI external-host grep gate green (test `ci: report-no-external-hosts`).
- [ ] AC-4, AC-5 copy audit: forbidden-vocabulary grep clean on user-facing docs; FD-10 canon string present verbatim (test/script `docs: plain-language-grep`).
- [ ] AC-6, AC-7 first-run verified on a machine with no app data; no-raw-invoke lint green; capability manifest reviewed (no fs/shell to WebView).
- [ ] AC-8, AC-9 fresh app-data bootstrap, corrupt-DB recovery, and uninstall retention verified and recorded.
- [ ] AC-13 ceremony dry-run: bump-version output showing four sources agree; CHANGELOG diff; draft release from one captured sha (runbook G0-G4).
- [ ] AC-14 D-13 human gate: jp's recorded decision (private vs public); public-ready drafts present and marked pending.
- [ ] All prior-release signature gates re-verified per v1.0.0 posture note: rollback round-trip green, kill-resume reconciliation green, never-overwrite adversarial suite green (carried from v0.5.0/v0.6.0 gates; they run on every merge and must be green on the release commit).
- [ ] macOS posture stated in release notes (compiles-and-bundles-in-CI, unsigned), not blocking the Windows cut.

## Source Traceability

| Deliverable / AC | Discovery / planning source | D/FD decisions |
|---|---|---|
| DEL-1 installer (AC-1, AC-2, AC-11) | release plan Section 4 v0.9.0; Section 6.2 release.yml; Section 9 NFR footprint | FD-22 (unsigned), FD-24 (dist LTO), D-10/D-11 (draft, human publish) |
| DEL-2 docs (AC-3, AC-4, AC-5) | release plan Section 4 v0.9.0 (README/docs); PRODUCT.md register + safety model | FD-10 (canon copy), FD-22 (SmartScreen), design-system copy canon (D-05/D-06 lineage) |
| DEL-3 first-run (AC-6, AC-7) | feature-function breakdown F-909 (via FD-05), F-803; release plan Section 4 v0.9.0 onboarding | FD-05 (F-909), FD-07 (library home), FD-13 (paths), FD-29 (capabilities) |
| DEL-4 app-data lifecycle (AC-8, AC-9) | feature-function breakdown Section 4 (%LOCALAPPDATA%, corrupt-DB recovery), Section 7 | FD-04 (recovery surface), D-09 (safety invariants -> retention) |
| DEL-5 ceremony + gate (AC-10, AC-12, AC-13) | release plan Section 4 v0.9.0 gate; runbook_cut-tag-release G0-G4; Section 9 privacy | FD-25 (bump-version, CHANGELOG), FD-11 (no-network), D-10 (human publish) |
| DEL-6 D-13 gate (AC-14) | release plan Section 4 (signing posture); strategy brief OSS posture | D-13 (license + flip, human-only), FD-22 (signing fast-follow) |

## Revisions

| Date | Change | By |
|---|---|---|
| 2026-07-03 | Initial spec drafted from release plan Section 4 v0.9.0 and the v0.9.0 brief. | author agent |

## Sources & Evidence

- [S1] release plan Section 6.2 (release.yml, draft release, SHA256SUMS) - Class A (ratified planning doc).
- [S2] feature-function breakdown Sections 4, 7, 9 (%LOCALAPPDATA%, corrupt-DB recovery, NFR footprint/privacy) - Class A.
- [S3] PRODUCT.md (register, plain-language principles, safety model) - Class A (design contract).
- [S4] the suite's standing rules (plain-language register; AC live in specs) - Class A.
- [S5] runbook_cut-tag-release.md G0-G4 (reference ceremony) - Class B (adapted from another repo).
- [S6] docs/internal/decision-ledger.md decisions D-10, D-11, D-13, FD-04, FD-05, FD-07, FD-10, FD-11, FD-22, FD-24, FD-25, FD-29 - Class A (ratified ledger).

## Open Questions

- OQ-1 (D-13 license choice): if jp flips public, which license (candidates to present: MIT, Apache-2.0)? Decided by jp at the AC-14 gate, not here.
- OQ-2 (sample tree for AC-10): which sample tree is canonical for the fresh-machine gate, a fixture export or a small copied real subset? Resolve in the implementation plan against the fixture harness (test-strategy.md); default to a fixture export so the gate needs no real library.
- OQ-3 (macOS artifact in the same tag): the runbook stamps one version onto both platforms; confirm whether the v0.9.0 draft attaches the unsigned macOS bundle for honesty or omits it. Default: attach with a stated "unsigned, unverified on hardware" note.
