---
title: Runbook - Cut a Tag and Publish a Release
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (governance batch)
sources:
  - docs/internal/decision-ledger.md
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/release-plans/runbook_cut-tag-release.md
  - docs/internal/program-roadmap.md
  - EXECUTION.md
---

# Runbook: cut a tag and publish a release

The tag-cutting ceremony for Audiobook Organizer, adapted from the repo-sync-tool 6-gate runbook for a Tauri desktop app. Six gates, G0 through G4. No gate may be bypassed; each is a deliberate go/no-go.

This is the EXECUTE and NOTES layer. The PLAN layer is the release's own folder at `docs/internal/releases/<version>-<codename>/` (spec.md plus implementation-plan.md; see README.md in this folder for how the two relate).

## Scope reminder

Per D-10 (scope of "go"), the full ladder (v0.1.0 through v0.6.0, plus v0.9.0) runs without stopping for approval at each version boundary once jp approves the executive summary. Each version boundary still produces a non-blocking release report (this runbook), and this runbook still runs in full at each boundary: "non-blocking" describes the decision to proceed to the next release, not a skip of the gates below. The hard stops are human-only regardless of position on the ladder: any Real (non-dry-run) apply against the actual library, publishing releases or tags, the public flip, spending money, and history rewrites (D-10). G4 below is one of those human-only stops.

## Preconditions

- Clean working tree, on the release branch (or `main` once the repo is public and the merge policy flips per D-11, existing private repo governance).
- The release's own readiness (its `spec.md` acceptance criteria and `implementation-plan.md` definition of done) has been reviewed and you understand what is red.

## G0: Pre-tag readiness

- [ ] The release folder's acceptance criteria are all addressed (checked against `docs/internal/releases/<version>-<codename>/spec.md`) and the implementation plan's definition of done is met.
- [ ] CI is green on the release commit, on the Windows runner (the only platform this product ships; see EXECUTION.md and the D-01 stack lock). No macOS behavioral claim exists for this product; there is no macOS bundle job to gate on.
- [ ] The GitHub milestone `vX.Y.Z` is at 100% (every tracked issue for the release closed), if issues are in use.
- [ ] No open blocker-labelled issues for this milestone.
- [ ] `docs/internal/program-roadmap.md` Section 8 (effort tracking table) shows this release's row ready to flip from "planned" to "shipped".

**Blocking rule:** any red gate or non-green CI stops the cut. Fix or explicitly waive; a waiver is a documented decision (see No-bypass policy below), never a silent skip.

## G1: Adversarial review status

- [ ] The release has had its adversarial (Codex or equivalent) review, with findings fixed-in-release or filed with an owning release on the roadmap.
- [ ] No unaddressed high-severity finding remains open for in-scope work.
- [ ] Safety releases name their signature gate explicitly and confirm it green, per FD-30 model-tiering (Opus subagents own safety-critical implementation and adversarial verification):
  - v0.5.0 (acting): rollback round-trip byte-identical, both on fixtures AND on a real-data copy (D-09 safety invariants; roadmap Section 5, "not descopeable").
  - v0.6.0 (hardening): kill-during-apply reconciles in both directions (resume and rollback), hash-verified dedupe on copies.

## G2: Version bump and CHANGELOG

- [ ] Run the bump-version script (repo-sync's `scripts/bump-version.mjs` equivalent, scaffolded at v0.1.0 per FD-25). Confirm all version sources agree: workspace `Cargo.toml` (`[workspace.package]`), `src-tauri/Cargo.toml` (`[package]`), `package.json`, and `src-tauri/tauri.conf.json`.
- [ ] `cargo check` and the frontend package install still succeed after the bump (lockfiles updated if needed).
- [ ] In `CHANGELOG.md`, move the `[Unreleased]` items into a new `## [X.Y.Z] - YYYY-MM-DD` section; leave a fresh empty `[Unreleased]`.

## G2.5: Commit release-prep and re-verify

- [ ] Commit the version bump and CHANGELOG as a single "release: vX.Y.Z" commit.
- [ ] Re-run the local gate: `cargo check`/`clippy`/`test`/`fmt`, the core-purity check (`abo-core` never imports `tauri`, per D-07 engine-first order), frontend typecheck/lint/build, and the FD-11 zero-network grep gate (no external hosts in the app or the exported HTML report).
- [ ] **Capture the exact commit sha.** The tag goes on THIS sha and only this sha.

## G3: Tag and push

- [ ] Create the annotated tag on the captured sha: `git tag -a vX.Y.Z -m "Audiobook Organizer vX.Y.Z"`.
- [ ] Push the tag: `git push origin vX.Y.Z`.
- [ ] The release workflow fires on the `v*` tag: builds Windows with the `dist` profile (full LTO per FD-24 CI fixes) and creates a DRAFT GitHub Release with the Windows artifacts attached.

**Windows-first, one artifact set.** The product ships an unsigned NSIS/MSI installer through v0.9.0 (private and household distribution; FD-22). Attach the installer plus a `SHA256SUMS` file. Code signing (Azure Trusted Signing) is decided together with the public flip at v0.9.0-plus (D-13); until then, the install doc explains the SmartScreen "More info, then Run anyway" flow. There is no macOS artifact: macOS compiles-in-CI only, with no behavioral claim (standing rule 7).

## G4: Post-tag hygiene [HUMAN]

Publishing is human-only per D-10 (hard stops list: "publishing releases/tags"). Everything in this gate is performed by jp, not by an agent, even if an agent prepared the draft.

- [ ] Edit the draft Release: paste the `CHANGELOG.md` vX.Y.Z section as the body; confirm the Windows installer and `SHA256SUMS` are attached; state the unsigned-installer posture and the SmartScreen workaround link.
- [ ] Publish the Release. [HUMAN]
- [ ] Set the release folder's `spec.md` frontmatter `status: released`.
- [ ] Flip this release's row in `docs/internal/program-roadmap.md` Section 8 from "planned" to "shipped"; record any descope-trigger outcomes from Section 5 that fired during the release.
- [ ] Open a fresh `[Unreleased]` section in `CHANGELOG.md` (if not already).
- [ ] Wrap the session (`/jp-wrap-session`).

## No-bypass policy

No gate is skipped to "save time." A waiver is a maintainer (jp) decision recorded in the release folder's spec.md Open Questions or Decisions section, with a reason. A silent skip is not a waiver. This mirrors D-10: the only sanctioned "skip" is the pre-agreed descope triggers in `docs/internal/program-roadmap.md` Section 5, and those are recorded there, not invoked ad hoc at cut time.

## Rollback semantics

If a published release is broken: delete the tag (`git push origin :vX.Y.Z`) and the GitHub Release, fix forward on the branch, and re-cut as the next patch (`vX.Y.Z+1`). Do not re-point an existing tag at a new sha. A tag is immutable once it has been public. This is distinct from PRODUCT rollback (F-603, undoing a completed apply against the audiobook library): tag rollback is a git and GitHub Release operation only.
