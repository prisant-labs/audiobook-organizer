---
id: v0.1.0-spine
title: "Release v0.1.0 (spine) - scaffold and tracer bullet"
type: release-spec
date: 2026-07-03
status: review
owner: jprisant
produced-by: release-spec author agent
tier: engine-first spine (no user-facing product surface)
scope: workspace scaffold, first migration, F-101/F-103/F-105/F-1003, tauri-specta seam, tracer slice, CI live, hygiene set
depends_on: none (spine is the first release on the ladder)
linked-plan: docs/internal/releases/v0.1.0-spine/implementation-plan.md
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.1.0, Section 6 CI, Section 6.4 test strategy)
  - _local/planning/feature-function-breakdown_2026-07-02.md (F-101, F-103, F-105, F-1003; Section 4 architecture; Section 7 schema; Section 8 error taxonomy)
  - PRODUCT.md (design contract, register)
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md (reference architecture)
  - docs/internal/decision-ledger.md decisions D-01, D-07, D-09, D-10, D-11, D-12, D-13; FD-11, FD-15, FD-16, FD-17, FD-19, FD-20, FD-24, FD-25, FD-29, FD-30
  - docs/internal/ci-plan.md (final CI YAML, authored by the ci-plan artifact)
  - docs/internal/test-strategy.md (test layer definitions and evidence conventions)
---

# Spec: Release v0.1.0 (spine) - scaffold and tracer bullet

## Task Summary

- Status: review (pending jp approval of the planning suite)
- Release theme: prove the whole architecture once, cheaply, on a real Windows build, before any product feature exists.
- Features in scope: F-101 (live tree scanner, minimal), F-103 (file typing, with the FD-17 video class), F-105 (snapshot persistence), F-1003 (structured app logging), plus the workspace scaffold, first migration, tauri-specta seam, tracer slice, live CI, and the FD-25 hygiene set.
- Acceptance criteria: AC-1 through AC-24 below; none checked at review time.
- Open questions: 2 (see Open Questions).
- Last updated: 2026-07-03.

## Purpose

This release exists to de-risk the architecture, not to deliver value to a user. It follows D-07 (engine-first order: abo-core hardens before any GUI) and the release plan's tracer-bullet method: one thin slice through every layer (scan to persist to event to a throwaway JSON dump), so that the load-bearing seams (the Vfs-ready core crate, the sqlx migration, the tauri-specta contract, the CI matrix) are proven working together on Windows in week one. Everything the later releases build on is scaffolded and pinned here. The user-facing product begins at v0.4.0 (seeing); nothing in this release is a product surface, and its throwaway UI is explicitly disposable [S: release plan Section 4 v0.1.0].

## Scope

In scope for v0.1.0 (spine):

1. Cargo workspace: `crates/abo-core` (zero Tauri dependencies), `src-tauri` shell, and a React + TypeScript + shadcn/ui frontend scaffold.
2. First sqlx migration creating `scans`, `entries`, `jobs`, `settings`, `activity_records`, with WAL, `%LOCALAPPDATA%` placement, and corrupt-DB startup recovery.
3. F-101 (live tree scanner), minimal walk.
4. F-103 (file typing), including the FD-17 video class.
5. F-105 (snapshot persistence): immutable snapshots with metadata.
6. F-1003 (structured app logging): `tracing` in core, `tauri-plugin-log` in the shell.
7. tauri-specta bindings generation wired, versions pinned exactly.
8. Tracer-bullet slice: `scan_start` on a small folder to entries persisted to `job:completed` event to a throwaway JSON-dump UI (disposable).
9. CI workflows land live here (final YAML copied from `docs/internal/ci-plan.md`).
10. FD-25 hygiene set and FD-29 capability-model baseline.

## Non-Goals

- No classification (E-02), parsing (E-03), planning (E-04), execution (E-06), or duplicates (E-07). Those begin in v0.2.0 (understanding) and later. Pointer: release plan Section 4.
- No product GUI surfaces (F-901..F-906). The v0.1.0 UI is a throwaway JSON dump, deleted at v0.4.0 (seeing).
- No WizTree CSV import (F-102, v0.2.0), no job cancellation UI (F-104 lands in v0.2.0; v0.1.0 only needs the `jobs` table row and a completion event).
- No cross-volume executor, no journal, no rollback: those are v0.5.0 (acting).
- No macOS behavioral claims: macOS is compiles-plus-bundles-in-CI only [S: release principle 2].

## Users / Actors

- **jp (developer/operator).** The only human actor at this stage. Runs `pnpm tauri dev` on Windows 11, clicks the tracer button, reads the JSON dump. No tier-2 (household) or tier-3 (public) surface ships in this release [S: PRODUCT.md Users].
- **CI matrix (automated gate).** Enforces core-purity, bindings-drift, tests, and Windows/macOS builds per FD-24; CI is the substitute for code review while the repo is private (D-11).

## Requirements

**Workspace and core purity.** The workspace mirrors repo-sync-tool: `[workspace] members = ["crates/abo-core", "src-tauri"]`, with abo-core holding the pure engine and never importing `tauri`, even transitively, enforced by a CI check [S: breakdown Section 4; D-01]. IPC payload structs and `AppError` live in `abo-core::ipc` and `abo-core::error`, deriving `serde` plus `specta::Type`; the `src-tauri` command layer is thin [S: reference architecture Section 4].

**Database and recovery.** SQLite via sqlx with a pool, WAL mode, and numbered migrations applied by `sqlx::migrate!`. The DB lives at `%LOCALAPPDATA%\AudiobookOrganizer\abo.db` (Local, never Roaming, never OneDrive-synced). On startup, a corrupt or unopenable DB is logged, moved aside to `corrupt-backups\`, recreated, and the user notified; this behavior is inherited verbatim from the reference architecture [S: breakdown Section 7; reference architecture App-data location]. The migration must apply cleanly from an empty DB and against an already-migrated DB (idempotent, no double-apply) [S: this brief AC additions]. Snapshot-retention scaffolding follows FD-20 (WAL, the schema's declared indexes); the keep-last-N default of 10 is a setting introduced in F-803 (app settings, v0.4.0) and is only reserved in the `settings` shape here, not yet enforced [S: FD-20].

**F-101 (live tree scanner), minimal.** A `walkdir`-based recursive traversal of a chosen root capturing per entry: path, kind (file/dir), size, mtime, depth, and parent linkage [S: breakdown F-101]. From day one the scanner uses Windows extended-length (`\\?\`) path semantics so paths beyond 260 characters open; near-260 warnings and `LongPathsEnabled=0` detection are authored in F-404 (plan validation, v0.3.0) but the extended-length open posture is a v0.1.0 scanner requirement [S: FD-19]. Permission-denied entries are recorded and skipped, never aborting the scan; reparse points and junctions are recorded and not followed [S: breakdown F-101; D-09; FD-19]. "Minimal" means: no ruleset-driven exclude globs yet (those harden in v0.2.0), no progress events beyond the single terminal `job:completed`, and no cancellation token wired to a UI (F-104 is v0.2.0).

**F-103 (file typing), with FD-17 video class.** Extension-based classification of every file into: `audio` (m4b, mp3, m4a, opus, wma, flac), `ebook` (epub, pdf, mobi, azw3, lit, pdb, docx), `image` (jpg, jpeg, png, gif, webp, bmp), `playlist` (m3u, m3u8, cue), `release-info` (nfo, sfv, txt), `weblink` (url, html, htm), `comic` (cbr, cbz), `video` (mp4, mkv, avi, mov, wmv, m4v), and `other` [S: breakdown F-103; FD-17]. (Extension lists amended 2026-07-04 during implementation with same-class additions: m3u8, htm, webp, bmp; controller-approved, no class semantics changed.) FD-17 adds the `video` class; `.mp4` maps to `video` by extension because extension alone cannot distinguish audio-in-mp4 from video-in-mp4 (container inspection is deferred). The folder-level routing of video/course-dominated folders (the Zig Ziglar case) and radio plays to `manual-review` is a classification concern that lands in v0.2.0 (F-201); v0.1.0 only owns the extension-to-class table [S: FD-17].

**F-105 (snapshot persistence).** A completed scan is immutable: one `scans` row plus its `entries` rows, with `status`, `entry_count`, `total_bytes`, `source = live`, and timestamps. Later plans reference the snapshot they were built from; the staleness guard that fails re-validation is authored in F-404 (v0.3.0), but the immutability and metadata contract is established here [S: breakdown F-105].

**F-1003 (structured app logging).** `tracing` in abo-core with structured spans/events; `tauri-plugin-log` in the shell with file rotation. No telemetry, no network, no crash reporting [S: breakdown F-1003; NFR Privacy].

**tauri-specta seam.** Commands annotated `#[tauri::command]` plus `#[specta::specta]`; every payload struct and `AppError` derives `specta::Type`; `tauri_specta::Builder` collects commands and events and exports `src/lib/bindings.ts` under `#[cfg(debug_assertions)]` so release builds do no file I/O. Tauri-family crates and tauri-specta are pinned exactly in `Cargo.toml`; the frontend uses generated bindings only, never raw `invoke` [S: reference architecture Section 4; D-01].

**FD-29 capability model baseline.** Tauri v2 minimal capabilities from day one: the frontend gets `event:default` and `core:webview:default` and no filesystem and no shell. Folder access is mediated by the backend (tauri-plugin-dialog arrives with F-909 at v0.4.0); the frontend never touches the filesystem [S: FD-29; reference architecture Section 9].

**FD-25 hygiene set.** `.gitattributes` (`* text=auto eol=lf`) verified as landed in the docs branch; `rust-toolchain.toml`; `.nvmrc`; `packageManager` pin in `package.json`; `CHANGELOG.md`; `scripts/bump-version.mjs`; and draft `LICENSE`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, and `.github/` templates, each marked pending per D-13 (OSS posture decided at v0.9.0) [S: FD-25].

**FD-15 pre-flight (OSS-landscape check).** A one-hour timeboxed survey (beets audiobook plugins, ABS community organizers, standalone renamers) is recorded before scaffold work starts; the build does not start without its recorded outcome [S: FD-15].

## Acceptance Criteria

Pre-flight (before any scaffold commit):

- [ ] AC-1: The FD-15 OSS-landscape check is recorded (a dated note listing beets audiobook plugins, ABS community organizers, and renamers surveyed, with a one-line "build/adapt/ignore" conclusion) and committed before the first scaffold task. [S: FD-15]

Workspace and core purity:

- [ ] AC-2: `cargo build --workspace` succeeds on Windows; the workspace contains `crates/abo-core` and `src-tauri` as members. [S: breakdown Section 4]
- [ ] AC-3: The core-purity CI gate passes: `cargo tree -p abo-core -e normal` contains no `tauri` entry (case-insensitive), failing the job if it does. [S: release plan Section 6.1; D-01]
- [ ] AC-4: `AppError` and every v0.1.0 IPC payload struct live in `abo-core` and derive `serde::Serialize`, `serde::Deserialize`, and `specta::Type`. [S: reference architecture Section 4]

Database and recovery:

- [ ] AC-5: The first migration creates `scans`, `entries`, `jobs`, `settings`, and `activity_records` and applies cleanly against an empty DB. [S: release plan Section 4 gate; breakdown Section 7]
- [ ] AC-6: Re-running migration against an already-migrated DB is a no-op (no error, no duplicate objects), proven by a test that opens, migrates, closes, reopens, and migrates again. [S: this brief AC additions]
- [ ] AC-7: The pool opens the DB in WAL mode at `%LOCALAPPDATA%\AudiobookOrganizer\abo.db`; a test asserts `PRAGMA journal_mode` returns `wal`. [S: breakdown Section 7; FD-20]
- [ ] AC-8: A deliberately corrupted DB file on startup is logged, moved to `corrupt-backups\`, and recreated, with the user notified via `db-corrupt-recovered`; a test drives this path. [S: breakdown Section 7 + Section 8; reference architecture]

F-101 (live tree scanner):

- [ ] AC-9: Scanning a small fixture folder yields entries whose count, kinds, sizes, and depths exactly match the fixture's known contents. [S: breakdown F-101]
- [ ] AC-10: The scanner opens paths with extended-length (`\\?\`) semantics; a test scans a fixture entry whose full path exceeds 260 characters (generated at runtime, never committed) and records it without error. [S: FD-19]
- [ ] AC-11: A permission-denied entry is recorded and the scan continues to completion (no abort); a junction/reparse point is recorded and not followed, and a scan over a fixture containing a junction loop terminates. [S: breakdown F-101; D-09]

F-103 (file typing):

- [ ] AC-12: File typing maps each catalogued extension to its class, including `video` for `.mp4`/`.mkv`/`.avi`/`.mov`/`.wmv`/`.m4v` and `comic` for `.cbr`/`.cbz`; a table-driven test covers one example per class and an unknown extension mapping to `other`. [S: breakdown F-103; FD-17]

F-105 (snapshot persistence):

- [ ] AC-13: A completed scan persists exactly one `scans` row (with `source = live`, `entry_count`, `total_bytes`, `status = completed`, and timestamps) plus one `entries` row per scanned entry, and the rows are not mutated by a subsequent scan. [S: breakdown F-105]

F-1003 (structured logging):

- [ ] AC-14: abo-core emits `tracing` events for scan start and completion; the shell writes a rotating log file via `tauri-plugin-log`; no network egress occurs during a scan (verified by the FD-11 external-host grep over the app plus the absence of any HTTP client in the dependency tree). [S: breakdown F-1003; NFR Privacy]

tauri-specta seam and capability model:

- [ ] AC-15: `src/lib/bindings.ts` is generated by tauri-specta and includes the tracer command(s) and the `job:progress`/`job:completed` events; the file is committed and the frontend imports from it, with zero raw `invoke` calls (lint-enforced). [S: reference architecture Section 4]
- [ ] AC-16: The bindings-drift CI gate regenerates `bindings.ts` and fails on any diff; its runner placement is recorded per FD-24 (runs on the Windows runner if the specta export links Tauri, else ubuntu), and the chosen placement plus rationale is written into the release-gate evidence. [S: FD-24]
- [ ] AC-17: Tauri-family crates and tauri-specta are pinned to exact versions in `Cargo.toml` (no caret/wildcard ranges). [S: D-01; reference architecture]
- [ ] AC-18: The main window carries only `event:default` and `core:webview:default` capabilities; no `fs` or `shell` capability is granted to the WebView, verified by inspecting the capability config. [S: FD-29]

Tracer slice:

- [ ] AC-19: On Windows, `pnpm tauri dev` launches; clicking the throwaway button runs `scan_start` on a small folder, persists entries, emits `job:completed`, and renders the resulting entries as a JSON dump in the UI. The UI is labeled disposable and is deleted at v0.4.0. [S: release plan Section 4 gate]

CI and hygiene:

- [ ] AC-20: The `ci.yml` and `release.yml` workflows from `docs/internal/ci-plan.md` are present and green on the v0.1.0 branch, with concurrency (cancel-in-progress), `permissions: contents: read`, and the LTO profiles per FD-24. The docs-only branch push must not have created a red CI (workflows land live only here). [S: FD-24; release plan Section 6]
- [ ] AC-21: The CI matrix is green: lint (fmt, clippy -D warnings, core-purity, typecheck, lint, bindings-drift), test (ubuntu + windows), and build (windows GA + macOS honesty). [S: release plan Section 6.1]
- [ ] AC-22: The FD-25 hygiene set is present: `.gitattributes` verified in the branch, `rust-toolchain.toml`, `.nvmrc`, `packageManager` pin, `CHANGELOG.md` with a v0.1.0 entry, `scripts/bump-version.mjs`, and draft `LICENSE`/`CONTRIBUTING.md`/`CODE_OF_CONDUCT.md`/`.github/` templates each marked "pending (D-13)". [S: FD-25]
- [ ] AC-23: `.gitignore` ignores `_local/`, `.memsearch/`, and tool caches and is machine-independent (works on a fresh clone with no personal global excludesfile); `docs/internal/` is tracked. [S: D-12; Stream 4 findings 1-2]
- [ ] AC-24: `EXECUTION.md` exists at the repo root stating the D-11 governance (trunk-based, short-lived branches, agent self-merge of green PRs while private, CI as code-review substitute). [S: D-11; release plan Section 2]

## Behavior / Examples

**Tracer slice walk-through (AC-19).** jp runs `pnpm tauri dev`. A single button "Run tracer scan" is visible. Clicking it invokes the generated binding for `scan_start` with a path to a small fixture folder (for example `E:\tmp\abo-tracer\`). The backend spawns a job on the Tokio runtime, walks the folder (F-101), types each file (F-103), writes the `scans` and `entries` rows (F-105), and emits `job:completed{job_id}`. The frontend, listening via the generated event binding, fetches the persisted entries and renders them as pretty-printed JSON. No plan, no classification, no styling beyond raw text. This is the disposable proof that scan-to-persist-to-event-to-render works across the seam on Windows.

**Corrupt-DB recovery (AC-8).** A test writes garbage bytes to `abo.db`, then invokes startup. The app logs `db-corrupt-recovered`, moves the bad file to `%LOCALAPPDATA%\AudiobookOrganizer\corrupt-backups\abo-<timestamp>.db`, creates a fresh DB, re-runs the migration, and surfaces the notice. The app is usable afterward; no scan data survives (acceptable at spine, where all data is disposable).

**File typing ambiguity (AC-12).** A `.mp4` file types as `video`, not `audio`, because the extension is ambiguous and the container-level audio-vs-video decision is deferred to v0.2.0 classification. A test asserts `type_of("book.mp4") == video` and documents this as a known conservative default.

## Non-Functional Requirements

- **Determinism.** A repeated scan of an unchanged fixture produces the same entry set (ordering normalized); this is the seed of the v0.3.0 plan-determinism discipline [S: NFR Determinism].
- **Privacy.** No network, no telemetry, no crash reporting; the FD-11 external-host grep gate runs even at spine so the zero-network posture is enforced from the first commit [S: NFR Privacy; FD-11].
- **Footprint.** Bundle target < 30 MB via the WebView2 `downloadBootstrapper`; verified at build, not gated hard until v0.9.0 [S: NFR Footprint; reference architecture].
- **Platform.** Windows 11 is the validation bar; macOS is compiles-plus-bundles-in-CI only, allow-fail per the descope trigger below [S: NFR Platform].
- **Accessibility.** N/A for this release - the throwaway UI is not a product surface and is exempt from the WCAG AA bar that binds from v0.4.0 (FD-21).

## Release Gate

The v0.1.0 tag cuts only when all of the following are green. Evidence conventions follow `docs/internal/test-strategy.md` (name the test/artifact producing each check).

- [ ] G-1: All AC-1 through AC-24 satisfied, each with its evidence pointer.
- [ ] G-2: `pnpm tauri dev` launches on Windows and the tracer slice works end to end (AC-19). Evidence: manual QA checklist entry, Windows.
- [ ] G-3: CI matrix green - lint, test (ubuntu + windows), build (windows + macOS) (AC-20, AC-21). Evidence: CI run link.
- [ ] G-4: Core-purity gate passes (AC-3). Evidence: CI log line.
- [ ] G-5: Bindings-drift gate passes and its runner placement per FD-24 is recorded in this gate (AC-16). Evidence: CI config note + rationale line. Placement decision: [to record during implementation].
- [ ] G-6: Migration applies from empty and from existing DB (AC-5, AC-6). Evidence: sqlx migration test.
- [ ] G-7: FD-15 OSS-landscape check recorded before scaffold work started (AC-1). Evidence: dated note path.
- [ ] G-8: FD-19 extended-length path posture proven in the scanner (AC-10). Evidence: over-260 scan test.
- [ ] G-9: FD-25 hygiene set present and FD-29 capability baseline in place (AC-18, AC-22, AC-23, AC-24). Evidence: file presence + capability config.
- [ ] G-10: macOS bundle either green or explicitly downgraded to allow-fail with a tracking issue (descope trigger fired and recorded), never silently red. Evidence: CI config + issue link if triggered.

## Source Traceability

| Feature / item | Discovery / planning source | Decisions (D / FD) |
|---|---|---|
| F-101 (live tree scanner, minimal) | breakdown F-101; release plan Section 4 v0.1.0 | D-07 (engine-first), D-09 (junction not followed), FD-19 (extended-length paths) |
| F-103 (file typing) | breakdown F-103 | FD-17 (video class) |
| F-105 (snapshot persistence) | breakdown F-105 | D-07 |
| F-1003 (structured logging) | breakdown F-1003 | FD-11 (zero network), FD-23 (strings module posture, later) |
| Workspace + core purity | breakdown Section 4; reference architecture Section 4 | D-01 (locked stack) |
| First migration + WAL + recovery | breakdown Section 7; reference architecture | FD-20 (SQLite scale), D-01 |
| tauri-specta seam | reference architecture Section 4 | D-01 |
| Capability model baseline | reference architecture Section 9 | FD-29 |
| CI live + gates | release plan Section 6; ci-plan.md | FD-24 (CI fixes), D-11 (CI as review) |
| Hygiene set | release plan Section 4; reference architecture | FD-25, D-12, D-13 |
| OSS-landscape pre-flight | release plan; EXECUTION.md | FD-15 |
| Effort = release folder | n/a | FD-16 |
| Model tiering (in the plan) | n/a | FD-30 |

## Revisions

| Date | Author | Change |
|---|---|---|
| 2026-07-03 | jprisant (author agent) | Initial spec authored from the planning suite. Status: review. |

## Open Questions

1. OQ-1: FD-24 leaves the bindings-drift runner placement conditional on whether the specta export links Tauri. This must be resolved empirically during implementation (AC-16 / G-5) and the outcome recorded in this spec's gate. Both options (ubuntu-latest vs windows-latest) are documented; the decision is data-driven, not pre-set.
2. OQ-2: The exact `settings` row shape (which columns to reserve now for later features like theme, roots, quarantine path, reports path, log/snapshot retention) is minimal at spine. It is reserved to a single-row `CHECK (id = 1)` table; column additions for F-803 (v0.4.0) are additive migrations, permitted pre-v1 per the sqlx policy.

## Sources & Evidence

- [A] `_local/planning/release-plan-and-ci_2026-07-02.md` - Section 4 (v0.1.0 scope + gate), Section 6 (CI), Section 6.4 (test strategy).
- [A] `_local/planning/feature-function-breakdown_2026-07-02.md` - F-101, F-103, F-105, F-1003; Section 4 (architecture), Section 7 (schema), Section 8 (error taxonomy), Section 9 (NFR).
- [A] `PRODUCT.md` - users, register, principles.
- [A] `E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md` - Section 4 (IPC/core purity), Section 9 (capability model), app-data + WebView2 decisions.
- [A] `docs/internal/decision-ledger.md` - D-01, D-07, D-09, D-10, D-11, D-12, D-13; FD-11, FD-15, FD-16, FD-17, FD-19, FD-20, FD-24, FD-25, FD-29, FD-30.
- [B] `docs/internal/ci-plan.md` - final CI YAML (companion artifact, authored separately).
- [B] `docs/internal/test-strategy.md` - test layers and evidence conventions (companion artifact).
