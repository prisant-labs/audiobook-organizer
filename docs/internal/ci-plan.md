---
title: Audiobook Organizer - CI/CD Plan
date: 2026-07-03
status: review
owner: jprisant
produced-by: author agent (ci-plan)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 6, the sketches upgraded here)
  - E:/Projects/product-on-purpose/repo-sync-tool/.github/workflows/ci.yml
  - E:/Projects/product-on-purpose/repo-sync-tool/.github/workflows/release.yml
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md (CI sections)
  - E:/Projects/product-on-purpose/repo-sync-tool/EXECUTION.md
  - docs/internal/decision-ledger.md, docs/internal/planning-audit-2026-07-03.md
---

# Audiobook Organizer - CI/CD Plan

This is the complete continuous-integration and release-automation plan. It upgrades the sketches in the release plan (release-plan-and-ci_2026-07-02.md, Section 6) with the hardening decided in FD-24 (CI fixes adopted from the audit) and FD-11 (zero-network fonts gate).

Important: the workflow files below are authoritative content, not live files. They land in `.github/workflows/` during v0.1.0 (spine), where the v0.1.0 implementation plan copies them verbatim. Per FD-24, a docs-only push must never create a red CI, so this document CONTAINS the YAML and the planning/doc-suite branch does not create `.github/workflows/`.

Where a crate name appears (`abo-core` for the core crate, `abo` for the Tauri shell crate under `src-tauri`), the shell crate's exact package name is confirmed when the workspace is scaffolded in v0.1.0; adjust the `-p` flags to match at that point.

## 1. Philosophy

Five ideas govern this pipeline. They inherit from the reference architecture (v1-architecture-and-decisions.md, CI gates section) and adapt to this product's safety profile.

1. CI is the substitute for code review. This is a solo agent-driven build (D-11, private-repo governance). There is no second human reviewer while the repo is private, so the required check set IS the merge policy: a green matrix is the gate, not a courtesy.
2. Required checks ARE the merge policy. The list in Section 6 (branch protection) is the contract. If a check is not in that list, it does not gate merges; if it is, it is non-negotiable.
3. Safety invariants are mechanical gates, not conventions. The D-09 (safety invariants) guarantees - quarantine-only, journal-before-act, single-writer, rename-first, never-overwrite, rollback-as-a-plan - are proven by CI jobs (core-purity, plan-determinism, rollback round-trip, hostile-fixture validation), not by reviewer vigilance.
4. Windows-first, macOS honest-in-CI. Windows 11 is the GA bar (built, human-validated, packaged). macOS is "compiles + bundles in CI" only; its build leg is allow-fail-capable via the D-10 (full-ladder) descope trigger and never blocks a Windows release.
5. Determinism is testable and tested. Byte-stable goldens (FD-25 (v0.1.0 hygiene set): `.gitattributes` `* text=auto eol=lf`), the plan-determinism golden, and the bindings-drift gate all assume reproducible output; the pinning policy in Section 7 keeps toolchains stable so those goldens do not drift under the tools.

## 2. Workflow: ci.yml

Runs on every pull request and on pushes to `main`. Three jobs - `lint`, `test`, `build` - and these three are exactly the required checks for merge. FD-24 additions over the release-plan sketch: a concurrency group with cancel-in-progress; `permissions: contents: read`; the zero-network grep gate (FD-11); the bindings-drift gate defaulted to the Windows runner; the macOS build leg made allow-fail-capable.

Implementation note (Phase 7, landed on `release/v0.1.0-spine`): the YAML below is the plan as authored. Four reality adaptations were made when the workflow actually landed, and the live `.github/workflows/ci.yml` reflects them, not the block below verbatim:

1. **Linux system dependencies, added.** This plan did not enumerate Ubuntu packages. `cargo clippy --workspace --all-targets` and `cargo test --workspace` both compile the `src-tauri` crate on the `ubuntu-latest` legs (`lint` and `test`), even though the product ships no Linux bundle (Section 8). Both jobs install the current official Tauri v2 Linux prerequisite set for Debian/Ubuntu (`libwebkit2gtk-4.1-dev`, `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, per `https://v2.tauri.app/start/prerequisites`) before the Rust toolchain step.
2. **Zero-network gate, built output instead of source globs.** The `lint` job now runs `pnpm build` and greps the produced `dist/` plus `index.html`, instead of grepping committed source files matched by `git ls-files` globs. A bundler could inline a network reference that never appears in source, so grepping the real shipped output is strictly stronger. The report-template globs still join this gate at v0.3.0.
3. **Bindings-drift step, simplified to the existing script.** The step runs `pnpm bindings:check` (the `package.json` script, which already runs `cargo test -p abo --test export_bindings` then `git diff --exit-code -- src/lib/bindings.ts`) rather than restating both commands inline. Same gate, one source of truth.
4. **macOS `continue-on-error`, not preset.** The `build` job does NOT set `continue-on-error: ${{ !matrix.ga }}` at landing. Per the pre-agreed descope rule, macOS starts as a normal, blocking leg; `continue-on-error: true` is added only if the D-10 descope trigger fires (roughly a day of effort or three genuine fix attempts), together with a tracking issue. Windows is never allow-fail, preset or otherwise. The YAML below is retained as the eventual downgraded shape, for reference once/if the trigger fires.
5. **Zero-network regex, fixed and allowlisted.** The `(?!localhost|127\.0\.0\.1)` negative lookahead below is not valid POSIX extended regex (`grep -E`'s dialect); GNU grep mis-parses it silently (a "warning: ? at start of expression," not an error) and the catch-all `https?://` half of the pattern then never matches ANY plain URL, localhost or otherwise - a false negative in a safety-relevant gate, found by reading this phase's own first green run's log rather than trusting the pass. The live workflow splits the check into two greps (no lookahead needed) and, because it now correctly greps the real built `dist/`, also allowlists two static non-network vendor strings every stock React production bundle ships: the `http://www.w3.org/...` XML/SVG namespace URIs (identifiers, never dereferenced) and React's `https://react.dev/errors/` error-decoder link prefix (shown to a human only on a thrown error, never auto-fetched). Any other external reference still fails the gate.
6. **Bundle artifact paths, corrected to the workspace root.** `src-tauri/target/debug/bundle/` (ci.yml) and `src-tauri/target/dist/bundle/` (release.yml) below assume `src-tauri` is not a workspace member. In this repo it is (`[workspace] members = ["crates/abo-core", "src-tauri"]`), so Cargo places all build output, bundle included, under the WORKSPACE ROOT's `target/`, never under `src-tauri/target/`. With the plan's paths, `actions/upload-artifact`'s default `if-no-files-found: warn` silently matched zero files and reported the step successful anyway - the artifact never existed even though the job was green. Found by checking the actual artifacts list on the first green run (zero artifacts, despite a "success" conclusion) and confirmed against the Build-and-bundle step's own log. Both workflows use `target/debug/bundle/` and `target/dist/bundle/` (workspace root) instead.

```yaml
name: ci

# Runs on PRs and on pushes to main. Keying push to main (not all branches) avoids a
# duplicate run when a feature branch push and its PR would both fire. The workflow file
# itself lands in v0.1.0 (spine); the planning/doc-suite branch does NOT carry it, so a
# docs-only push never produces a red CI (FD-24).
on:
  pull_request:
  push:
    branches: [main]

# A newer push/PR-sync to the same ref cancels the older in-flight run (FD-24).
concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

# Least privilege: CI only reads the repo. No write scope, no packages, no id-token (FD-24).
permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  CARGO_INCREMENTAL: "0"

jobs:
  # --- lint: fast, Ubuntu-only, static gates ------------------------------------------
  lint:
    name: lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable   # channel pinned via rust-toolchain.toml
        with:
          components: clippy, rustfmt

      - uses: Swatinem/rust-cache@v2

      - name: cargo fmt (check)
        run: cargo fmt --all -- --check

      - name: cargo clippy (deny warnings)
        run: cargo clippy --workspace --all-targets -- -D warnings

      # Core-purity gate (D-07 engine-first, D-09 seam): abo-core must never depend on
      # tauri, even transitively. This is the most load-bearing static assertion in the repo.
      - name: Assert abo-core has no tauri dependency
        run: |
          tree="$(cargo tree -p abo-core -e normal)"
          echo "$tree"
          if echo "$tree" | grep -qi 'tauri'; then
            echo "::error::abo-core depends on tauri (directly or transitively). The core crate must stay Tauri-free."
            exit 1
          fi
          echo "OK: abo-core is Tauri-free."

      - uses: pnpm/action-setup@v4          # version from packageManager in package.json
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      - name: pnpm typecheck
        run: pnpm typecheck

      # pnpm lint includes the no-raw-invoke ESLint rule: the frontend must call the
      # tauri-specta generated bindings, never import `invoke` from @tauri-apps/api
      # directly (FD-29 typed-IPC-only; release-plan v0.4.0 gate "no raw invoke").
      - name: pnpm lint
        run: pnpm lint

      # Zero-network gate (FD-11): the exported HTML report and the app shell must make no
      # external network requests. Literata is bundled (self-hosted woff2 in-app; subsetted
      # data URI in the report). This greps for external hosts and any Google Fonts link (a
      # prototype-only artifact that must never ship). Extend globs as files land in v0.3.0/v0.4.0.
      - name: Assert zero external network references
        run: |
          set -eu
          patterns='https?://(?!localhost|127\.0\.0\.1)|fonts\.googleapis\.com|fonts\.gstatic\.com|cdn\.|unpkg\.com|jsdelivr'
          targets=$(git ls-files \
            'crates/**/report*.html' 'crates/**/*report*.rs' \
            'src/**/*.html' 'index.html' 'src/**/*.css' 2>/dev/null || true)
          if [ -z "$targets" ]; then
            echo "No report/app-shell files present yet; gate is a no-op until v0.3.0/v0.4.0."
            exit 0
          fi
          if grep -HnEi "$patterns" $targets; then
            echo "::error::External host or font-CDN reference found. App and report must be zero-network (FD-11)."
            exit 1
          fi
          echo "OK: no external network references."

  # --- test: Rust + frontend on both platforms ---------------------------------------
  test:
    name: test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false                      # want both platform signals, not the first failure
      matrix:
        os: [ubuntu-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      # The workspace test suite. Grows across the ladder: fixture generation and golden
      # classification (v0.2.0); plan-determinism golden and hostile-fixture validation
      # (v0.3.0); MemFs executor suites plus the RealFs rollback round-trip in a temp dir
      # (v0.5.0); kill/resume reconciliation (v0.6.0). Over-length-path fixtures are
      # generated at runtime into the temp dir, never committed, so Windows checkout never
      # needs core.longpaths (FD-19).
      - name: cargo test (workspace)
        run: cargo test --workspace

      # Bindings-drift gate (stream 3 audit item 7; FD-24). Regenerate the committed bindings
      # from the live tauri-specta surface and fail if the tree drifts: a command/event/type
      # added without re-exporting would ship a frontend type surface that lies about the backend.
      # PLACEMENT DECISION (FD-24): default to the WINDOWS runner, because the export test is a
      # Tauri-linked [[test]] target needing the comctl32-v6 manifest that build.rs attaches only
      # on Windows. If v0.1.0 scaffolding confirms the specta export does NOT link Tauri (a
      # pure-data export binary), move this step to the Ubuntu lint job. Verify in v0.1.0; default
      # to Windows to be safe.
      - name: Assert bindings are up to date
        if: matrix.os == 'windows-latest'
        shell: bash
        run: |
          cargo test -p abo --test export_bindings
          if ! git diff --exit-code -- src/lib/bindings.ts; then
            echo "::error::src/lib/bindings.ts is stale. Run the export_bindings test and commit the result."
            exit 1
          fi

      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      # Vitest: approval-state logic plus the axe-core accessibility smoke on primary
      # surfaces (FD-21). The mechanical token-contrast check is a separate cargo/node
      # script wired into pnpm test from v0.4.0.
      - name: pnpm test
        run: pnpm test

  # --- build: bundle on both platforms; Windows is the GA bar -------------------------
  build:
    name: build (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - { os: windows-latest, ga: true }   # the real GA bar
          - { os: macos-latest,  ga: false }   # honesty only: compiles + bundles
    # macOS is allow-fail-capable: this is the D-10 / release-plan descope trigger made
    # mechanical. While macOS is green it stays required; if the macOS bundle fights for
    # more than the trigger window, set continue-on-error true here (or remove macOS from
    # required checks) and file a tracking issue. Windows is NEVER allow-fail.
    continue-on-error: ${{ !matrix.ga }}
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      # Per-push CI builds use the thin-LTO [profile.release] so bundling stays fast
      # (Cargo.toml; FD-24). Full-LTO [profile.dist] is reserved for release-tag artifacts
      # in release.yml. macOS bundling is UNSIGNED on purpose (FD-22, D-13): signing is
      # human-only and decided at the public flip.
      - name: Build and bundle
        run: pnpm tauri build --debug

      - name: Upload Windows bundle (inspection only)
        if: matrix.ga
        uses: actions/upload-artifact@v4
        with:
          name: windows-debug-bundle
          path: src-tauri/target/debug/bundle/
```

## 3. Workflow: release.yml

Fires only on version tags (`v*`), so it never runs on a normal push. It builds both platforms with the full-LTO `dist` profile, emits `SHA256SUMS`, and creates a DRAFT GitHub Release. Publishing the draft is a human-only action (D-11 governance, autonomy boundary).

```yaml
name: release

on:
  push:
    tags: ["v*"]

# Needs write to create the GitHub Release. This is the only workflow with write scope,
# and it produces a DRAFT only; a human publishes it.
permissions:
  contents: write

jobs:
  release:
    name: release (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2

      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      # Full-LTO distribution artifacts (Cargo.toml [profile.dist]). Windows produces
      # NSIS/MSI; macOS produces .app/.dmg, UNSIGNED (FD-22). Confirm how --profile dist
      # threads through the tauri action against its current README before the first cut.
      - name: Build release bundles
        run: pnpm tauri build --profile dist

      # SHA256SUMS over every produced bundle so the family install doc can verify the
      # download. bash is available on both runners in Actions.
      - name: Generate SHA256SUMS
        shell: bash
        run: |
          set -eu
          out="SHA256SUMS-${{ matrix.os }}.txt"
          find src-tauri/target/dist/bundle -type f \
            \( -name '*.msi' -o -name '*.exe' -o -name '*.dmg' -o -name '*.app.tar.gz' \) \
            -print0 | xargs -0 sha256sum > "$out" || true
          cat "$out"

      - name: Upload to draft release
        uses: softprops/action-gh-release@v2
        with:
          draft: true                        # publishing is human-only (D-11)
          prerelease: true
          files: |
            src-tauri/target/dist/bundle/**/*.msi
            src-tauri/target/dist/bundle/**/*.exe
            src-tauri/target/dist/bundle/**/*.dmg
            SHA256SUMS-${{ matrix.os }}.txt
          body: |
            See CHANGELOG.md for this version's notes.
            Windows is the primary supported target. The installer is UNSIGNED through
            v0.9.0 (FD-22): on first run Windows SmartScreen shows "Windows protected your
            PC"; choose "More info", then "Run anyway". The install doc explains this flow.
            The macOS build is UNSIGNED and beta (compiles + bundles honesty only).
```

Unsigned posture (FD-22): the installer ships unsigned through v0.9.0 for private/family distribution; the install doc explains the SmartScreen "More info, then Run anyway" path. Code signing (Azure Trusted Signing on Windows; notarization on macOS) is decided with the public flip at v0.9.0+ (D-13), and no signing secrets live in CI while the repo is private (Section 9). No auto-update in v1 (FD-22, fully offline posture); users download new installers manually.

## 4. Workflow: scheduled.yml (honesty cron, from v0.3.0)

A campaign tool sits idle for weeks between bursts. A weekly cron of the full matrix on `main` catches ecosystem drift (new clippy lints, Tauri patch releases, runner-image changes) before the next active session. A failure auto-files a tracking issue; it never pages anyone. This workflow lands with v0.3.0 (planning), the first release stable enough to be worth watching between bursts.

```yaml
name: scheduled

on:
  schedule:
    - cron: "0 6 * * 1"   # 06:00 UTC every Monday
  workflow_dispatch: {}    # allow a manual honesty run

permissions:
  contents: read
  issues: write            # only to file a drift issue on failure

jobs:
  matrix:
    name: honesty (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy, rustfmt }
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version-file: .nvmrc
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm typecheck && pnpm lint && pnpm test

  report-failure:
    name: file drift issue
    needs: matrix
    if: failure()
    runs-on: ubuntu-latest
    permissions:
      issues: write
    steps:
      - uses: actions/github-script@v7
        with:
          script: |
            const title = `Scheduled honesty run failed (${new Date().toISOString().slice(0,10)})`;
            await github.rest.issues.create({
              owner: context.repo.owner,
              repo: context.repo.repo,
              title,
              labels: ['ci-drift'],
              body: `The weekly honesty matrix failed on \`main\`.\n\nRun: ${context.serverUrl}/${context.repo.owner}/${context.repo.repo}/actions/runs/${context.runId}\n\nLikely ecosystem drift (clippy, Tauri patch, runner image). Triage before the next work burst.`
            });
```

## 5. Gate registry

Every gate: what it proves, the release it starts in, where it runs, and whether it is descopeable. Gates are mechanical expressions of the D-09 (safety invariants) and the release-plan acceptance gates. Acceptance criteria themselves live in the release specs (standing rule 4); this table only records the CI mechanism.

| Gate | What it proves | Starts | Where it runs | Descopeable |
|---|---|---|---|---|
| cargo fmt | Formatting drift cannot accumulate | v0.1.0 | ci.yml lint | No |
| clippy -D warnings | No lint debt, no warnings shipped | v0.1.0 | ci.yml lint | No |
| Core-purity (cargo tree) | abo-core is Tauri-free (D-07, D-09 seam) | v0.1.0 | ci.yml lint | No |
| pnpm typecheck | Frontend types sound | v0.1.0 | ci.yml lint | No |
| pnpm lint + no-raw-invoke | Frontend uses typed bindings only (FD-29) | v0.1.0 | ci.yml lint | No |
| Bindings-drift | IPC contract matches committed bindings (planning audit stream 3 item 7, docs/internal/planning-audit-2026-07-03.md) | v0.1.0 | ci.yml test (Windows; see FD-24 rule) | No |
| cargo test (workspace) | Unit and integration suites pass both platforms | v0.1.0 | ci.yml test matrix | No |
| Windows build + bundle | The GA bar compiles and packages | v0.1.0 | ci.yml build (windows) | No |
| macOS build + bundle | Cross-platform honesty (compiles + bundles) | v0.1.0 | ci.yml build (macos) | Yes (allow-fail per D-10 trigger) |
| Schema migration apply | Migrations apply from empty and existing DB | v0.1.0 | ci.yml test (in cargo test) | No |
| Zero-network grep (FD-11) | Report and app shell make no external requests | v0.3.0 (report), v0.4.0 (shell) | ci.yml lint | No |
| Plan-determinism golden | Same snapshot + ruleset = byte-identical plan (D-09) | v0.3.0 | ci.yml test (in cargo test) | No |
| Hostile-fixture validation | Every seeded hazard (collision, cycle, over-length, reserved name, insufficient space) is caught | v0.3.0 | ci.yml test (in cargo test) | No |
| Contrast token check (FD-21) | All token pairs pass WCAG AA in both themes | v0.4.0 | ci.yml test (script in pnpm test) | No |
| axe-core smoke (FD-21) | Primary surfaces pass automated a11y checks | v0.4.0 | ci.yml test (Vitest) | No |
| Rollback round-trip | Apply then roll back is byte-identical (recursive hash) in a temp dir - the executor's signature gate (D-09) | v0.5.0 | ci.yml test (RealFs, on every merge) | No (release freezes if flaky) |
| Never-overwrite adversarial | Executor halts on target-appeared with a consistent journal (D-09) | v0.5.0 | ci.yml test (in cargo test) | No |
| Kill/resume reconciliation | Abort between journal-intent and act reconciles on restart, both directions (FD-02, D-09) | v0.6.0 | ci.yml test (in cargo test) | No |
| Hash-verified dedupe (BLAKE3) | Candidate-only hashing; keeper/loser resolution correct | v0.6.0 | ci.yml test (in cargo test) | Descope to flag-only per release-plan trigger |

## 6. Branch protection and merge policy

Mirrors the reference EXECUTION.md governance and D-11 (private-repo self-merge). Configured on `main` when the repo is scaffolded in v0.1.0.

- Trunk-based: `main` is the default branch; all work on short-lived feature branches; PRs into `main`.
- Required status checks (must be green before merge, exactly): `lint`, `test (ubuntu-latest)`, `test (windows-latest)`, `build (windows-latest)`. The `build (macos-latest)` leg is required while green but is the sanctioned allow-fail per the D-10 descope trigger; if downgraded it drops off this list and gets a tracking issue.
- Require branches up to date before merging (linear history against `main`).
- Merge policy (D-11): while the repo is private, the agent may self-merge a green PR (CI is the reviewer). If the repo ever flips public (human-only, D-13), merges to `main` become human-reviewed.
- No force-push to `main`; no history rewrites (both are on the D-10 / EXECUTION.md human-only allowlist).
- Human-only allowlist that gates merges and publishes: any Real (non-dry-run) apply against the actual library, publishing releases/tags, the public flip, spending money (signing certificate), and history rewrites (D-10).

## 7. Version pinning policy

Reproducible toolchains keep the goldens (plan-determinism, bindings-drift) honest. Pins mirror the repo-sync posture where it exists in that repo, and extend it where this project needs more.

- Rust toolchain: `rust-toolchain.toml` with `channel = "stable"` and `components = ["clippy", "rustfmt"]` (verified present in repo-sync). Lands in v0.1.0. If a determinism golden ever drifts across stable releases, pin an exact dated stable channel and record why.
- Node: `.nvmrc` consumed by `node-version-file` in every workflow (repo-sync sets node-version 22 inline; this project centralizes it in `.nvmrc` so app and CI share one source). Node 22 LTS.
- pnpm: `packageManager` field in `package.json` via corepack (repo-sync uses `pnpm@10.33.4`); `pnpm/action-setup@v4` reads it, so no second version input anywhere. `pnpm install --frozen-lockfile` everywhere; `pnpm-lock.yaml` is committed.
- Tauri family + tauri-specta: exact-pinned in `Cargo.toml` (no caret ranges) per the reference posture. Version-pinning friction is a known v0.1.0 risk (release-plan); budget for it.
- Byte-stability: `.gitattributes` with `* text=auto eol=lf` (FD-25) so goldens are byte-stable across Windows and Linux checkouts. Lands in the docs branch now (FD-25).
- Dependency automation: recommend Dependabot, weekly, grouped (one PR per ecosystem: cargo, npm, github-actions), agent triages and self-merges the green grouped PR under D-11. Repo-sync has no `dependabot.yml` or Renovate config visible in its repo, so this is a proposal, not an inherited pattern. A starting `.github/dependabot.yml` lands with the v0.1.0 workflow set.

## 8. WebKitGTK smoke: deliberate non-adoption

A Linux WebKitGTK smoke job (launching the WebView on Ubuntu to catch rendering breakage) is deliberately NOT adopted (stream 3 audit item 10; FD-24). Rationale: this is a quiet Windows-first desktop utility with no tray popup and no Linux target; the macOS `build` leg already exercises a WebKit-family WebView (WKWebView) for compile-and-bundle honesty. Adding a headless GTK smoke would spend CI time on a platform the product never ships to.

Revisit condition (FD-24): adopt it only if GUI divergence appears - specifically if a WebView-rendering bug is observed on macOS/WKWebView that Windows/WebView2 does not show, indicating the WebKit family needs its own automated smoke. Until then this stays a recorded non-adoption, not an oversight.

## 9. What CI does NOT do

Explicit non-goals, so their absence is never read as a gap:

- No telemetry or analytics upload. The product is fully offline (FD-22); CI collects no usage data and uploads none.
- No signing secrets while private. Code-signing custody is on the D-10 human-only allowlist; no signing certificate or notarization credential lives in CI through v0.9.0 (FD-22, D-13). macOS and Windows artifacts are unsigned by design until the public-flip decision.
- No publish step. release.yml creates a DRAFT only; a human publishes the GitHub Release and pushes the tag (D-11). CI never publishes, never flips the repo public, never spends money.
- No Real apply against the actual library, ever, from CI. The executor is exercised only against fixtures and temp-dir copies (D-09, D-10). The 297 GB library is a human-operated campaign target (M-1), never a CI target.
- No auto-update channel (FD-22): no update-manifest generation, no update endpoint. Users download new installers manually.

## 10. Related hygiene landing in v0.1.0

Recorded here so the v0.1.0 implementation plan wires them alongside the workflows (FD-25):

- `.gitattributes` (`* text=auto eol=lf`) and `.gitignore` land NOW on the docs branch (FD-25); the rest of the hygiene set lands in v0.1.0.
- `scripts/bump-version.mjs` (or equivalent) stamps the version into `src-tauri/tauri.conf.json`; release.yml artifacts inherit that single version across both platforms. Lands in v0.1.0.
- `CHANGELOG.md` lands in v0.1.0; release.yml notes reference it.
- Draft `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `.github/` templates land marked pending the D-13 public-flip decision.
