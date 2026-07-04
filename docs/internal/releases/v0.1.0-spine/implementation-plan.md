---
id: v0.1.0-spine-plan
title: "Implementation Plan: Release v0.1.0 (spine)"
type: implementation-plan
date: 2026-07-03
status: review
owner: jprisant
produced-by: implementation-plan author agent
linked-spec: docs/internal/releases/v0.1.0-spine/spec.md
linked-release: docs/internal/releases/v0.1.0-spine/
depends_on: none (spine is the first release)
phase-count: 8
ac-coverage: complete
executor-model-guidance: >
  Per FD-30 (model tiering): Opus-tier owns the safety-adjacent and correctness-critical
  seams even at spine - the sqlx migration + corrupt-DB recovery, the scanner's
  extended-length/junction/permission handling, and the tauri-specta contract wiring.
  Sonnet-tier owns mechanical scaffolding - workspace layout, the file-typing extension
  table, hygiene files, and CHANGELOG/template stubs. Fable reviews the release gate and
  the two decision gates (bindings-drift placement, macOS descope).
sources:
  - docs/internal/releases/v0.1.0-spine/spec.md
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md
  - docs/internal/ci-plan.md
  - docs/internal/test-strategy.md
---

# Implementation Plan: Release v0.1.0 (spine)

## Task Summary

- Status: review (pending jp approval).
- Goal: scaffold the workspace, land the first migration, prove F-101/F-103/F-105/F-1003 through a tracer slice, and turn CI green on Windows.
- Phases: 8 (Phase 0 pre-flight through Phase 7 release gate).
- AC coverage: complete (every AC-1..AC-24 mapped to at least one phase).
- Model tiers assigned per phase (FD-30). Last updated: 2026-07-03.

## Completion Status

| Phase | Goal | Fulfills AC | Owner | Status |
|---|---|---|---|---|
| P0 | Pre-flight: OSS-landscape check + branch | AC-1 | LLM (Sonnet) + Fable review | Not started |
| P1 | Workspace scaffold + core purity + hygiene | AC-2, AC-3, AC-22, AC-23, AC-24 | LLM (Sonnet) | Not started |
| P2 | DB: migration, WAL, corrupt-DB recovery | AC-5, AC-6, AC-7, AC-8 | LLM (Opus) | Not started |
| P3 | F-101 scanner + F-103 typing + F-105 persistence | AC-9, AC-10, AC-11, AC-12, AC-13 | LLM (Opus) | Not started |
| P4 | Logging, IPC types, error taxonomy | AC-4, AC-14 | LLM (Opus) | Not started |
| P5 | tauri-specta seam + capability baseline | AC-15, AC-16, AC-17, AC-18 | LLM (Opus) + Fable gate | Not started |
| P6 | Tracer slice UI (disposable) | AC-19 | LLM (Sonnet) | Not started |
| P7 | CI live + green + release gate | AC-20, AC-21, plus gate G-1..G-10 | LLM (Sonnet) + Fable review | Not started |

## Test-First Posture

Per `docs/internal/test-strategy.md` layers, write these tests before (or alongside) the implementation they cover:

- P2: `migrate_from_empty`, `migrate_idempotent` (re-apply is a no-op), `wal_mode_enabled`, `corrupt_db_recovers` (Rust integration tests against a temp `%LOCALAPPDATA%`).
- P3: `scan_counts_match_fixture`, `scan_over_260_char_path`, `scan_records_permission_denied`, `scan_terminates_on_junction_loop`, `file_typing_table` (table-driven), `snapshot_immutable`.
- P4: compile-time proof that `AppError` and payloads derive `specta::Type`; `no_network_during_scan` (dependency-tree assertion + FD-11 grep).
- P5: bindings-drift check (`pnpm bindings:check` = regenerate + `git diff --exit-code`); `no_raw_invoke` lint; capability-config assertion.
- P6: manual QA checklist entry (Windows), tracer slice end to end.

Frontend Vitest component testing does not begin until v0.4.0 (seeing); at spine the throwaway UI rides the typed seam with only the manual check.

## Phase 0: Pre-flight (OSS-landscape check + branch)

**Goal:** satisfy the FD-15 gate before any code exists. **Addresses:** AC-1.

**Steps:**
1. Create the feature branch `release/v0.1.0-spine` off `main` (D-11 trunk-based; branch first).
2. Timebox one hour: survey beets audiobook plugins, Audiobookshelf community organizer scripts, and standalone audiobook renamers. Record findings in `docs/internal/oss-landscape-check.md` with a dated header and a one-line "build / adapt / ignore" conclusion per tool.
3. Commit the note. No scaffold task starts until this file exists (FD-15 hard gate).

**Verification:** `docs/internal/oss-landscape-check.md` exists on the branch with a 2026-07-03 date and at least three tools surveyed. **Decision Gate:** if the survey finds a tool that materially subsumes the plan, stop and surface to jp before scaffolding. Otherwise proceed. **Output Artifacts:** `docs/internal/oss-landscape-check.md`. **Suggested Owner:** LLM (Sonnet) research, Fable reviews the conclusion.

## Phase 1: Workspace scaffold, core purity, hygiene

**Goal:** stand up the Cargo workspace, the frontend scaffold, and the FD-25 hygiene set. **Addresses:** AC-2, AC-3, AC-22, AC-23, AC-24.

**Steps:**
1. Create `Cargo.toml` with `[workspace] members = ["crates/abo-core", "src-tauri"]`.
2. Scaffold `crates/abo-core/` (`src/lib.rs`, `src/error.rs`, `src/ipc.rs`, `src/scan/`, `src/db/`, `src/paths.rs`) with zero `tauri` dependency.
3. Scaffold `src-tauri/` (`src/main.rs`, `src/commands/`, `src/events.rs`) and the React + TS + shadcn/ui frontend under `src/` via `pnpm create tauri-app` conventions; add `tauri.conf.json`.
4. Add hygiene files: `rust-toolchain.toml`, `.nvmrc`, `packageManager` pin in `package.json`, `CHANGELOG.md` (v0.1.0 entry), `scripts/bump-version.mjs`.
5. Add draft `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `.github/` templates (issue + PR), each with a "pending (D-13)" banner.
6. Write `.gitignore` (ignore `_local/`, `.memsearch/`, target/, node_modules/, tool caches; do NOT ignore `docs/internal/`) - machine-independent. Confirm `.gitattributes` (`* text=auto eol=lf`) is present from the docs branch.
7. Write `EXECUTION.md` at repo root with the D-11 governance rules and the standing no-em-dash / Co-Authored-By conventions.

**Verification:** `cargo build --workspace` succeeds; `cargo tree -p abo-core -e normal | grep -i tauri` is empty; hygiene files present; `.gitignore` correct on a fresh clone. **Decision Gate:** N/A. **Output Artifacts:** workspace tree, hygiene set, `EXECUTION.md`. **Suggested Owner:** LLM (Sonnet).

## Phase 2: Database - migration, WAL, corrupt-DB recovery

**Goal:** land the first sqlx migration and the safe startup path. **Addresses:** AC-5, AC-6, AC-7, AC-8.

**Steps:**
1. In `crates/abo-core/migrations/`, add `0001_init.sql` creating `scans`, `entries`, `jobs`, `settings` (single-row `CHECK (id = 1)`), and `activity_records` per breakdown Section 7, with the declared indexes on `entries (scan_id, parent_id)` and `(scan_id, path)`.
2. In `abo-core/src/db/mod.rs`, build the sqlx `SqlitePool` with `PRAGMA journal_mode=WAL`; apply `sqlx::migrate!()` on open.
3. In `abo-core/src/paths.rs`, resolve `%LOCALAPPDATA%\AudiobookOrganizer\abo.db` (Windows) behind a single path function; macOS branch returns `~/Library/Application Support/...` for CI-compiles honesty.
4. Implement corrupt-DB startup recovery: on open/migrate failure, log, move the file to `corrupt-backups\abo-<timestamp>.db`, recreate, re-migrate, and surface `AppError::db-corrupt-recovered`.
5. Write the P2 tests listed above (temp-dir-scoped app-data root).

**Verification:** all four P2 tests pass on Windows and ubuntu. **Decision Gate:** N/A. **Output Artifacts:** `migrations/0001_init.sql`, `db/mod.rs`, `paths.rs`, db tests. **Suggested Owner:** LLM (Opus) - safety-adjacent recovery path.

## Phase 3: Scanner (F-101), file typing (F-103), persistence (F-105)

**Goal:** the engine slice that produces a persisted snapshot. **Addresses:** AC-9, AC-10, AC-11, AC-12, AC-13.

**Steps:**
1. In `abo-core/src/scan/walk.rs`, implement a `walkdir`-based traversal capturing path, kind, size, mtime, depth, parent linkage; open with extended-length (`\\?\`) semantics on Windows.
2. Record permission-denied entries and continue; record reparse points/junctions without following them (loop-safe).
3. In `abo-core/src/scan/typing.rs`, implement the extension-to-class table including the FD-17 `video` class; default unknowns to `other`.
4. In `abo-core/src/scan/persist.rs`, write the `scans` row and `entries` rows atomically; mark the scan `completed`; expose a `scan_get` read for the tracer.
5. Add a small committed fixture folder plus a runtime-generated over-260-char path fixture (never committed) for AC-10.
6. Write the P3 tests listed above.

**Verification:** P3 tests pass; scan of the fixture yields exact expected counts and types; junction-loop scan terminates. **Decision Gate:** N/A. **Output Artifacts:** `scan/walk.rs`, `scan/typing.rs`, `scan/persist.rs`, fixtures, scan tests. **Suggested Owner:** LLM (Opus) - filesystem edge correctness.

## Phase 4: Logging, IPC types, error taxonomy

**Goal:** structured logging and the typed contract's Rust side. **Addresses:** AC-4, AC-14.

**Steps:**
1. In `abo-core/src/error.rs`, define the `AppError` enum (thiserror) seeding the Scan and Storage families from breakdown Section 8 (`root-not-found`, `permission-denied`, `junction-skipped`, `db-migration-failed`, `db-corrupt-recovered`); derive `serde` + `specta::Type`.
2. In `abo-core/src/ipc.rs`, define v0.1.0 payload structs (`ScanSummary`, `EntryRow` or equivalent) deriving `serde` + `specta::Type`.
3. Wire `tracing` in abo-core (spans for scan start/complete); wire `tauri-plugin-log` in `src-tauri` with file rotation.
4. Add the `no_network_during_scan` assertion and confirm no HTTP client is in the dependency tree.

**Verification:** compiles with all derives; logging test observes scan events; FD-11 grep finds no external hosts. **Decision Gate:** N/A. **Output Artifacts:** `error.rs`, `ipc.rs`, logging setup. **Suggested Owner:** LLM (Opus).

## Phase 5: tauri-specta seam + capability baseline

**Goal:** freeze the typed IPC contract and lock the security surface. **Addresses:** AC-15, AC-16, AC-17, AC-18.

**Steps:**
1. Annotate the tracer command(s) with `#[tauri::command]` + `#[specta::specta]`; define `job:progress` / `job:completed` events.
2. In `src-tauri/src/main.rs`, collect commands + events via `tauri_specta::Builder` and export `src/lib/bindings.ts` under `#[cfg(debug_assertions)]`.
3. Pin tauri-family crates and tauri-specta to exact versions in `Cargo.toml` (remove any caret/wildcard).
4. Configure minimal capabilities: main window gets `event:default` + `core:webview:default`, no `fs`, no `shell`.
5. Add `pnpm bindings:check` (regenerate + `git diff --exit-code`) and the `no_raw_invoke` lint.
6. Empirically determine whether the specta export links Tauri; set the bindings-drift job runner (windows vs ubuntu) per FD-24 and record the choice + rationale in the spec's gate G-5.

**Verification:** `bindings.ts` generated and committed; bindings-drift check passes; capability config asserts no fs/shell; versions pinned. **Decision Gate:** bindings-drift runner placement (FD-24) - record outcome in spec G-5; Fable reviews. **Output Artifacts:** `bindings.ts`, capability config, pinned `Cargo.toml`, bindings-drift script. **Suggested Owner:** LLM (Opus) + Fable gate.

## Phase 6: Tracer slice UI (disposable)

**Goal:** prove the full seam end to end on Windows. **Addresses:** AC-19.

**Steps:**
1. Build a single-screen React component with a "Run tracer scan" button calling the generated `scan_start` binding on a small folder.
2. Listen for `job:completed` via the generated event binding; on completion, call the read binding and render entries as pretty-printed JSON.
3. Label the component clearly disposable (comment + on-screen note) and file a v0.4.0 deletion reminder.

**Verification:** `pnpm tauri dev` on Windows; click runs scan, persists, emits event, renders JSON. Manual QA checklist entry recorded. **Decision Gate:** N/A. **Output Artifacts:** throwaway UI component, QA checklist entry. **Suggested Owner:** LLM (Sonnet).

## Phase 7: CI live, green, release gate

**Goal:** land the CI workflows live and clear the release gate. **Addresses:** AC-20, AC-21; release gate G-1..G-10.

**Steps:**
1. Copy the final `ci.yml` and `release.yml` from `docs/internal/ci-plan.md` into `.github/workflows/`, including concurrency (cancel-in-progress), `permissions: contents: read`, thin-LTO release profile + full-LTO dist profile (FD-24).
2. Ensure the lint job runs fmt, clippy `-D warnings`, core-purity, typecheck, frontend lint, and bindings-drift; the test job runs on ubuntu + windows; the build job runs windows (GA) + macOS (honesty).
3. Iterate until the matrix is green on the branch PR.
4. If the macOS bundle fights for more than a day, downgrade that job to allow-fail and file a tracking issue (descope trigger; record in gate G-10).
5. Walk the spec's release gate G-1..G-10, attach evidence pointers, and hand to Fable for the gate review.

**Verification:** CI matrix green (or macOS allow-fail recorded); all AC checkboxes satisfied with evidence; gate reviewed. **Decision Gate:** macOS descope (record in G-10); Fable signs the gate. **Output Artifacts:** `.github/workflows/ci.yml`, `.github/workflows/release.yml`, green CI run, completed gate. **Suggested Owner:** LLM (Sonnet) for CI iteration, Fable for the gate.

## Branch / PR Plan

- One short-lived branch `release/v0.1.0-spine` off `main`; phase-clustered commits, or sub-branches per phase cluster (P1-P2 scaffold+db, P3-P5 engine+seam, P6-P7 tracer+CI) merged via green PRs.
- Merge policy: agent self-merges green PRs while the repo is private (D-11); CI is the code-review substitute.
- Required green checks before merge: lint (incl. core-purity + bindings-drift), test (ubuntu + windows), build (windows; macOS green or allow-fail). No force-push, no hook bypass. Reference: `EXECUTION.md`.
- CI workflows land live in this release only; the earlier docs-only branch must not have introduced a red CI (FD-24).

## Risks and Descope Triggers

| Risk / trigger | Pre-agreed action |
|---|---|
| Tauri v2 + tauri-specta version-pinning friction | Budget half a day (known from repo-sync); if unresolved, freeze on the last-known-good pinned set and file a follow-up. [S: release plan Section 4 risks] |
| macOS bundle red in CI > 1 day | Downgrade the macOS build job to allow-fail + tracking issue; never block the Windows spine on it (record in gate G-10). [S: release plan Section 5; NFR Platform] |
| bindings-drift runner placement wrong (specta links Tauri) | Resolve empirically in P5; move the job to the Windows runner per FD-24 and record in G-5 (OQ-1). |
| Extended-length path handling harder than expected on the runner | The over-260 fixture is generated at runtime and never committed, so checkout never breaks; if the scan test is flaky, isolate to a Windows-only test and document. [S: FD-19; release plan Section 6.1 note] |
| Corrupt-DB recovery interacts badly with WAL sidecar files (`-wal`, `-shm`) | P2 recovery moves the sidecars alongside `abo.db`; add a test covering their presence. [S: reference architecture app-data decision] |

## Definition of Done

The spec's release gate, restated as the exit checklist:

- [ ] All AC-1..AC-24 satisfied with evidence pointers (G-1).
- [ ] Tracer slice works end to end on Windows (G-2).
- [ ] CI matrix green; macOS green or recorded allow-fail (G-3, G-10).
- [ ] Core-purity gate passes (G-4).
- [ ] Bindings-drift gate passes; runner placement recorded per FD-24 (G-5).
- [ ] Migration applies from empty and existing DB (G-6).
- [ ] FD-15 OSS-landscape check recorded before scaffold started (G-7).
- [ ] FD-19 extended-length path posture proven in the scanner (G-8).
- [ ] FD-25 hygiene set present; FD-29 capability baseline in place (G-9).
- [ ] Fable has reviewed and signed the release gate.
