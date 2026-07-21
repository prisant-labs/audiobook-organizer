# Changelog

All notable changes to Audiobook Organizer are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [0.5.0] - unreleased

### Added

- F-601 (executor): applies approved plans in dependency order on Windows;
  rename-first for same-volume moves, copy+verify+delete for cross-volume.
- F-602 (journal + undo manifest): append-only journal-before-act per operation;
  JSON manifest exported to Reports so recovery never depends on a healthy database.
- F-603 (rollback): full or partial undo from a manifest, run through the same
  validate and apply pipeline as a forward tidy-up.
- F-604 (post-apply verification): verifies moved trees, refreshes snapshot, and
  blocks further campaign groups until any discrepancy is acknowledged.
- F-605 (set-aside / quarantine): move-not-delete holding area outside the library
  root, preserving relative paths and provenance; no audio file is ever deleted.
- F-607 (dry-run harness): identical executor code path running against an
  in-memory filesystem (MemFs) instead of RealFs.
- F-608 (pause and resume apply): pause an apply job between operations; the
  journal is unaffected by a pause or a stop.
- F-904 (apply + activity surface): plain-sentence live progress, journal view,
  verification results, and rollback entry point in the GUI.
- F-507 carry-through: provenance from plan time is carried into each journal entry
  and the exported manifest; the provenance report is re-emitted post-apply.

## [0.4.0] - unreleased

### Added

- F-901 (app shell + navigation): sidebar navigation, Day/Evening themes, and
  command palette; replaces the throwaway v0.1.0 tracer-bullet UI.
- F-902 (library home): cover-forward shelf rendering health facts inside sentences;
  no stat bands or hero metrics.
- F-903 (plan preview surface): hosts campaign group review, search, and
  explainability per change.
- F-502 (campaign group review): approve, reject, or defer per campaign group;
  per-change override available inside group detail.
- F-503 (search and filter): find any book, path, or rule across the plan.
- F-504 (explainability): every change shows matched pattern, rationale, and
  confidence tier.
- F-906 (settings + ruleset editor): ruleset CRUD with live re-plan preview counts.
- F-803 (app settings): library roots, set-aside root, reports folder, theme, and
  snapshot retention configurable through the UI.
- F-907 (cover extraction and fallback tiles): reads embedded art and cover.jpg
  sidecars read-only; hash-colored fallback tile when no cover exists.
- F-908 (error, empty, and loading states): every AppError family maps to a
  designed, plain-language surface with no jargon.
- F-909 (first-run and library root selection): onboarding flow using
  tauri-plugin-dialog; the frontend never touches the filesystem directly.

## [0.3.0] - unreleased

### Added

- F-401 (naming templates): configurable target patterns with three presets
  (abs-author-first default, title-first, hybrid-genre).
- F-402 (structure policies): one-book-per-folder, pack shell, sidecar, preferred
  format, and clutter policies with safe defaults.
- F-403 (plan builder): generates a deterministic, immutable change list from a
  snapshot plus a ruleset, grouped into seven user-facing campaign groups.
- F-404 (plan validation): checks collisions, path safety, source-in-target cycles,
  disk space, reserved names, and snapshot staleness before any executor sees a plan.
- F-405 (plan persistence): immutable plans with approval state; regenerating after
  a ruleset change always creates a new row, never mutates the old one.
- F-204 (disc-structure detection): recognizes disc-based books; proposes renames
  for non-conforming disc folder names.
- F-205 (parallel-format detection): flags books with both mp3 chapters and an m4b
  sibling; feeds the preferred-format policy.
- F-505 (plan export): CSV, JSON, and Markdown plan artifacts; one row per operation.
- F-506 (dry-run HTML report): self-contained, zero-network HTML report generated
  by abo-core with no GUI dependency; proven on the real 1,817-change library plan.
- F-507 (pack provenance capture): records source-pack and award membership (Hugo,
  Nebula, Top 100, Dune Universe) as durable data at plan time, plus an exported
  provenance report.
- F-701 (duplicate candidate detection): name-plus-size exact grouping across the
  snapshot; 406 groups / 856 copy files on the real library.
- F-801 (ruleset model + persistence): named bundles of naming templates and
  structure policies; JSON body against a versioned schema.
- F-1002 (reports folder): plan exports, provenance report, and HTML report all
  land as files in a configurable reports folder.

## [0.2.0] - unreleased

### Added

- Fixture harness: deterministic synthetic library from a declarative manifest,
  used as the bedrock for all golden tests.
- F-102 (WizTree CSV import): imports an existing WizTree export into the same
  snapshot schema as a live scan; rescued the 2026-03-25 14,689-row baseline.
- F-104 (job progress + cancel): progress events and cooperative cancel at safe
  boundaries; a killed scan is visible as not-completed on restart.
- F-201 (folder classification engine): assigns one of nine FolderClasses to every
  folder bottom-up; each verdict carries rule ID and evidence.
- F-202 (library health metrics): aggregate problem counts and byte totals per
  class; every metric states its counted unit.
- F-203 (multi-book detection): flags folders holding several complete books
  (Harry Potter 11-file/7-title style) without misreading disc-split single books.
- F-301 (pattern matcher set): nine ordered pure-function matchers covering the
  full naming variety found in the real library; 237 of 238 loose-root files parse.
- F-302 (noise strippers): ripper-tag, bitrate/size, rank/year, suffix, and
  underscore strippers; idempotent by property test (4096 cases).
- F-303 (field extraction with confidence): title, author, series, index, year,
  narrator with high/medium/low tiers; folder-first default confirmed by FD-14 probe.
- F-304 (name normalizer): strips illegal characters, guards reserved names,
  applies NFC normalization, and truncates at a word boundary.
- F-1001 (activity log): append-only record of every scan, import, classify, and
  parse run with parameters and outcome.
- FD-14 tag-quality probe: bounded read-only measurement on 300 real files; confirmed
  folder names are more complete and consistent than embedded tags.

## [0.1.0] - unreleased

### Added

- Workspace scaffold (Phase 1): a Cargo workspace with `crates/abo-core`
  (the Tauri-free engine crate) and `src-tauri` (the thin Tauri v2 shell),
  plus a React, TypeScript, and Vite frontend scaffold at the repo root.
- FD-25 hygiene set: `.gitattributes`, `rust-toolchain.toml`, `.nvmrc`, a
  pinned `packageManager` field, this `CHANGELOG.md`,
  `scripts/bump-version.mjs`, and draft `LICENSE`, `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, and `.github/` templates, each marked pending
  (D-13, OSS posture decided at v0.9.0).
- FD-29 capability baseline: the main window grants only
  `core:event:default` and `core:webview:default`; no filesystem, shell, or
  dialog access.
- FD-15 OSS-landscape pre-flight check recorded
  (`docs/internal/oss-landscape-check.md`) before any scaffold work began.
- Tracer slice UI (Phase 6, AC-19): a disposable single-screen React
  component (`src/App.tsx`) that runs `scan_start` on a hardcoded fixture
  folder, listens for `job:completed`/`job:failed`, and renders the
  persisted entries as pretty-printed JSON. This UI is throwaway and is
  deleted at v0.4.0 (seeing) when the real product surface lands.
