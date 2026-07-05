---
title: Audiobook Organizer - V1 Architecture and Decisions
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (architecture)
sources:
  - _local/planning/feature-function-breakdown_2026-07-02.md (Sections 2-11)
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 6)
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md
  - PRODUCT.md
  - docs/internal/decision-ledger.md, docs/internal/planning-audit-2026-07-03.md
---

# Audiobook Organizer - V1 Architecture and Decisions

This is the engineering contract for the v1 build. It plays the role that repo-sync-tool's `v1-architecture-and-decisions.md` plays there: everything an implementing agent needs to build correctly without re-deriving decisions. Where a rule is inherited from the reference architecture, this document cites the section it comes from and states the deliberate deviation, if any.

Precedence when sources conflict: the decision ledger (docs/internal/decision-ledger.md, D-nn / FD-nn) > PRODUCT.md > planning docs > discovery docs > prototypes. The stack directive (D-01, stack locked to the repo-sync common stack) governs architecture and mechanism only; it does NOT inherit repo-sync's visual language (planning audit stream 3 item 6, docs/internal/planning-audit-2026-07-03.md: mechanism-vs-look). Look and feel are owned by PRODUCT.md and the design-system doc (D-05, two-mood design system; D-06, anti-reference). This split is load-bearing and recurs below wherever the reference architecture describes UI.

Contents:

1. Workspace layout and the core-purity rule
2. The pipeline and module ownership
3. IPC command surface (v1)
4. Data model
5. Tauri capability and security model (FD-29)
6. Windows filesystem reality (FD-19)
7. Error taxonomy (AppError)
8. Concurrency and the job model
9. Performance posture
10. Logging and observability
11. Deliberate non-adoptions
12. Items to verify at scaffold time

## 1. Workspace layout and the core-purity rule

The workspace mirrors repo-sync-tool (reference Section 4.3) with the same load-bearing invariant: the core crate never imports Tauri, and CI enforces it.

```
audiobook-organizer/
├─ Cargo.toml                  # [workspace] members = ["crates/abo-core", "src-tauri"]
├─ rust-toolchain.toml         # pinned toolchain
├─ .nvmrc                      # pinned Node
├─ crates/
│  └─ abo-core/                # NO tauri deps; pure engine
│     ├─ src/
│     │  ├─ lib.rs
│     │  ├─ error.rs           # AppError taxonomy (thiserror; serde + specta::Type)
│     │  ├─ ipc.rs             # IPC payload structs (ScanSummary, PlanPreview, ...)
│     │  ├─ scan/              # walker, WizTree CSV import, file typing
│     │  ├─ classify/          # FolderClass engine + health metrics
│     │  ├─ parse/             # pattern matchers, noise strippers, normalizers
│     │  ├─ plan/              # templates, builder, validate, provenance capture
│     │  ├─ exec/              # executor, journal, manifest, rollback, quarantine
│     │  ├─ dupes/             # candidate detection, optional hashing
│     │  ├─ cover/             # lofty subset: embedded art + cover.jpg read (F-907)
│     │  ├─ report/            # dry-run HTML report generator (F-506, F-507)
│     │  ├─ ruleset.rs         # ruleset model + JSON import/export
│     │  ├─ vfs.rs             # Vfs trait; RealFs + MemFs implementations
│     │  ├─ paths.rs           # platform path seam (%LOCALAPPDATA% vs ~/Library)
│     │  └─ db/                # sqlx pool, queries
│     └─ migrations/           # numbered .sql, applied via sqlx::migrate!
├─ src-tauri/                  # thin shell: commands, events, windows, capabilities
│  ├─ capabilities/            # Tauri v2 capability JSON (minimal, per-window)
│  └─ src/{main.rs, commands/, events.rs}
├─ src/                        # React + TS + shadcn/ui + Tailwind
│  ├─ lib/bindings.ts          # tauri-specta generated; never hand-edited
│  └─ assets/fonts/            # bundled Literata woff2 (FD-11)
└─ fixtures/                   # synthetic library generator + golden files
```

Core-purity rule (breakdown Section 4; reference Section 4.3 "dependency hygiene"): `abo-core` must not pull `tauri` even transitively. CI gate (release plan Section 6.1): `cargo tree -p abo-core -e normal | grep -qi "tauri"` must be empty, else the job fails. `specta` in core is fine (it is not a Tauri dependency); only `tauri-specta`, the glue, lives in the shell.

The `fixtures/` directory holds a synthetic library generator plus golden files. It is the highest-leverage early investment (release plan Section 6.4): programmatic trees fabricated into known states (staging, loose books, noisy names, box sets, bundles, duplicate groups, empty folders, over-length paths generated at runtime into the temp dir, never committed) make the whole pipeline testable deterministically without a real library, network, or UI. `.gitattributes` sets `* text=auto eol=lf` so goldens are byte-stable across machines (FD-25).

### Architecture invariants enforced in CI

These architecture rules are not honor-system; each has a mechanical gate (release plan Section 6.1, FD-24). An implementing agent that breaks one gets a red CI, not a review comment.

| Invariant | Gate | Where |
|---|---|---|
| Core never imports Tauri | `cargo tree -p abo-core` must not match `tauri` | lint job |
| IPC contract cannot drift | `pnpm bindings:check` regenerates `bindings.ts`, `git diff --exit-code` | lint job |
| No raw `invoke` in frontend | ESLint rule; only generated `commands.*` allowed | lint job |
| Zero network / no external hosts | grep the report template and app bundle for remote hosts (FD-11) | lint job |
| Plan is deterministic | byte-identical golden plan for a fixed snapshot + ruleset | test job |
| Rollback round-trips | apply full plan then roll back, byte-identical tree (RealFs temp dir, v0.5.0+) | test job |
| WCAG AA token contrast | mechanical contrast check of all token pairs in both themes (FD-21, from v0.4.0) | lint job |

CI concurrency uses cancel-in-progress; workflow `permissions:` are `contents: read`; per-push CI uses thin-LTO `[profile.release]` while release artifacts use full-LTO `[profile.dist]` (FD-24). Live workflow files land in the v0.1.0 spine, not in the docs-only branch, so a docs push never creates a red CI.

Inherited verbatim from the reference architecture (Sections 4.5, 4.10): sqlx with WAL and numbered migrations (schema resettable pre-v1, additive-only after v1); app data under `%LOCALAPPDATA%` (never Roaming, never a OneDrive-synced path); tauri-specta as the single source of IPC truth with all Tauri-family crate versions pinned exactly in `Cargo.toml`; rustls (never OpenSSL) if any HTTP client is ever added (not in v1); the corrupt-DB startup recovery behavior (Section 4 below).

### Deliberate differences from repo-sync-tool

Same architecture, different product shape (breakdown Section 4).

| Concern | repo-sync-tool | Audiobook Organizer | Rationale |
|---|---|---|---|
| Residency | Always-on tray app | Launch-when-needed campaign tool; no tray, no autostart | The tool runs a reorganization campaign, then closes. Nothing to watch between runs. |
| Concurrency hazard | Two git ops on one repo -> per-repo mutex | Two mutations in one tree -> single-writer rule: at most one apply job process-wide, enforced by a SQLite job lock plus an in-process mutex (Section 8) | One filesystem tree, not N independent repos. Any second writer risks the journal contract (D-09, safety invariants). |
| Long-running work | Short git calls on a schedule | Minutes-long scan/hash/apply jobs -> Tokio job model with progress events, cooperative cancel, pause/resume (Section 8) | Scale is 14k files; jobs are the unit of work, not the exception. |
| External binary | System git | None in v1 (`std::fs` only) | One less discovery/version problem; the app is self-contained. |
| Network | GitHub API (octocrab/rustls) | None in v1; fully offline (Section 5 zero-network invariant) | No online lookup, no telemetry, no font CDN. Privacy and determinism are product promises. |
| Visual language | Graphite design system (repo-sync DESIGN.md) | NOT inherited; owned by PRODUCT.md + design-system doc (D-06 anti-reference) | The stack directive covers architecture, not look (planning audit stream 3 item 6). |

## 2. The pipeline and module ownership

### System model

Audiobook Organizer is a single-process Tauri v2 desktop application (reference Section 4.1). One native host process (Rust) owns the Tokio async runtime (shared with Tauri, not a second runtime), the SQLite connection pool, and the window lifecycle. There is no tray, no autostart, no scheduler, and no external binary (Section 1 differences table). Three layers with one direction of dependency: the React frontend depends on the typed IPC contract; the `src-tauri` command/event layer depends on `abo-core`; `abo-core` depends only on the OS and crates, never on Tauri. That last constraint is what makes the core headlessly testable and what the core-purity gate enforces. All communication between the WebView and the host is over Tauri IPC (typed `commands` and `events`); there is no business logic in JavaScript and no direct filesystem access from the WebView (Section 5).

Everything the product does is one strict pipeline (breakdown Section 2). No stage may be skipped; the executor refuses plans that did not pass validation. There is one alternate entry point: WizTree CSV import (F-102) parses an existing WizTree export into the same snapshot schema as a live scan, flagged `source = csv`. This lets planning start from the existing 2026-03-25 snapshot and enables cheap what-if analysis without touching the drive. Downstream stages read the snapshot, never the live filesystem, so analysis is reproducible and diffable; a plan whose snapshot is stale relative to the live tree fails re-validation before apply (F-105, `snapshot-stale`).

```
scan -> classify -> parse -> plan -> validate -> preview/approve -> apply -> verify
 |                                       |               |
 +-- WizTree CSV import (alt entry)      +-- export      +-- journal -> rollback
```

| Stage | Input | Output | Owner module | Key feature IDs |
|---|---|---|---|---|
| Scan | Root path or WizTree CSV | Normalized tree snapshot in SQLite | `abo-core::scan` | F-101 (live tree scanner), F-102 (WizTree CSV import), F-103 (file typing) |
| Classify | Tree snapshot | `FolderClass` per folder + health metrics | `abo-core::classify` | F-201 (folder classification engine), F-202 (library health metrics) |
| Parse | Folder/file names | Extracted fields + confidence + noise annotations | `abo-core::parse` | F-301 (pattern matcher set), F-302 (noise strippers), F-303 (field extraction), F-304 (name normalizer) |
| Plan | Classifications + parses + ruleset | Ordered operation list + provenance | `abo-core::plan` | F-401 (naming templates), F-402 (structure policies), F-403 (plan builder), F-507 (pack provenance capture) |
| Validate | Plan | Pass/warn/block per operation | `abo-core::plan::validate` | F-404 (plan validation) |
| Preview/approve | Validated plan | Approval state; exported artifacts + report | GUI + `abo-core::plan` + `abo-core::report` | F-502 (campaign group review), F-505 (plan export), F-506 (dry-run HTML report) |
| Apply | Approved operations | Executed changes + journal + undo manifest | `abo-core::exec` | F-601 (executor), F-602 (journal + undo manifest), F-605 (quarantine) |
| Verify/rollback | Journal + manifest | Post-apply verification; full/partial undo | `abo-core::exec` | F-603 (rollback), F-604 (post-apply verification) |

Module-ownership notes that constrain the implementation:

- `abo-core::classify` assigns one of nine classes (F-201): `book`, `series-container`, `pack-container`, `staging`, `mixed`, `multi-book-suspect`, `empty`, `docs-resources`, `manual-review`. Rules evaluate bottom-up (children before parents) and every classification records why (rule id + evidence) so the UI can explain itself. `manual-review` is a first-class outcome, not an error; FD-17 routes video/course folders (the Zig Ziglar 52 Sales Lessons mp4 case), radio plays, and comics (cbr/cbz) here and never auto-plans them.
- `abo-core::parse` runs the nine discovery pattern matchers in specificity order (F-301), each a pure function returning fields plus a match score; ties surface as `ambiguous`. Noise strippers (F-302) are composable and individually toggleable, with `strip(strip(x)) == strip(x)` as a hard idempotence test. The folder-first default supersedes the discovery `preferSource=tags` default (FD-14); confidence in that assumption is tied to the v0.2.0 tag-quality probe.
- `abo-core::plan` groups operations by campaign group. FD-26 fixes seven user-facing groups (staging, loose books, messy names, box sets, bundles, copies, empty folders); series-index normalization folds into "messy names" for the UI while remaining a distinct internal plan pass. The review UI and the report agree on count and labels. The duplicates canonical unit is the GROUP - one book, N identical copies (FD-08); counts are groups, members are "copies".
- `abo-core::cover` is a read-only side-channel, not a pipeline stage (F-907, FD-03). It uses a bounded subset of the `lofty` crate to read embedded cover art and `cover.jpg` sidecars. It never writes tags, never writes files, and is invoked lazily via `cover_get` for the shelf. Covers render square 1:1; a miss yields the deterministic fallback tile (hash of title). This is the one place tag-crate code touches the library, and its read-only invariant is load-bearing: F-1101 (embedded tag reader) and F-1106 (tag writing) stay deferred (breakdown E-11).
- The same read-only `lofty` subset backs the v0.2.0 tag-quality probe (FD-14): read embedded tags on a bounded few-hundred real files, report field completeness, and record whether the folder-first assumption holds. The probe is a gate input, not a pipeline stage, and writes nothing.

### The Vfs seam and why dry-run is the same executor

The executor runs against a `Vfs` trait with two implementations (breakdown F-607 dry-run harness; D-09 safety invariants):

- `RealFs`: performs actual filesystem operations via `std::fs` with extended-length path semantics (Section 6).
- `MemFs`: an in-memory tree seeded from the snapshot; records the same operations without touching disk.

Dry-run (F-607) is not a separate code path. It is the identical executor (F-601) running the identical plan against `MemFs`, producing the identical journal shape. This is the mechanism behind D-09's "dry-run is the same executor" invariant and D-04's hard requirement: a fully functional dry run must exist and produce a browsable confirmation screen plus an exportable self-contained HTML report (F-506) before anything Real runs. Consequences:

- Executor logic is exhaustively unit-testable without disk (release plan Section 6.4: MemFs unit suites via the Vfs seam from v0.5.0).
- The GUI gets "simulate apply" for free.
- Rollback (F-603) is "just another plan" through the same validate/preview/apply pipeline: given a manifest, generate the inverse plan, validate it with F-404, preview it, apply it. Not a special path.

The `apply_start` command carries `mode: DryRun | Real`. `DryRun` binds `MemFs`; `Real` binds `RealFs`. A Real apply against the actual library is a human-only gate (D-10): agents never trigger it.

### Duplicates architecture (E-07)

Duplicate handling is a detection concern that feeds a normal campaign group; dedupe is "just another plan" through the standard plan/apply pipeline, and its canonical unit is the GROUP (FD-08). Two detection methods live in `abo-core::dupes`:

- Exact candidates (F-701): basename + size exact grouping across the snapshot. This is the method that found the 403 groups (~10.08 GB) in discovery; the exact GB figure stays "unknown until measured" until a fresh scan (planning audit stream 1 item 7, 2026-03-25 baseline labeling per FD-18).
- Version candidates (F-701): folder-level normalized-title matching, which catches root-vs-genre re-encodes that exact matching misses. These are labeled distinctly as version candidates and are never auto-resolved.

Hashing (F-702) is BLAKE3 over candidates only, opt-in, as a background job with progress - never hash-everything on scan (Section 11). A quarantine action on a duplicate group requires either verified hashes or an explicit user override (the dupes-override affordance is an explicit warning confirm, planning audit stream 2 item 15). Resolution policies (F-704): keep-larger / keep-higher-bitrate / keep-m4b / flag-only, default flag-only; a policy proposes a keeper, the user confirms, and losers quarantine via the normal executor (F-605). No duplicate is ever deleted (D-09).

## 3. IPC command surface (v1)

All commands are defined in Rust and exported to TypeScript via tauri-specta (breakdown Section 6; reference Section 4.4). Payload structs live in `abo-core::ipc`. tauri-specta is the single source of truth: renaming a Rust field or changing a return type breaks the TS build, which is the guardrail an agent build needs. Versions of `tauri-specta`/`specta` are pinned exactly and re-checked at each dependency review (reference Section 4.4 version-status caveat). The bindings-drift CI gate regenerates `bindings.ts` and fails on any diff (release plan Section 6.1). The frontend imports the generated typed `commands.*`/`events.*`; raw `invoke` is lint-forbidden.

The table below is the breakdown Section 6 surface EXTENDED with the features fixed by FD-01/02/03/05.

| Command | Signature (conceptual) | Feature |
|---|---|---|
| `scan_start` | `(root, ruleset_id) -> JobId` | F-101 (live tree scanner) |
| `scan_import_csv` | `(csv_path) -> JobId` | F-102 (WizTree CSV import) |
| `scan_list` / `scan_get` | `() -> Vec<ScanSummary>` / `(id) -> ScanDetail` | F-105 (snapshot persistence) |
| `classify_overview` | `(scan_id) -> HealthMetrics` | F-202 (library health metrics) |
| `folder_detail` | `(scan_id, path) -> FolderDetail` | Class, evidence, parsed fields (F-201/F-303) |
| `ruleset_list/get/save/delete` | CRUD over `Ruleset` | F-801 (ruleset model) |
| `plan_generate` | `(scan_id, ruleset_id) -> PlanId` | F-403 (plan builder); runs F-404 validation + F-507 provenance capture |
| `plan_get` | `(plan_id, filter) -> PlanPage` | Paged; powers preview (F-501/F-502) |
| `plan_set_group_approval` | `(plan_id, group, decision)` | F-502 (campaign group review) |
| `plan_exclude_op` | `(plan_id, op_id, excluded: bool)` | Per-op override (F-502) |
| `plan_export` | `(plan_id, format) -> PathBuf` | F-505 (plan export): CSV/JSON/Markdown |
| `report_generate` | `(plan_id) -> PathBuf` | F-506 (dry-run HTML report) + F-507 provenance report |
| `apply_start` | `(plan_id, mode: DryRun\|Real) -> JobId` | F-601/F-607; refuses if another apply job exists (single-writer, Section 8) |
| `job_status` / `job_cancel` | `(job_id) -> JobStatus` / `(job_id)` | F-104 (job progress + cancel); cooperative Stop |
| `job_pause` / `job_resume` | `(job_id)` / `(job_id)` | F-608 (pause and resume apply), FD-02; takes effect between operations only |
| `rollback_prepare` | `(manifest_id) -> PlanId` | F-603 (rollback) as a plan |
| `dupes_detect` / `dupes_hash_verify` | `(scan_id) -> JobId` | E-07 (duplicates): F-701/F-702 |
| `cover_get` | `(scan_id, entry_id) -> CoverResult` | F-907 (cover extraction and fallback tiles), FD-03; read-only lofty subset |
| `settings_get` / `settings_set` | Singleton settings | F-803 (app settings), incl. theme (FD-09) + retention (FD-20) |
| `library_root_pick` | `() -> Option<PathBuf>` | F-909 (first-run and library root selection), FD-05; opens tauri-plugin-dialog, backend persists + re-allows |
| `first_run_state` | `() -> FirstRunState` | F-909; reports whether a root/ruleset/theme are chosen so the shell can route onboarding |

Notes on the FD extensions:

- `cover_get` (FD-03): returns either the extracted image bytes (embedded art or a `cover.jpg` sidecar, read-only) or a `Fallback { title, tile_color }` payload. `tile_color` is a deterministic hash of the title so the fallback tile is stable across renders. Covers render square 1:1 (D-06). The frontend never reads the image file itself; the backend owns all filesystem access (FD-29).
- `library_root_pick` / `first_run_state` (FD-05): onboarding picks the library root via `tauri-plugin-dialog`. No library is assumed. Defaults: ruleset `abs-author-first` (D-02, author-first default layout), theme `day` (FD-09). The picked path is handed to the backend, persisted in `settings`, and re-allowed at startup (Section 5). Settings (F-803) hosts re-selection.
- Provenance in plan payloads (FD-01): `PlanPage` operation rows and `folder_detail` carry the source-pack membership captured at plan time (Section 4), so the review UI and the report can show award/pack provenance without a second query.

### Key payload struct shapes

Conceptual field lists for the load-bearing payloads, so the frozen seam is unambiguous before the GUI exists. All derive `serde` + `specta::Type` in `abo-core::ipc`.

- `HealthMetrics` (from `classify_overview`): per-class counts and byte totals, plus per-problem-type counts (loose-root books, noisy names, deep nesting, duplicate groups, empties). These are the library-home facts stated inside sentences, not a stat band (FD-07). Any figure derived from the 2026-03-25 baseline is labeled "2026-03-25 baseline, pending fresh scan" (FD-18).
- `PlanPage` (from `plan_get`): `{ plan_id, total_ops, groups: [{ group, op_count, byte_total, confidence_hist, warning_count }], ops: [PlanOp] }` where `PlanOp = { op_id, group, kind, source_path, target_path, rationale, matched_pattern, confidence, validation, approval, provenance }`. `matched_pattern` and `confidence` back the "Show file details" tier-1 content (FD-13, F-504).
- `FolderDetail` (from `folder_detail`): class, rule id, evidence, parsed fields with per-field confidence, and provenance. The tier-2 surface shows the plain-language summary; the technical truth sits behind one "Show file details" disclosure (D-03, audience tiers).
- `CoverResult` (from `cover_get`): `Image { bytes, mime }` or `Fallback { title, tile_color }` (FD-03).
- `JobStatus` (from `job_status`): `{ job_id, kind, state, done, total_estimate, current_location, error_code? }`. `state` is one of `queued|running|paused|done|failed|cancelled`; `current_location` is a friendly name, never a raw path on the scan line (FD-13).
- `FirstRunState` (from `first_run_state`): `{ has_root, has_ruleset, theme }` so the shell routes onboarding (FD-05).

### Events

`job:progress`, `job:completed`, `job:failed`, `job:paused`, `job:resumed`, `plan:invalidated` (snapshot drift detected), `apply:op-executed` (journal tail for the live apply view). `job:paused`/`job:resumed` are the FD-02 additions. The frontend listens and invalidates the corresponding TanStack Query keys; there is no polling (breakdown Section 6; reference Section 4.4).

### Frontend state architecture

The React layer (breakdown E-09) holds no business logic and no filesystem access. Server state (scans, health metrics, plans, jobs) is cached in TanStack Query keyed by the IPC command; events invalidate those keys, so the UI is push-driven, not poll-driven. Local UI state (selections, drawer open, filter chips) lives in Zustand. All backend calls go through the generated typed `commands.*`/`events.*` from tauri-specta; raw `invoke`/`listen` are lint-forbidden (the no-raw-invoke gate, Section 1). This keeps the frontend a pure function of the frozen seam: it can be built against stubs before the backend exists and never blocks on backend work, and vice versa.

## 4. Data model

SQLite via sqlx with WAL (breakdown Section 7; reference Section 4.5). Draft DDL shape; the real migration freezes at v0.1.0, additive-only after v1. DB location: `%LOCALAPPDATA%\AudiobookOrganizer\abo.db` (Local, never Roaming, never OneDrive-synced). The table below is the breakdown Section 7 model EXTENDED per FD-01, FD-09, FD-20.

| Table | Role | Key fields |
|---|---|---|
| `scans` | One row per snapshot | `id`, `source (live\|csv)`, `root_path`, `started_at`, `completed_at`, `entry_count`, `total_bytes`, `status` |
| `entries` | One row per file/folder | `scan_id`, `id`, `parent_id`, `path`, `name`, `kind`, `file_class`, `size`, `mtime`, `depth`; indexed `(scan_id, parent_id)`, `(scan_id, path)` |
| `classifications` | One row per folder | `scan_id`, `entry_id`, `class`, `rule_id`, `evidence_json`, `parsed_fields_json`, `confidence` |
| `rulesets` | Named rule bundles | `id`, `name`, `body_json`, `schema_version`, timestamps |
| `plans` | Immutable plan headers | `id`, `scan_id`, `ruleset_id`, `created_at`, `status`, `stats_json` |
| `plan_ops` | One row per operation | `plan_id`, `op_id`, `group`, `kind`, `source_path`, `target_path`, `rationale`, `confidence`, `validation (valid\|warning\|blocked)`, `approval`, **`provenance_json`** |
| `jobs` | Scan/hash/apply/rollback jobs | `id`, `kind`, `state (queued\|running\|paused\|done\|failed\|cancelled)`, `progress`, `started_at`, `finished_at`, `error_code` |
| `journal` | Append-only apply journal | `job_id`, `seq`, `op_id`, `phase (intent\|done\|failed)`, `at`, `detail_json` (incl. provenance) |
| `manifests` | Completed apply jobs, exportable | `id`, `job_id`, `plan_id`, `json_path`, `reversible` |
| `duplicate_groups` / `duplicate_members` | Dedupe candidates | group key, method, hash state, resolution |
| `activity_records` | App-level audit trail | `action`, `params_json`, `outcome`, timestamps |
| `settings` | Singleton (`CHECK (id = 1)`) | roots, quarantine path, reports path, **`theme`**, **`snapshot_retention_n`**, log retention |

DDL sketch for the two extended tables (illustrative; the frozen migration lives under `crates/abo-core/migrations/`):

```sql
CREATE TABLE plan_ops (
  plan_id        TEXT    NOT NULL,
  op_id          TEXT    NOT NULL,
  "group"        TEXT    NOT NULL,   -- one of the seven campaign groups (FD-26)
  kind           TEXT    NOT NULL,   -- move|rename|mkdir|rmdir-empty|quarantine|no-op
  source_path    TEXT,
  target_path    TEXT,
  rationale      TEXT    NOT NULL,   -- human sentence + rule id
  confidence     TEXT    NOT NULL,   -- high|medium|low
  validation     TEXT    NOT NULL,   -- valid|warning|blocked
  approval       TEXT    NOT NULL DEFAULT 'pending',
  provenance_json TEXT,              -- FD-01: source-pack membership, nullable
  PRIMARY KEY (plan_id, op_id)
);

CREATE TABLE settings (
  id                   INTEGER PRIMARY KEY CHECK (id = 1),
  library_root         TEXT,
  quarantine_root      TEXT,
  reports_root         TEXT,
  theme                TEXT NOT NULL DEFAULT 'day',   -- day|evening (FD-09)
  snapshot_retention_n INTEGER NOT NULL DEFAULT 10,   -- FD-20
  log_retention_d      INTEGER NOT NULL DEFAULT 90
);
```

Extensions in detail:

- Provenance (FD-01, D-14): `plan_ops.provenance_json` records source-pack membership per book (Hugo, Nebula, Top 100, Dune Universe, and similar) captured at plan/flatten time. Shape: `{ packs: [{ name, kind: "award"|"collection"|"bundle", evidence }] }`. The provenance travels into `journal.detail_json` and the exported `manifests` (v0.5.0), so it survives apply. The provenance report exports beside the plan at v0.3.0 and is re-emitted post-apply at v0.5.0. Pack shells after a successful extraction go to quarantine by default, with a policy toggle to leave-in-place (F-402 structure policies).
- Set-aside folder naming (FD-31): the physical folder that holds set-aside items (emptied pack shells, non-preferred-format copies, clutter) is named `Set Aside` on disk (the plain-language rule extends to disk artifacts the family can see). "quarantine" stays internal-only vocabulary: the `settings.quarantine_root` column, the `quarantine` op kind, and internal type/module names keep it, but it never appears in an on-disk name, a stored rationale sentence, or report copy.
- Theme (FD-09): `settings.theme` stores `day` or `evening` (the canonical `data-theme` values). Default `day`, persisted via F-803. UI labels are Day / Evening.
- Snapshot retention (FD-20): `settings.snapshot_retention_n` (default 10) bounds DB growth by keeping the last N scans. A retention sweep drops older `scans` and their `entries`/`classifications`. 20k entries is comfortably in range; no server DB ever.

### Ruleset model

Rulesets are to this product what update policies are to repo-sync: the persisted user intent (breakdown E-08). A `rulesets` row carries a JSON body validated against a versioned schema (`schema_version`), so `abo-core` and any future CLI share one definition (F-801). A ruleset bundles naming templates (F-401), structure policies (F-402), and cleanup toggles. Shipped presets: `abs-author-first` (the default per D-02: `{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/`), `title-first`, and `hybrid-genre`. Genre and awards become tags/collections, never folders (D-02); missing-field fallbacks are explicit per template (omit the ` ({Year})` segment rather than emit `()`). Regenerating a plan after a ruleset tweak creates a new immutable plan (F-405), never mutating an existing one; approval state lives beside the plan. Structure-policy defaults with safety implications (one-book-per-folder, preferred format = m4b with the loser quarantined, non-audio clutter policy, empty-folder removal) are ruleset toggles; the hard safety invariants (never delete audio, journal-before-act, single-writer) are NOT toggles - they are architecture (D-09, Section 8).

### Corrupt-DB startup recovery, WAL, migration policy

Inherited from the reference architecture (Section 4.10.b migration-failure recovery): migrations run at launch via `sqlx::migrate!` (embedded at compile time). On migration or corruption error, do NOT silently delete data and do NOT crash to a blank window. Instead: (1) log the failure via `tauri-plugin-log`; (2) move the existing DB aside to `AudiobookOrganizer\corrupt-backups\abo-<timestamp>.db`; (3) create a fresh DB so the app is usable; (4) surface a one-time family-safe notice that prior state was preserved as a backup, mapping to the `db-corrupt-recovered` error family (Section 7, FD-04 error, empty, and loading states). WAL mode plus the existing indexes and the retention policy above are the SQLite scale posture (FD-20). Pre-v1 the schema may be reset freely; post-v1 additive-only (new nullable columns with defaults, new tables, never destructive renames or drops).

## 5. Tauri capability and security model (FD-29)

This section resolves planning audit stream 3 item 4 (Tauri capability/security model omitted) and is mandatory from v0.1.0. It inherits the reference architecture Section 4.9 capability model and applies it to a filesystem tool.

- Minimal default capabilities from day one. The WebView gets `event:default` and `core:webview:default` and NO filesystem and NO shell capability by default. Capabilities are additive and scoped per window; the main window and any future window carry only what they need.
- Folder access via `tauri-plugin-dialog` only (F-909, FD-05). "Pick library root" / re-selection open the OS folder picker; the chosen path is handed to the backend, which persists it in `settings`. There is no blanket FS grant to the WebView.
- Backend re-allows persisted roots at startup. On launch the backend re-establishes access to the persisted library root, quarantine root, and reports folder. The WebView is never granted a standing filesystem scope.
- Frontend-never-touches-fs invariant. All filesystem reads and mutations (scan, classify, plan, apply, cover extraction, report writing) run inside `abo-core` behind typed IPC. The React layer holds no path logic and performs no file I/O. This is the same discipline as the reference architecture's "backend owns path resolution" rule (Section 4.5): never re-resolve a path from the JS side.
- No shell exposure. There is no `tauri-plugin-shell` allowlist in v1 (unlike repo-sync, which shells to git and to the editor/terminal). The one place a raw path is shown to the user is the ABS setup path on the Done next-step card (FD-13); that is display-only text, not a shell action.
- CSP posture. A strict Content-Security-Policy is set in `tauri.conf.json`: `default-src 'self'`, no remote origins, no inline remote script. Combined with the zero-network invariant below, the WebView cannot reach any external host.

Illustrative main-window capability (under `src-tauri/capabilities/`), showing the minimal grant plus the dialog plugin and nothing else:

```json
{
  "identifier": "main-capability",
  "windows": ["main"],
  "permissions": [
    "core:event:default",
    "core:webview:default",
    "dialog:allow-open"
  ]
}
```

There is deliberately no `fs:*` and no `shell:*` permission. Folder selection uses `dialog:allow-open` only; the returned path is consumed by the backend, which owns all subsequent filesystem access.

### Zero-network invariant and font bundling (FD-11)

The app makes zero network requests, in the app and in the exported report. This resolves planning audit stream 2 item 10 (the prototypes' Google Fonts `<link>` violates zero-network):

- Literata is bundled in-app as self-hosted woff2 under `src/assets/fonts/` (SIL OFL). No `<link>` to Google Fonts. The prototypes' Google Fonts tag is a prototype-only artifact and never ships.
- The exported HTML report (F-506) embeds a subsetted Literata as a `data:` URI with a system-serif fallback stack, so the report is a single self-contained file with no external assets (FD-28).
- CI grep gate: a check greps the report template and the app bundle for external hosts (`http://`, `https://`, `//fonts.`, `<link` to remote) and fails the build on any hit. This is the enforcement, not just a promise.

## 6. Windows filesystem reality (FD-19)

The scanner and executor treat Windows path semantics as first-class, not as an edge case. This section is normative for `abo-core::scan`, `abo-core::exec`, and `abo-core::parse::normalizer`.

- Extended-length paths. Open and manipulate paths with extended-length (`\\?\`) semantics so the pipeline works past the legacy 260-char `MAX_PATH` limit. Full-path length is computed with the `\\?\` allowance in F-404 (plan validation).
- LongPathsEnabled detection. Detect the `LongPathsEnabled` registry setting. When it is 0 and a target path exceeds 260 chars, warn with a linked how-to (`path-too-long` family, Section 7). Keep a softer near-260 warning even when long paths are enabled, for interop with third-party tools that lack long-path support.
- Reserved names and trailing dots (F-304 name normalizer recap). The normalizer strips characters illegal on Windows (`<>:"/\|?*`), forbids reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`), forbids trailing dots and trailing spaces (Win32 strips them silently), enforces a max component length (default 120, word-boundary truncation), and applies Unicode NFC normalization so visually identical names compare equal. Validation (F-404) is the backstop for anything that slips through: `illegal-component`, `reserved-name`.
- NTFS case-insensitive collision policy. All collision checks - within the plan and against on-disk paths - compare case-insensitively (F-404). The executor re-checks source-exists / target-does-not-exist case-insensitively before each operation (F-601 TOCTOU backstop).
- Junctions and reparse points. The scanner records junctions and reparse points but never follows them (F-101), so a junction loop terminates the scan cleanly rather than recursing forever. Recorded as `junction-skipped(path)`.
- Hardlink note. The executor does not create or rely on hardlinks. Same-volume moves use `rename` (metadata-only, atomic per entry); no hardlink dedupe is attempted (Section 11 non-adoptions).
- Defender / Controlled Folder Access posture. Mass renames can trip Windows Defender Controlled Folder Access. Three mitigations: (1) a pre-campaign check step in the M-1 (campaign) runbook; (2) executor semantics of retry-once-then-halt-group on access-denied (do not thrash; stop the group and surface a decision); (3) an error-taxonomy entry `access-denied-av` (Section 7). This resolves planning audit stream 2 item 10's neighbor concerns around real-library apply safety.
- OneDrive placeholder hazard. Arbitrary user-chosen roots may live under a OneDrive-synced folder where files are cloud placeholders (not fully local). Detect a resolved root under a known OneDrive prefix and warn; the app data DB itself is always in `%LOCALAPPDATA%` (never synced), which neutralizes the WAL-sidecar corruption path (reference Section 4.10.c).

Detection-and-response matrix (each hazard maps to a detection point, a runtime response, and where it surfaces):

| Hazard | Detected at | Response | Surface / error code |
|---|---|---|---|
| Path > 260 chars, LongPaths off | plan validation (F-404) | Warn with linked how-to; still plan with `\\?\` | `path-too-long` |
| Reserved / illegal / trailing-dot name | normalize (F-304), validated (F-404) | Rewrite in normalizer; validation is backstop | `illegal-component`, `reserved-name` |
| NTFS case collision | plan validation + pre-op re-check | Block the op; require ruleset fix or exclude | `collision-in-plan`, `collision-on-disk` |
| Junction / reparse loop | scan (F-101) | Record, do not follow; scan terminates | `junction-skipped(path)` |
| Controlled Folder Access denies rename | apply (F-601) | Retry once, then halt the group | `access-denied-av` |
| Root under OneDrive prefix | root selection (F-909) | Warn; DB stays in %LOCALAPPDATA% | family-safe notice (FD-04) |
| Cross-volume move, insufficient free space | plan validation (F-404) | Block; summed byte estimate vs free space | `cross-volume-space-insufficient` |

## 7. Error taxonomy (AppError)

`thiserror` enum in `abo-core::error`, serialized across IPC with stable machine codes plus remediation strings (breakdown Section 8). Every code maps to exactly one family-safe remediation sentence; the UI never shows a raw OS error without its code and remediation. FD-04 (F-908 error, empty, and loading states) ties each family to a concrete UI surface, so this taxonomy and the design-system surfaces are one-to-one. Families below are the breakdown set EXTENDED per FD-19 (access-denied-av), FD-02 (pause states), FD-03 (cover failures).

- Scan: `root-not-found`, `root-not-directory`, `permission-denied(path)`, `junction-skipped(path)`, `csv-parse(row)`.
- Plan: `snapshot-stale`, `collision-in-plan`, `collision-on-disk`, `path-too-long`, `illegal-component`, `reserved-name`, `cross-volume-space-insufficient`, `cycle-detected`, `nothing-approved`.
- Apply: `source-vanished`, `target-appeared`, `rename-failed(os-code)`, `copy-verify-mismatch`, `journal-write-failed` (hard stop), `job-already-running`, `cancelled`, `resume-required`, **`access-denied-av`** (FD-19: retry-once-then-halt-group exhausted; remediation names Controlled Folder Access and links the how-to), **`paused`** / **`resume-blocked`** (FD-02: pause state surfaced, resume refused because the snapshot drifted).
- Cover (FD-03, non-fatal): **`cover-extract-failed`**, **`cover-absent`**. Neither blocks any pipeline stage; both resolve to the deterministic fallback tile (Section 3 `cover_get`). The shelf degrades gracefully rather than erroring.
- Rollback: `manifest-missing`, `manifest-not-reversible`, `inverse-collision`.
- Storage: `db-migration-failed`, `db-corrupt-recovered`.

FD-04 surface mapping (authored in full in the design-system and v0.4.0 spec): blocked campaign group state, scan/apply failure, snapshot-stale re-validation prompt, corrupt-DB recovery notice, permission-denied - each a family above. Empty/edge states (already-tidy library, empty library root, all-groups-excluded, no duplicates) and loading states (building-the-plan between scan and review, re-scan progress) are not errors; they are first-class surfaces the specs author, not gaps inherited from the happy-path prototypes.

## 8. Concurrency and the job model

### Single-writer rule (D-09)

At most one apply job runs process-wide, ever. Enforced by two mechanisms that compose:

1. A SQLite job lock: an apply job claims a lock row; `apply_start` refuses with `job-already-running` if one is held. The lock persists, so a crashed apply is visible on restart (F-606 interruption safety).
2. An in-process async mutex held for the duration of the apply job, so two in-process callers cannot race even before the DB row is written.

This is the deliberate difference from repo-sync's per-repo mutex (Section 1 table): one tree, one writer. Scan, classify, and dupe-hash jobs are read-only against the live tree and may run without the apply lock, but only one apply job blocks the world.

A Real apply is additionally gated outside the software by the M-1 (campaign) backup decision (D-17). The pre-campaign backup posture is user-defined: the product and the M-1 runbook present the options (external-drive copy, same-drive copy, manifests-plus-quarantine only) with trade-offs, and the user chooses at campaign time. The M-1 gate stays open until a choice is recorded; nothing Real runs without a recorded backup decision. This gate is a human-only stop (D-10), enforced by process and runbook, not by the executor code, but the architecture makes it safe to hold: because dry-run is the same executor against `MemFs` (Section 2), a full, faithful preview is always available while the backup decision is pending.

### Tokio job model, progress, cancellation, pause

All long operations (scan, hash, apply, rollback) run as jobs on the shared Tokio runtime (breakdown F-104; reference Section 4.1 "one runtime, shared with Tauri"). Each job:

- Emits `job:progress` events (items done, total estimate, current friendly location - not a raw path on the scan line, FD-13). No UI freeze; all work is off the UI thread.
- Persists a `jobs` row so a crashed job is visible on restart.
- Honors a cooperative cancellation token at safe boundaries only - never mid-file-move (F-104). The Stop control on both scan and tidy progress screens is this cancel (FD-02). Cancellation leaves at most one operation in doubt, auto-reconciled on next start (F-606).

Pause and resume (F-608, FD-02): `job_pause` / `job_resume` take effect between operations only. Pausing does not touch the journal; the job simply stops advancing after the current operation completes and its `done` row is flushed. `job:paused` / `job:resumed` events fire. Resume re-validates snapshot freshness first; if the tree drifted, resume is refused (`resume-blocked`) and the plan must be re-validated. The prototypes' "Pause between books" control maps to this feature; "Skip ahead" in the prototypes is demo-only and never ships.

### Executor mechanics: rename-first (D-08)

The executor is rename-first (D-08, corrected rationale). Same-volume operations use `rename` as the primary mechanism: metadata-only, atomic per entry, no data copied. This is mandatory at this library's scale (297 GB), where the feasible full copy is better spent as a pre-campaign backup (D-17) than as the apply mechanism. Cross-volume operations - the only case that cannot rename - use copy + size/hash verify + delete-source, and are explicitly marked `copy+verify+delete` in the plan with a summed byte estimate checked against free space at validation time (F-404, `cross-volume-space-insufficient`).

Per-operation discipline before each op (F-601): re-check source exists and target does not (TOCTOU backstop, NTFS case-insensitive, Section 6); one operation at a time within an apply job (sequential by design - interleaving buys little on one spindle and complicates the journal); never-overwrite (a target that appeared is `target-appeared`, not a clobber). Operation ordering respects dependencies: `mkdir` before any move into it, moves out of a folder before its `rmdir-empty`. `rmdir-empty` only ever targets a verified-empty directory - this is the mechanism behind the FD-10 guarantee that only empty folders are removed and no audio is deleted. Quarantine (F-605) moves losers to `E:\Books - Audio\Quarantine\<job-id>\...` preserving the original relative path so provenance is self-evident; nothing is auto-deleted, and the user empties quarantine manually.

### Journal-before-act and interruption safety

The executor writes an `intent` journal row and flushes it BEFORE each operation, then writes `done` (or `failed`) after (F-602). `journal-write-failed` is a hard stop: if the journal cannot be written, the operation does not proceed. On startup, a journal with `intent` rows lacking `done` means exactly one operation is in doubt (F-606); the executor verifies the actual on-disk outcome (a rename either happened or did not; copy phases are distinguishable by a target-size check) and marks the journal accordingly, then offers resume or abort-with-rollback.

## 9. Performance posture

| NFR | Target | Source |
|---|---|---|
| Scale | Full pipeline comfortable at 20,000 files / 1,000 folders | breakdown Section 9 |
| Scan speed | Scan of the real library (2026-03-25 baseline: 13,970 files, 718 folders) < 60 s on the local drive | breakdown Section 9 |
| Responsiveness | No UI freeze during jobs; all long work on Tokio, event-driven progress | breakdown Section 9 |
| Determinism | Same snapshot + ruleset = byte-identical plan (golden-tested) | breakdown Section 9; release plan Section 6.4 |
| Recoverability | Kill -9 during apply leaves at most one operation in doubt, auto-reconciled on restart | breakdown Section 9; F-606 |
| Footprint | Bundle < 30 MB (evergreen WebView2 `downloadBootstrapper`) | reference Section 4.10.a |

Per-stage posture: scan (F-101) is I/O-bound and streams entries into SQLite in batched transactions; it is the stage governed by the < 60 s target. Classify and parse (F-201/F-301) run over the in-DB snapshot, not the live tree, so they are CPU-bound and reproducible; the pattern matchers and strippers are pure functions, which keeps them table-testable and cheap. Plan building (F-403) is deterministic and single-pass over the snapshot. Apply (F-601) is sequential by design (one op at a time within a job); at library scale the dominant cost is metadata renames (near-instant), so wall-clock apply time is bounded by op count, not byte volume, because same-volume renames copy no data (D-08).

Virtualization: the F-501 (everything view) later surface - the virtualized full change list, grouped, P1/v0.6.0 (FD-06, D-16) - uses list virtualization (TanStack Virtual) because the library has 718+ folders and the change list can be large. It is a static, no-row-cap surface, so virtualization is a rendering concern only; the data is already fully computed in `plan_ops`. The P0 review surface is per-group cards plus the full HTML report (D-16), which do not need virtualization.

SQLite scale and retention (FD-20): WAL mode, the existing `(scan_id, parent_id)` / `(scan_id, path)` indexes, and the snapshot retention setting (default keep last 10 scans) bound DB growth. 20k entries is comfortably within SQLite's range; there is no server database, ever.

## 10. Logging and observability

- Structured logging via `tracing` in `abo-core` plus `tauri-plugin-log` in the shell (breakdown F-1003). Log files rotate; log retention is a `settings` field.
- Reports folder (F-1002): all exports - plans (F-505), the dry-run HTML report and provenance report (F-506/F-507), undo manifests as JSON (F-602), verification reports (F-604), duplicate reports (F-703) - land as files in a `Reports/` folder beside the app data, plus anywhere the user picks. Manifests exporting as JSON means recovery never depends on the app's own database being healthy.

### Dry-run HTML report generator (F-506, FD-28)

The report is a P0 product in its own right (D-04, D-16): the review artifact for the early mini-campaign, generated before any GUI exists. Architecture:

- Generated by `abo-core::report` from a validated plan, with the template baked into the crate (no network assets, no build-time fetch). It ships in v0.3.0, before the GUI. `report_generate(plan_id)` writes a single self-contained `.html` file.
- Self-contained: subsetted Literata embedded as a `data:` URI with a system-serif fallback stack (FD-11); all CSS inline; no external hosts. The CI grep gate covers this template.
- Format (FD-28 completes the missing sections): a per-group summary table; before/after example tables per group; warnings needing a decision; the full change-list table (columns: group, book/title, from, to, note; grouped by campaign group; no row cap, since it is a static file); and the FD-10 guarantee block in canon copy ("No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.").
- Distinct theme by design: the report uses a single light "paper" theme with a serif register, deliberately NOT the app's Day/Evening themes (FD-28). It has its own print rules. This is an intentional divergence, not drift: the report is a document, the app is a tool.
- Plain-language register throughout (D-03, FD-23): a non-engineer household reader (tier 2) must be able to read it. Counts use the duplicates group unit (FD-08); any GB figure states which quantity it refers to.
- Activity log (F-1001): an append-only `activity_records` row for every scan/plan/apply/rollback with its parameters and outcome. This is the app-level audit trail, distinct from the per-operation `journal`.
- No telemetry, no crash reporting, no network, ever (breakdown Section 10; reference Section 4.9). Made visible in the UI footer, matching the OSS posture. This is a product promise, not a default that can be toggled on.

## 11. Deliberate non-adoptions

Each with a one-line why, so an implementing agent does not "helpfully" add it back.

- No auto-update in v1 (FD-22). Fully offline posture; users manually download new installers. Revisit post-1.0. The v0.9.0 installer is unsigned (private/family distribution; the install doc explains the SmartScreen "More info, then Run anyway" flow); code signing (Azure Trusted Signing) is decided with the public flip at v0.9.0+ (D-13).
- English-only v1 with centralized strings (FD-23). All user-facing copy lives in one strings module so later localization is possible; the plain-language copy register is part of the design-system doc. No i18n framework in v1.
- WebKitGTK smoke job: deliberate non-adoption (FD-24). Recorded as a conscious choice, not an oversight; this is a quiet campaign UI with no tray popup or vibrancy geometry to validate cross-engine, unlike repo-sync where the tray popup drove that job. Revisit only if GUI divergence appears. (The reference architecture pins a WebKitGTK smoke target in its Section 2 mitigations; we consciously drop it.)
- No shell / process exposure to the WebView (Section 5). There is no external binary in v1 (Section 1 differences table); nothing to spawn.
- No hash-everything on scan. BLAKE3 hashing runs over duplicate candidates only, opt-in, as a background job (F-702); hashing the whole 297 GB library on every scan is the discovery docs' explicit anti-pattern.
- No hardlink dedupe. Duplicates are set aside via quarantine through the normal plan/apply pipeline (F-605/F-703), never collapsed with hardlinks; hardlinks would violate the "one book, one place, fully undoable" model and confuse ABS.
- No delete of audio anywhere (D-09). Quarantine-only. Only verified-empty folders are removed (`rmdir-empty`), and every change is undoable. Canon guarantee copy (FD-10): "No audiobook is ever deleted. Only empty folders are removed, and every change can be undone."
- No online metadata lookup, no tag writing, no ABS-side changes in v1 (breakdown Section 10; D-14, FD-12). Provenance is captured as durable data plus an exported report (F-507); pushing collections to ABS stays deferred (F-1102, v1.1+). No v1 copy may promise ABS-side changes or tag writes.

## 12. Items to verify at scaffold time

A short list of decisions this document commits to in principle but that need a concrete check when the v0.1.0 spine is built, following the reference architecture's verify-before-pinning posture (reference Section 4.4).

- tauri-specta / specta versions. Confirm the current published versions at scaffold time, pin them exactly in `Cargo.toml`, and re-check at each dependency review (reference Section 4.4 version-status caveat). If the release candidate proves unstable, the fallback is a single hand-maintained `bindings.ts` plus a round-trip CI test - the contingency, not the plan.
- Bindings-drift gate platform placement (FD-24). Decide whether the drift gate runs on the Ubuntu lint runner or must move to the Windows runner. If the specta export links Tauri (so generating bindings needs the Tauri build), the gate belongs on Windows. Verify during v0.1.0 and document whichever option is chosen; both are acceptable, but the workflow must not silently generate stale bindings.
- WebView2 runtime strategy. Ship the evergreen `downloadBootstrapper` to keep the bundle under 30 MB (reference Section 4.10.a). Confirm the first-run experience on a machine lacking the runtime surfaces a clear message rather than a silent failure.
- OSS-landscape timeboxed check (FD-15). Before the build starts, spend one bounded hour surveying existing tools (beets audiobook plugins, Audiobookshelf community organizers, renamers) and record the outcome in the roadmap and EXECUTION.md. The build does not start without this recorded; it may confirm the build, narrow it, or surface a reusable component, but the decision must be explicit.
- Migration freeze point. The schema is resettable until v0.1.0 and additive-only after. Confirm the `plan_ops.provenance_json`, `settings.theme`, and `settings.snapshot_retention_n` columns (Section 4) are present in the first frozen migration, since adding them later is possible but cheaper to do now while resets are free.

Each item is a known-unknown with a default already chosen; the verification confirms the default holds on the real toolchain rather than reopening the decision.

---

Status and scope of this document: this is the architecture contract, not an acceptance record. Acceptance criteria live in the per-release specs under `docs/internal/releases/<version>-<codename>/` (FD-16, effort = release); the release plan and roadmap aggregate and reference those, and this document is the map they execute against (release plan Section 8). Where a release spec and this document diverge in detail, the spec governs the build of its release and this document should be updated (or the relevant part superseded) rather than left to drift. All decisions cited here trace to the decision ledger (docs/internal/decision-ledger.md, D-nn / FD-nn); nothing in this document invents scope beyond it.
