---
title: Audiobook Organizer - Product Requirements Document
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (PRD)
sources:
  - PRODUCT.md
  - _local/planning/audiobook-organizer-strategy-brief_2026-07-02.md
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - _local/planning/release-plan-and-ci_2026-07-02.md
  - _local/prior-work/folder-structure.md
  - _local/initial-discovery/audio-books-audiobookshelf_codex.md
  - E:/Projects/product-on-purpose/repo-sync-tool/docs/internal/v1-architecture-and-decisions.md
supersedes: _local/planning/feature-function-breakdown_2026-07-02.md (as feature registry of record)
extends: PRODUCT.md (design contract for look and tone)
---

# Audiobook Organizer - Product Requirements Document

## 0. Status and relationship to other documents

This PRD is the authoritative statement of what the product is, who it serves, what it must do, and how success is measured. It formalizes the three planning drafts in `_local/planning/` (strategy brief, feature-function breakdown, release plan and CI) into one tracked requirements document.

- **PRODUCT.md** (repo root) stays the design contract. It owns look, tone, brand personality, design principles, and accessibility posture. Where copy or visual language is at issue, PRODUCT.md governs.
- **This PRD** owns product requirements, scope, the feature registry, decisions, non-functional requirements, non-goals, and success criteria. From this document forward, the PRD is the feature registry of record: the `_local` feature-function breakdown is superseded by the inventory in Section 5.
- **Specs** (per-release folders under `docs/internal/releases/<version>-<codename>/`) own acceptance criteria. This PRD references features and their target releases; it never authors AC (a standing rule of the suite).
- Precedence when sources conflict: the decision ledger (docs/internal/decision-ledger.md, D-nn/FD-nn) > PRODUCT.md > planning docs > discovery docs > prototypes.

The stack directive (D-01, stack locked) covers architecture, CI shape, and engineering conventions inherited from repo-sync-tool. It does NOT cover visual language: the look and feel is set by PRODUCT.md and the design-system doc, and the repo-sync design tokens are inherited as a mechanism only, not as an inherited aesthetic (resolves planning audit stream 3 item 6, docs/internal/planning-audit-2026-07-03.md; F-501 token-discipline conflict).

## 1. Problem statement and job to be done

jp has a 297 GB audiobook library (2026-03-25 baseline: about 13,970 files across 718 folders at `E:\Books - Audio`) that he wants to serve cleanly through Audiobookshelf (ABS). The library's physical structure fights ABS at every level: at least nine coexisting naming conventions, roughly 200 folder names polluted with ripper tags, 238 loose `.m4b` files in the root (about 68 GB), award-collection mega-folders nesting books 3 to 5 levels deep, staging folders (`_sort`, `_process`, about 21.6 GB) inside the scanned root, confirmed duplicates, and 20 empty folder skeletons.

**The job to be done.** Functionally: convert an accumulated, multi-convention audiobook tree into an ABS-native structure without losing a byte or spending a weekend doing it by hand. Emotionally: press Apply on 297 GB of hard-to-reacquire files without fear.

**The fear component is the product.** The tool moves files that are annoying to reacquire, so the catastrophic risk is data loss, not a missed feature. Preview, the dry-run HTML report, the append-only journal, and full undo ARE the product, not features bolted onto it. Every destructive-adjacent flow runs scan, then review, then confirm. The review screen and the exportable report are the trust ceremony (PRODUCT.md design principle 2). This framing is load-bearing and must not be weakened downstream.

The tool complements ABS and never replaces it (D-03 audience, PRODUCT.md purpose). It is an analyzer and planner first, a mover second (Codex discovery consensus, `_local/initial-discovery/audio-books-audiobookshelf_codex.md`).

## 2. Users and context of use

Three tiers, all real (D-03 audience: all three tiers). The design bar is set by tier 2.

| Tier | Who | Needs |
|---|---|---|
| 1 | jp: a technical product person on Windows 11 who runs the campaigns | Full technical truth on demand: paths, matched pattern, confidence, operation detail, behind explicit disclosure |
| 2 | Household members (non-engineers) who may run a tidy-up or review one | Must never be shown a file path, exit code, or jargon term as the primary interface. This tier sets the UI bar. |
| 3 | Eventual public open-source users with messy audiobook libraries and ABS | Same plain-language surface as tier 2; a clean install on a fresh machine |

**Tier-2 family bar (verbatim intent, PRODUCT.md).** If a non-technical member of the household could not confidently review and confirm a tidy-up, the surface is wrong. Technical truth (paths, operation detail) stays available behind one consistent "Show file details" disclosure for tier 1 (extended in FD-13 to show matched pattern and confidence, F-504 content).

**Context of use (verbatim intent, PRODUCT.md).** At a desk or in an evening chair, occasionally (campaigns and intake batches, not daily), with real anxiety about a tool that moves hundreds of gigabytes of personally collected audiobooks.

## 3. Product shape: the pipeline

Everything the product does is one strict pipeline. No stage may be skipped, and the executor refuses plans that did not pass validation.

```
scan -> classify -> parse -> plan -> validate -> preview/approve -> apply -> verify
  |                                                   |               |
  +-- WizTree CSV import (alternate entry)            +-- export      +-- journal -> rollback
```

| Stage | Input | Output | Owner module |
|---|---|---|---|
| Scan | Root path or WizTree CSV | Normalized tree snapshot in SQLite | `abo-core::scan` |
| Classify | Tree snapshot | A `FolderClass` per folder plus library health facts | `abo-core::classify` |
| Parse | Folder/file names | Extracted fields (title, author, series, index, year, narrator) with confidence and noise annotations | `abo-core::parse` |
| Plan | Classifications, parses, ruleset | Ordered list of proposed changes (move, rename, create-folder, set-aside, no-op) | `abo-core::plan` |
| Validate | Plan | Pass/fail per change: collisions, path safety, cycle detection, disk space | `abo-core::plan::validate` |
| Preview/approve | Validated plan | User approval state per group; exported plan artifacts and HTML report | GUI + `abo-core::plan` |
| Apply | Approved changes | Executed filesystem changes plus append-only journal and undo manifest | `abo-core::exec` |
| Verify/rollback | Journal + manifest | Post-apply verification report; full or partial undo | `abo-core::exec` |

### 3.1 The seven user-facing campaign groups (FD-26)

A campaign group is the unit of user approval. The UI, the review surface, and the report all present exactly seven groups and agree on their count and labels (FD-26 campaign group canon, resolves planning audit stream 2 item 13). The plan builder (F-403) keeps eight internal plan passes; series-index normalization folds into "messy names" for the UI while remaining a distinct internal pass.

| User-facing group | What it does | Internal plan pass(es) (F-403) |
|---|---|---|
| Staging | Move intake/processing areas out of the scanned library | `staging-separation` |
| Loose books | Give each loose root audiobook its own folder | `loose-root-books` |
| Messy names | Strip ripper tags and normalize names; fix series numbering | `strip-noise`, `normalize-series` |
| Box sets | Split folders that hold several complete books | `split-multi-book` |
| Bundles | Extract books from award/source packs into canonical folders | `flatten-packs` |
| Copies | Set aside duplicate copies of a book | `dedupe-quarantine` |
| Empty folders | Remove verified-empty folders | `empty-cleanup` |

Duplicates canonical unit (FD-08): the GROUP is one book with N identical copies. Nav badges, headlines, and the report all count groups; member files are "copies". Any GB figure states which quantity it refers to.

## 4. Domain model (glossary)

| Term | Definition |
|---|---|
| Library root | The canonical folder ABS scans. The tool treats it as the only live tree. |
| Book folder | A folder containing exactly one book's audio, optionally with sidecars (cover, ebook, description). The canonical unit. |
| Series container | A folder whose children are book folders of one series. |
| Pack / source-pack container | An acquisition bundle (Hugo, Nebula, Top 100, Dune Universe). Provenance, not canonical structure. |
| Mixed folder | A folder containing both direct audio files and child folders. Structurally risky for ABS. |
| Box set / multi-book folder | One folder holding several complete books as sibling files. ABS collapses these into one wrong item. |
| Loose book | An audio file sitting directly in a container (acceptable only in the library root, per ABS docs). |
| Staging | Intake/processing areas (`_sort`, `_process`, future `Intake/`). Never inside the scanned root. |
| Noise | Ripper tags, bitrate/size markers, rank prefixes, release-group suffixes, underscores-as-spaces. |
| Plan | An immutable, versioned set of proposed changes from one scan plus one ruleset. |
| Journal | Append-only record written during apply, one entry per executed change, flushed before the next change. |
| Undo manifest | The completed journal in reverse-executable form; the rollback contract. |
| Set aside (quarantine) | Move-not-delete holding area outside the library root. Nothing in v1 deletes audio. "Set aside" is the user-facing term; "quarantine" is internal only (a standing rule of the suite). |
| Ruleset | A named, persisted bundle of naming templates, structural policies, and cleanup toggles. |

## 5. Feature inventory (registry of record)

Priority: P0 = v1 must-have; P1 = v1 should-have (cut via descope triggers); P2 = v1.x fast-follow; P3 = v2+. Release column references the ladder in the release plan and roadmap. New features introduced by this suite (F-507, F-608, F-907, F-908, F-909) and the FD-06/FD-07 redefinitions are marked. Every ID carries its handle on first use.

### E-01 (scan and ingest)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-101 (live tree scanner) | Recursive walk of a chosen root | P0 | v0.1.0 | Extended-length (`\\?\`) paths (FD-19); records junctions, never follows |
| F-102 (WizTree CSV import) | Alternate snapshot source | P1 | v0.2.0 | Same schema as F-101, flagged `source=csv` |
| F-103 (file typing) | Extension-based class per file | P0 | v0.1.0 | Adds a `video` class (mp4 video, cbr/cbz comics) per FD-17 |
| F-104 (job progress + cancel) | Progress events and cooperative cancellation | P0 | v0.2.0 | Real Stop control on scan and tidy (FD-02); safe boundaries only |
| F-105 (snapshot persistence) | Immutable snapshots with metadata | P0 | v0.1.0 | Stale-snapshot plans fail re-validation before apply |

### E-02 (classification)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-201 (folder classification engine) | A `FolderClass` per folder | P0 | v0.2.0 | Classes per Codex taxonomy; video/course folders route to `manual-review` (FD-17) |
| F-202 (library health facts) | Aggregate problem counts and sizes | P0 | v0.2.0 | Health facts stated inside sentences, no stat bands (FD-07) |
| F-203 (box-set detection) | Flag folders holding several complete books | P0 | v0.2.0 | Feeds split proposals |
| F-204 (disc-structure detection) | Recognize disc-based books, nonconforming disc names | P1 | v0.3.0 | Propose ABS-conformant `Disc NN` |
| F-205 (parallel-format detection) | Flag books with both mp3 chapters and an m4b sibling | P1 | v0.3.0 | Feeds preferred-format policy (F-402) |

Classification is the product; the rename engine is commodity (Codex discovery). `manual-review` is a first-class outcome, not an error.

### E-03 (parsing and normalization)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-301 (pattern matcher set) | The 9 discovery patterns as ordered, testable matchers | P0 | v0.2.0 | Pure functions, table-driven-tested against real examples |
| F-302 (noise strippers) | Ripper tags, bitrate/size markers, rank/year prefixes, suffixes, underscores | P0 | v0.2.0 | Idempotent: `strip(strip(x)) == strip(x)` |
| F-303 (field extraction with confidence) | title/author/series/index/year/narrator plus per-field confidence | P0 | v0.2.0 | Low-confidence fields surface for review |
| F-304 (name normalizer) | Whitespace, punctuation, casing, illegal-character and reserved-name policy | P0 | v0.2.0 | Windows reserved names, trailing dots/spaces, NFC (FD-19) |

### E-04 (planning)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-401 (naming templates) | Configurable target patterns for standalone/series/disc books | P0 | v0.3.0 | Default preset `abs-author-first` (D-02 author-first default) |
| F-402 (structure policies) | One-book-per-folder, pack handling, sidecar, preferred format, non-audio | P0 | v0.3.0 | Pack shell after extraction goes to set-aside by default (FD-01) |
| F-403 (plan builder) | Generate the change list from snapshot plus ruleset | P0 | v0.3.0 | Emits the seven user-facing groups (FD-26); deterministic (golden test) |
| F-404 (plan validation) | Collisions, path safety, cycles, disk space, staleness | P0 | v0.3.0 | Case-insensitive NTFS collision checks (FD-19) |
| F-405 (plan persistence and versioning) | Immutable plans with approval state | P0 | v0.3.0 | Regenerating after a ruleset tweak creates a new plan |

### E-05 (preview and approval)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-501 (everything view) REDEFINED | Virtualized full change list, grouped; tree presentation optional | P1 | v0.6.0 | REDEFINED by D-16 (review surface) + FD-06 (F-501 redefined): the prior P0 tree-diff definition is superseded; this is a tier-1 disclosure surface |
| F-502 (campaign group review) | Approve/reject/defer per group; per-change override | P0 | v0.4.0 | Per-change exclude lives inside the group detail (planning audit stream 2 item 15) |
| F-503 (search and filter in preview) | Find any book/path/rule across the plan | P1 | v0.4.0 | Simple filter box (planning audit stream 2 item 15) |
| F-504 (explainability) | Every change shows rationale, matched pattern, and confidence | P0 | v0.4.0 | The content behind "Show file details" for tier 1 (FD-13) |
| F-505 (plan export) | CSV, JSON, and Markdown plan artifacts | P1 | v0.3.0 | Lands in the reports folder plus a user-picked location |
| F-506 (dry-run HTML report) | Self-contained, non-engineer-readable HTML report of a validated plan | P0 | v0.3.0 | The trust artifact for the early mini-campaign (D-04 dry run before execute); format canon in FD-28; reference `_local/gui/06-dryrun-report.html` |
| F-507 (pack provenance capture and report) NEW | Record source-pack and award membership per book; export a provenance report | P0 | v0.3.0, v0.5.0 | NEW per D-14 (provenance in v1) + FD-01 (F-507 provenance). v0.3.0: plan builder records membership in `plan_ops`, report exports beside the plan. v0.5.0: journal and manifest carry provenance, report re-emitted post-apply |

D-04 hard requirement: a fully functional dry run producing a browsable confirmation screen AND an exportable self-contained HTML report (F-506) exists before anything executes.

### E-06 (execution and safety)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-601 (executor) | Apply approved changes in dependency order | P0 | v0.5.0 | Rename-first same-volume; copy+verify+delete only cross-volume (D-08 rename-first executor) |
| F-602 (journal + undo manifest) | Append-only journal; reversible manifest per apply job | P0 | v0.5.0 | Journal-before-act; JSON manifest export so recovery survives a sick DB |
| F-603 (rollback) | Full or partial undo from a manifest | P0 | v0.5.0 | Rollback is just another plan through the same pipeline (D-09 safety invariants) |
| F-604 (post-apply verification) | Verify moved trees; refresh snapshot; report | P0 | v0.5.0 | Discrepancies block further groups until acknowledged |
| F-605 (set-aside / quarantine) | Move-not-delete holding area with provenance | P0 | v0.5.0 | Outside the library root; preserves relative path; user empties it manually |
| F-606 (interruption safety + resume) | Crash/cancel mid-apply leaves a resumable, known state | P0 | v0.6.0 | At most one change in doubt, auto-reconciled on restart |
| F-607 (dry-run harness) | Execute a plan against a virtual filesystem only | P0 | v0.5.0 | `Vfs` seam (MemFs/RealFs); dry run is the same executor against memory (D-09) |
| F-608 (pause and resume apply) NEW | Pause an apply job between changes; resume | P1 | v0.5.0 | NEW per FD-02. `job_pause`/`job_resume` IPC; pause takes effect between changes only; journal unaffected. This is the prototype's "Pause between books". "Skip ahead" in prototypes is demo-only and never ships |

Design invariants (D-09 safety invariants): never overwrite; never delete audio; single-writer rule (one apply job process-wide); journal-before-act.

### E-07 (duplicates)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-701 (duplicate candidate detection) | Name+size exact grouping across the snapshot | P0 | v0.3.0 | Counts GROUPS (FD-08); version candidates labeled distinctly, never auto-resolved |
| F-702 (hash verification) | Opt-in content hash before any set-aside action | P1 | v0.6.0 | BLAKE3 over candidates only; never hash-everything |
| F-703 (duplicate review + report) | Grouped review UI plus CSV export | P1 | v0.6.0 | Dedupe is just another campaign group |
| F-704 (resolution policies) | keep-larger / keep-higher-bitrate / keep-m4b / flag-only | P1 | v0.6.0 | Default policy `flag-only` |

### E-08 (rulesets and settings)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-801 (ruleset model + persistence) | Named rulesets bundling templates, policies, toggles | P0 | v0.3.0 | Rows with a JSON body against a versioned schema |
| F-802 (ruleset import/export) | Portable JSON ruleset files | P1 | v0.6.0 | |
| F-803 (app settings) | Library roots, set-aside root, reports folder, theme, snapshot retention | P0 | v0.4.0 | Hosts theme (default `day`, FD-09), re-selection of library root (FD-05), snapshot retention (default 10, FD-20) |

### E-09 (GUI surfaces)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-901 (app shell + navigation) | Sidebar nav, theme, command palette | P0 | v0.4.0 | Day/Evening themes (FD-09) |
| F-902 (library home) RENAMED | Warm cover-forward shelf; health facts inside sentences; exactly one primary action | P0 | v0.4.0 | RENAMED from "dashboard" per FD-07. No stat bands, no hero metrics (D-06 anti-reference). The word "dashboard" appears nowhere user-facing |
| F-903 (plan preview surface) | Hosts F-502/F-503/F-504 (and F-501 everything view from v0.6.0) | P0 | v0.4.0 | |
| F-904 (apply + activity surface) | Live job progress, journal view, verification, rollback entry, Stop/Pause | P0 | v0.5.0 | Deliberately boring and explicit; one job at a time |
| F-905 (duplicates surface) | Hosts F-703 review | P1 | v0.6.0 | |
| F-906 (settings + ruleset editor) | Ruleset CRUD with live re-plan preview counts | P0 | v0.4.0 | |
| F-907 (cover extraction and fallback tiles) NEW | Read embedded art and cover.jpg sidecars, read-only; square 1:1; designed no-cover fallback | P0 | v0.4.0 | NEW per D-15 (covers in v0.4.0) + FD-03 (F-907 covers). `lofty` subset, read-only. Covers render square 1:1, never 2:3, no fake spine shading on covers (D-06). Fallback tile = title text on a deterministic hash-colored tile |
| F-908 (error, empty, and loading states) NEW | Family-safe surface for every error family; empty and loading states | P0 | v0.4.0 | NEW per FD-04. Blocked group, scan/apply failure, snapshot-stale re-validation, corrupt-DB notice, permission-denied; already-tidy, empty root, all-excluded, no-duplicates; building-the-plan and re-scan loading states |
| F-909 (first-run and library root selection) NEW | Onboarding: pick library root via dialog; default ruleset and theme | P0 | v0.4.0 | NEW per FD-05. `tauri-plugin-dialog`, no library assumed; default `abs-author-first`, default theme `day`; re-selection in Settings (F-803); frontend never touches the filesystem (FD-29) |

### E-10 (observability)

| ID | Feature | Pri | Rel | Notes |
|---|---|---|---|---|
| F-1001 (activity log) | Append-only record of every scan/plan/apply/rollback | P0 | v0.2.0 | |
| F-1002 (reports folder) | All exports as files | P0 | v0.3.0 | Plans, manifests, verification, provenance, duplicate reports |
| F-1003 (structured app logging) | tauri-plugin-log plus tracing in core; log rotation | P0 | v0.1.0 | No telemetry, no crash reporting, no network |

### E-11 (v2 candidates, explicitly deferred)

| ID | Feature | Pri | Notes |
|---|---|---|---|
| F-1101 (embedded tag reader) | P2 | Full `lofty` tag reconciliation; first deferred item to revisit. The cover subset ships early as F-907 |
| F-1102 (ABS API integration) | P2 | Trigger ABS rescan; later push award/pack provenance as ABS collections (D-14 defers the push) |
| F-1103 (online metadata lookup) | P3 | Both discovery docs say defer |
| F-1104 (cover art management) | P3 | ABS handles covers natively |
| F-1105 (intake mode) | P2 | Watch/classify/file new acquisitions; needs the product-posture decision first (open item, Section 11) |
| F-1106 (tag writing) | P3 | Explicitly out per discovery; structure first |
| F-1107 (multi-library / media generalization) | P3 | Keep the engine generic where free; do not design for it |

## 6. Decisions

### 6.1 Ratified decision ledger (D-nn), by jp

| ID (handle) | Date | One-line rationale |
|---|---|---|
| D-01 (stack locked) | 2026-07-02 | Tauri v2, Rust, React, TS, shadcn/ui, SQLite (sqlx), tauri-specta: one architecture and CI shape across projects |
| D-02 (author-first default) | 2026-07-03 | Default layout `abs-author-first`; genre and awards become tags/collections, never folders |
| D-03 (all three tiers) | 2026-07-03 | jp, household non-engineers, eventual public; tier 2 sets the UI bar |
| D-04 (dry run before execute) | 2026-07-03 | Early mini-campaign; a working dry run plus exportable HTML report (F-506) exists before anything runs |
| D-05 (two moods, one system) | 2026-07-03 | Warm evening library and calm daytime utility shipped as themes, not separate designs |
| D-06 (anti-reference) | 2026-07-03 | Reject the AI-dashboard look; covers square 1:1, no fake spine shading; series spine clusters keep their metaphor |
| D-07 (engine-first order) | 2026-07-02 | abo-core hardens on fixtures before any GUI; GUI renders a frozen tauri-specta contract |
| D-08 (rename-first executor) | 2026-07-02 | Same-volume rename primary; copy+verify+delete only cross-volume; the full copy is better spent as a backup |
| D-09 (safety invariants) | 2026-07-02 | Quarantine-only, journal-before-act, single-writer, Vfs seam, rollback-as-a-plan, never-overwrite |
| D-10 (scope of go) | 2026-07-03 | On approval, execute the full ladder; hard stops only at human-only gates |
| D-11 (remote and governance) | 2026-07-03 | Existing private repo; trunk-based, short-lived branches, PRs; CI substitutes for code review while private |
| D-12 (docs tracked) | 2026-07-03 | `docs/internal/` is tracked in git; only `_local/`, `.memsearch/`, tool caches are gitignored |
| D-13 (OSS posture) | 2026-07-03 | Private now; license and public flip decided at v0.9.0 (human-only); docs written public-ready |
| D-14 (provenance in v1) | 2026-07-03 | Pack/award provenance captured as durable data plus report at plan/flatten time; ABS-side push deferred |
| D-15 (covers in v0.4.0) | 2026-07-03 | Cover extraction P0 in v0.4.0 (embedded art + cover.jpg, read-only) with a designed no-cover fallback |
| D-16 (review surface) | 2026-07-03 | Per-group cards plus the full HTML report is the P0 product; the everything view is P1, later |
| D-17 (backup is user-defined) | 2026-07-03 | The product and M-1 runbook present backup options; the user chooses at campaign time; the gate stays open until recorded |

### 6.2 Fable-fixed content decisions (FD-nn) and where they land

Resolutions of audit findings, fixed by the orchestrator on 2026-07-03. Full text in docs/internal/decision-ledger.md.

| ID | Summary | Where it lands |
|---|---|---|
| FD-01 (F-507 provenance) | New F-507; pack shells to set-aside by default | This PRD Section 5 (E-05), architecture schema, v0.3.0 and v0.5.0 specs, report spec |
| FD-02 (F-608 pause + Stop) | New F-608; real Stop on progress; no "Skip ahead" | This PRD (E-06, F-104), v0.4.0 and v0.5.0 specs |
| FD-03 (F-907 covers) | New F-907; square 1:1; hash-colored fallback tile | This PRD (E-09), design-system, v0.4.0 spec |
| FD-04 (F-908 states) | New F-908; error/empty/loading surfaces | This PRD (E-09), design-system, v0.4.0 spec |
| FD-05 (F-909 first-run) | New F-909; root selection, defaults | This PRD (E-09), v0.4.0 spec |
| FD-06 (F-501 redefined) | Everything view, P1, v0.6.0 | This PRD (E-05), roadmap, v0.4.0 and v0.6.0 specs |
| FD-07 (F-902 library home) | Rename from dashboard; no stat bands | This PRD (E-09), design-system, v0.4.0 spec |
| FD-08 (duplicates unit) | The GROUP is canonical; copies are members | This PRD Section 3.1, design-system, v0.6.0 spec, report spec |
| FD-09 (theme ids) | `day`/`evening` canonical; default day; error token pair | Design-system, F-803 |
| FD-10 (deletion guarantee copy) | Canon reassurance sentence | Design-system, report spec, v0.4.0 spec |
| FD-11 (fonts) | Literata bundled/subsetted; zero network; CI grep gate | Architecture, ci-plan, design-system, report spec |
| FD-12 (no tag-write promises) | Remove "genre view lives on as tags" | This PRD Section 8, design-system copy, v0.4.0 spec |
| FD-13 (raw paths) | Only the ABS setup path on Done; everything else disclosed | This PRD Section 2, design-system, v0.4.0 spec |
| FD-14 (tag-quality probe) | v0.2.0 gate probe; folder-first supersedes preferSource=tags | This PRD Sections 9 and 10, v0.2.0 spec |
| FD-15 (OSS-landscape check) | 1-hour pre-v0.1.0 check | Roadmap, EXECUTION.md |
| FD-16 (effort = release) | One tracked release folder each; epics stay taxonomy | Roadmap, release folders |
| FD-17 (video/course routing) | `video` file class; course/radio folders to manual-review | This PRD (F-103, F-201), Section 8, v0.2.0 spec |
| FD-18 (baseline labeling) | Codex ABS-item baselines; label "2026-03-25 baseline" | This PRD Section 9, v0.2.0 spec |
| FD-19 (Windows reality) | Long paths, Defender, NTFS case, junctions, OneDrive | This PRD Section 7, architecture, M-1 runbook |
| FD-20 (SQLite scale) | WAL, indexes, snapshot retention (default 10) | This PRD Section 7, architecture, F-803 |
| FD-21 (a11y verification) | Mechanical contrast check, axe-core smoke, keyboard walk | This PRD Section 7, design-system, ci-plan |
| FD-22 (distribution) | Unsigned through v0.9.0; no auto-update in v1 | This PRD Section 7, roadmap, install doc |
| FD-23 (localization) | English-only v1; centralized strings module | This PRD Section 7, design-system |
| FD-24 (CI fixes) | Concurrency, permissions, LTO profiles, bindings-drift platform | ci-plan, v0.1.0 spec |
| FD-25 (hygiene set) | .gitattributes, bump-version, CHANGELOG, draft LICENSE/templates | v0.1.0 spec, hygiene batch |
| FD-26 (campaign group canon) | Seven user-facing groups | This PRD Section 3.1, v0.3.0 spec, design-system |
| FD-27 (sample-data rule) | Demo numbers are sample; targets derive from baselines | This PRD Section 9, all specs |
| FD-28 (report format spec) | Full change-list table, paper theme, print rules, guarantee block | F-506 report spec |
| FD-29 (Tauri security model) | Minimal capabilities; no fs/shell to WebView; typed IPC only | Architecture, v0.1.0 spec |
| FD-30 (model-tiering policy) | Fable plans; Opus safety-critical; Sonnet mechanical | EXECUTION.md |

## 7. Non-functional requirements

| NFR | Target |
|---|---|
| Safety | No change ever overwrites or deletes audio; set-aside is the only removal; journal precedes every mutation (D-09) |
| Scale | Full pipeline comfortable at 20,000 files / 1,000 folders; UI virtualized; real-library scan under 60 s on the local drive |
| Responsiveness | No UI freeze during jobs (all long work on the Tokio runtime, event-driven progress) |
| Determinism | Same snapshot plus ruleset yields a byte-identical plan (golden-tested) |
| Recoverability | Kill during apply leaves at most one change in doubt, auto-reconciled on restart |
| Footprint | Bundle under 30 MB (evergreen WebView2 download bootstrapper) |
| Privacy | No network, no telemetry; everything local and inspectable, made visible in the UI footer |
| Platform | Windows 11 is the GA bar (human-validated); macOS is compiles-and-bundles-in-CI honesty only |
| Windows reality (FD-19) | Extended-length (`\\?\`) path semantics in scanner and executor; detect `LongPathsEnabled=0` and warn with a how-to when targets exceed 260 chars; keep near-260 warnings for interop; Defender/Controlled Folder Access pre-campaign check plus retry-once-then-halt-group on access-denied; case-insensitive NTFS collision checks everywhere; junctions/reparse points recorded, never followed; OneDrive placeholder hazard documented for arbitrary roots |
| Storage scale (FD-20) | SQLite with WAL and the existing indexes; snapshot retention policy (keep last N scans, default 10, setting in F-803) to bound DB growth; 20k entries is comfortably in range; no server DB ever |
| Accessibility verification (FD-21) | WCAG AA is verified, not just promised: mechanical contrast check of all token pairs in both themes (script, CI from v0.4.0); axe-core smoke in Vitest on primary surfaces; keyboard-walkthrough item in the per-release manual QA checklist. Tertiary text (`--ink-3`) is restricted to decorative content or darkened/lightened to pass 4.5:1 where it conveys information |
| Distribution (FD-22) | Unsigned installer through v0.9.0 (private/family distribution; the install doc explains the SmartScreen "More info, then Run anyway" flow); code signing decided with the public flip at v0.9.0+ (D-13); no auto-update in v1 (fully offline); manual download of new installers |
| Localization (FD-23) | English-only v1; all user-facing copy centralized in one strings module so later localization is possible; the copy register (plain-language vocabulary) is part of the design-system doc |

## 8. Non-goals (v1)

Not a media player, not an ABS replacement, not a metadata editor, not a downloader. No online lookup, no tag writing, no deletion of anything, no cloud sync, no multi-user, no AI-driven renaming without deterministic rules (discovery consensus, PRODUCT.md).

Specific non-goal clarifications from the audit:

- **No ABS-side changes or tag writes (FD-12).** The Done screen's "the old genre view lives on as tags" claim is removed: it promised tag writing, a non-goal. Replacement copy: genre folders are not carried into the new layout; pack and award membership is preserved in the provenance report (F-507 pack provenance). No v1 copy may promise ABS-side changes or tag writes.
- **No auto-planning of video/course content (FD-17).** Folders dominated by video or course content (the `Zig Ziglar - 52 Sales Lessons` case: 52 mp4 files) route to `manual-review` and are never auto-planned. Radio plays and similar non-book audio route to `manual-review` when detected. The `video` file class (mp4 video, cbr/cbz comics) makes this detectable.

## 9. Success criteria

### 9.1 The three "solved" outcomes (strategy brief)

1. **A clean canonical library.** ABS scans a `Library` root containing only one-folder-per-book structures it interprets correctly: authors, series, sequence numbers, and years land in the right ABS fields; no ripper tags in titles; no staging or pack content in the scan.
2. **A trustworthy, repeatable tool.** Any future reorganization (new intake batch, policy change, second library) runs through the same pipeline with a manifest that can undo it. The tool prevents re-messification rather than performing a one-time miracle.
3. **A second proof of the common stack.** The Tauri/Rust/React/SQLite architecture from repo-sync-tool is validated on a second, different product shape (batch file operations rather than a resident tray app).

### 9.2 Measurable campaign targets

All library figures are the 2026-03-25 baseline, pending fresh scan (FD-18 labeling rule). Demo numbers in the prototypes are sample data and are never hardcoded as targets (FD-27); real targets derive from the fresh-scan baselines below.

| Target | 2026-03-25 baseline | Success state |
|---|---|---|
| Loose root books folderized | 238 loose files, ~68 GB (237 parse cleanly as `Title by Author`) | Near zero loose books in the library root |
| Ripper-tag / noise names cleaned | 203 bracket tags, 170 bitrate, 214 size, 143 rank prefixes, 116 year prefixes | Noise names near zero after the messy-names group |
| Staging out of the scanned root | `_sort` + `_process`, ~21.6 GB | No staging content under the scanned library |
| Box sets split | Harry Potter (11 files/7 titles), Narnia (7), Wings of Fire Main Series (13) | One folder per book |
| Empty folders removed | 20 empty skeletons | Zero verified-empty folders |
| ABS import correctness | Structural taxonomy: ~582 book-like folders, ~11 mixed folders, ~831 estimated ABS items | ABS displays authors/series/sequence correctly on sampled shelves |
| Duplicate copies set aside | Exact basename+size: 403 groups (~10.08 GB), unknown until re-measured | Duplicate groups reviewed, keepers confirmed, copies set aside |

### 9.3 Tag-quality probe (FD-14)

The v0.2.0 gate includes a tag-quality probe: read embedded tags on a few hundred real files (bounded `lofty` subset, read-only), report field completeness, and record whether the folder-first assumption holds. This converts the strategy brief's inferred (not measured) claim about embedded metadata into evidence, and sets the confidence for the folder-first default.

## 10. Intentional cuts and supersessions

- **Count reconciliation (planning audit stream 1 item 7).** The library figures differ across sources because they were measured differently. The Codex snapshot counts 719 folder rows, 13,544 audio files, and 14,689 total rows; the working baseline cites about 13,970 files and 718 folders. These are the same 2026-03-25 WizTree export counted with different row filters (audio-only vs all files; whether the root row is counted as a folder). The single source of truth is the fresh scan at v0.2.0; all cited numbers are labeled "2026-03-25 baseline, pending fresh scan" (FD-18) until then. Duplicate reclaim is stated as a range and treated as unknown until measured: the confirmed-pairs method gave about 3 GB, the exact basename+size method gave about 10.08 GB (403 groups).
- **Dropped discovery settings (planning audit stream 1 item 8).** The discovery settings catalog (cover.jpg standardization, `maxBatchSize`, separate intake/process/archive roots, `yearPosition`, archived pack-shell destination) is intentionally not carried as user-facing toggles in v1. Rationale: safety settings are invariants (Section 7), not toggles; `yearPosition` stays implicit in the naming presets (F-401); the pack-shell destination is fixed to set-aside by default with a leave-in-place policy toggle (FD-01); separate intake/process/archive roots belong to the deferred intake mode (F-1105). Cover.jpg is read-only in v1 (F-907), never written.
- **preferSource supersession (planning audit stream 1 item 9, FD-14).** The discovery draft defaulted `preferSource=tags`. The v1 default is folder-first: folder names are more reliable than embedded tags across this library (Codex analysis). This PRD states the supersession explicitly; confidence in it is tied to the tag-quality probe (Section 9.3). jp's own historical naming preference (`_local/prior-work/folder-structure.md`) is title-first (`Title - Author (Year)`); D-02 sets the shipped default to author-first (`abs-author-first`), with title-first available as a preset, so the historical preference is preserved as a choice rather than the default.
- **"Fix this folder" deferral (planning audit stream 1 item 10).** The discovery "fix this one folder" focused workflow is not in v1. The seven campaign groups (Section 3.1) supersede per-folder flow for the campaign. Per-folder is a candidate for v1.x alongside intake mode (F-1105 intake mode).

## 11. Open items (human-only)

These remain jp's decisions and are not resolved by this suite (D-10 hard stops are human-only gates):

- **License and public flip (D-13, at v0.9.0).** Private now. The license choice and the public flip are decided at v0.9.0. Docs and hygiene are written public-ready; LICENSE and CONTRIBUTING land as drafts marked pending until then.
- **Backup choice at campaign time (D-17).** The product and the M-1 runbook present the options (external-drive copy, same-drive copy, manifests-plus-set-aside only) with trade-offs; jp chooses at campaign time. The M-1 gate stays open until a choice is recorded. Nothing Real runs without a recorded backup decision.
- **Intake mode posture (F-1105, strategy brief open question 4).** Whether the watch/classify/file intake workflow belongs on the roadmap, or campaign-mode is the permanent shape, is an unresolved product decision. F-1105 (intake mode) stays P2 and enters only through a spec after jp decides.
