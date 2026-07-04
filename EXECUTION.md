---
title: EXECUTION.md - How Audiobook Organizer gets built and shipped
date: 2026-07-03
status: review
owner: jprisant
produced-by: author agent (execution contract)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 2 governance, Section 6 CI)
  - E:/Projects/product-on-purpose/repo-sync-tool/EXECUTION.md (structure and voice reference)
  - PRODUCT.md (safety model, plain-language register)
  - docs/internal/decision-ledger.md (D-nn decision ledger, FD-nn fixes)
  - docs/internal/planning-audit-2026-07-03.md (stream 3 and stream 4 findings)
---

# EXECUTION.md - How Audiobook Organizer gets built and shipped

This is the operating contract between the human operator (jp) and the AI agent(s) building Audiobook Organizer. It defines what an agent may do on its own, what must stop and hand off to jp, how merges and releases are gated, and which model tier owns which class of work. It is adapted from the repo-sync-tool EXECUTION.md and tightened for this product's risk profile: this tool moves a human's irreplaceable 297 GB library, so the safety invariants below are contract, not convention.

## 1. Roles

1. Operator (human): jp. Holds money, legal identity, publishing authority, and the real library. Final approver for everything on the human-only list (Section 3).
2. Orchestrator: Fable. Program-level planning, cross-release synthesis, gate reviews, final verification, and executive reporting. Fable escalates to jp only at a human-only gate.
3. Implementer agents: Opus and Sonnet subagents. Drive code, tests, CI, and pull requests, autonomous everywhere outside the human-only list, subject to the merge policy and the model-tiering policy (Section 5).

## 2. The boundary, in one rule

Anything that spends money, asserts a legal identity, publishes to the world, changes the real library, or cannot be cleanly undone stays with jp. Everything upstream of that line is agent-autonomous.

## 3. Human-only list (stop and hand off, with reason)

Per D-10 (full-ladder go), the release plan Section 2, D-13 (OSS posture), D-17 (backup posture), and the audit additions, these actions STOP and hand to jp:

| Action | Why it is human-only |
| --- | --- |
| Any Real (non-dry-run) apply against the actual library at E:\Books - Audio | Irreversible in practice; this is the campaign, and the campaign belongs to jp |
| The M-1 backup posture choice (D-17) | Nothing Real runs until jp records a backup decision; the M-1 gate stays open until then |
| Publishing a GitHub Release / cutting a public release tag | Publishing; users will install it; effectively irreversible |
| Flipping the repo from private to public | Publishing decision; irreversible in practice (D-13) |
| License choice and public-flip terms at v0.9.0+ (D-13) | Legal identity and licensing commitment |
| Storing signing or notarization secrets in CI | Custody of credentials and legal responsibility for their use |
| Spending money: code-signing certificate (Azure Trusted Signing), Apple Developer Program enrollment | Money plus organizational/identity validation |
| Any force-push or history rewrite on a shared branch | Irreversibility; destroys recoverable history |

## 4. Agent-safe list (proceed without asking)

1. Scaffold the Cargo workspace, `crates/abo-core`, `src-tauri` shell, and the React/TS/shadcn frontend.
2. Write and refactor all Rust and TypeScript source and all unit, integration, UI, and property tests.
3. Iterate CI locally and in GitHub Actions until the matrix is green.
4. Create short-lived feature branches, commit, push, open pull requests, and self-merge green PRs while the repo is private (Section 6).
5. Run fixture campaigns and dry-run applies anywhere.
6. Run Real applies against fixtures and disposable copies only, never against E:\Books - Audio.
7. Build unsigned local artifacts for inspection.
8. Draft the non-blocking per-release report (Section 8) for jp.

## 5. Model-tiering policy (FD-30, jp directive)

Work is routed by risk and complexity, not by convenience:

1. Fable (orchestrator): program-level planning, cross-release synthesis, gate reviews, final verification, executive reporting.
2. Opus subagents: safety-critical implementation - the abo-core executor (F-601), journal and undo (F-602), rollback (F-603), plan validation (F-404) - plus complex authorship and adversarial verification passes.
3. Sonnet subagents: mechanical implementation - boilerplate, table-driven parser tests, UI wiring from frozen designs, doc formatting and conversions.
4. Escalation rule: any agent uncertain about a safety invariant (Section 9) STOPS and escalates to Fable. Fable escalates to jp only at a human-only gate (Section 3). Uncertainty about a safety invariant is never resolved by guessing.

## 6. Governance: branches, merges, CI gates

1. Branching: trunk-based; `main` is the default branch; all work on short-lived feature branches; PRs into `main`. Branch first, always.
2. Merge policy, tiered by visibility (D-11, private-repo self-merge): while the repo is private, an agent may self-merge a PR once every required check is green; CI is the substitute for code review. The moment the repo is public (a human-only action), merges to `main` require human review.
3. Required green checks before any merge are exactly the ci-plan gate list, per release plan Section 6.1 and FD-24 (CI shape fixes):
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo test --workspace` (fixtures, golden plans, parser tables, MemFs suites, and from v0.5.0 the RealFs rollback round-trip)
   - `pnpm typecheck` and `pnpm lint`
   - core-purity gate: `cargo tree -p abo-core` shows no `tauri` dependency, even transitively (D-07, engine-first order)
   - bindings-drift gate: regenerate the tauri-specta output and `git diff --exit-code` (runs on the Windows runner if the specta export links Tauri; verify placement during v0.1.0, FD-24)
   - build matrix: Windows (the real GA bar - launches, human-validated, packaged) and macOS (compiles plus bundles in CI only, honesty clause)
   - zero-network gate: grep the app and the exported HTML report template for external hosts; fail on any (FD-11, zero-network fonts)
4. CI shape (FD-24): a concurrency block with cancel-in-progress; `permissions: contents: read`; thin-LTO `[profile.release]` for per-push CI and full-LTO `[profile.dist]` for release artifacts. The WebKitGTK smoke job is a recorded deliberate non-adoption unless GUI divergence appears.
5. Live CI workflow files land in the v0.1.0 (spine) release, not in the docs-only branch. A docs-only push must never create a red CI (FD-24).
6. No force-push, ever, without an explicit jp go-ahead (Section 3).

## 7. Release cadence

1. Work proceeds release by release along docs/internal/program-roadmap.md: v0.1.0 (spine) through v0.6.0 (hardening), then M-1 (campaign) and v0.9.0 (packaged), per the full-ladder go (D-10).
2. Each release ends with a non-blocking release report to jp containing gate evidence per docs/internal/test-strategy.md. jp can interject at any boundary; execution continues unless jp stops it.
3. Effort unit is the RELEASE (FD-16): each release owns one tracked folder docs/internal/releases/<version>-<codename>/ holding spec.md and implementation-plan.md. Epics E-01..E-11 stay taxonomy inside the PRD and breakdown only; no E-NN id is used for governance, avoiding collision with the repo-sync effort namespace.
4. Acceptance criteria live in specs (release folders). The roadmap and release plan aggregate and reference AC; they never author it.

## 8. Standing rules

1. Never use em-dashes (U+2014) or en-dashes (U+2013) anywhere. Use " - ", commas, colons, or sentence breaks; numeric ranges use plain hyphens (2-5). A PreToolUse hook rejects violations.
2. Every reference ID carries its handle on first use in a section: "F-506 (dry-run HTML report)", "D-17 (backup posture)", never a bare ID.
3. Plain-language register in every user-facing surface: books, shelves, copies, tidy-up, set aside. Never operations, ops, dedupe, manifest, quarantine, or dashboard in UI copy (FD-07, standing rules 3).
4. Commit messages end with the Co-Authored-By trailer and the session link per jp's global rules.
5. Session logs are written via /jp-wrap-session at the end of each session.
6. `_local/`, `.memsearch/`, and tool caches are gitignored and never committed. `docs/internal/` IS tracked in git (D-12), correcting the release plan Section 2 quarantine line.
7. Windows-first: examples use Windows paths; macOS is compiles-in-CI honesty only.

## 9. Safety invariants (restated as contract)

These are non-negotiable and mechanically gated where possible (release plan Section 6.4):

1. Quarantine-only: no audio file is ever deleted anywhere in the product. Only empty folders are removed. Files are set aside, never destroyed (D-09, FD-10).
2. Journal-before-act: every intent is flushed to the journal before the filesystem is touched (F-602).
3. Never-overwrite: a target that appears mid-apply halts the group with `target-appeared`; the executor never clobbers (D-09).
4. Single-writer: exactly one apply job runs process-wide at a time (D-09).
5. Dry-run-first: the dry run is the same executor against the Vfs MemFs seam; a browsable confirmation screen and an exportable self-contained HTML report exist before anything executes (D-04, D-09).
6. Rollback is just another plan: undo runs through the same validate, preview, apply pipeline (D-09).
7. D-17 backup gate: the M-1 backup posture must be recorded before any Real apply against the real library. No exceptions, regardless of how green CI is.

## 10. Pre-flight checklist (roadmap)

The build does not start until all three are recorded:

1. FD-15 OSS-landscape timeboxed check complete (1 hour: beets audiobook plugins, ABS community organizers, renamers) with its outcome written into docs/internal/program-roadmap.md.
2. v0.1.0 hygiene set landed (FD-25): `.gitignore` and `.gitattributes` now in the docs branch; the remaining hygiene items land in the v0.1.0 spine.
3. Executive summary approved by jp ("go", D-10).
