---
id: v0.3.0
title: "Release v0.3.0 (planning) - plans, validation, exports"
codename: planning
date: 2026-07-03
status: review
owner: jprisant
tier: engine-only (no GUI); release is the effort unit (FD-16)
depends_on: v0.2.0-understanding (scan, classify, parse hardened on the fixture library)
produced-by: author agent (release spec)
sources:
  - _local/planning/release-plan-and-ci_2026-07-02.md (Section 4 v0.3.0)
  - _local/planning/feature-function-breakdown_2026-07-02.md (E-04, E-05, E-07, E-08, E-10)
  - _local/planning/audiobook-organizer-strategy-brief_2026-07-02.md (2026-07-03 amendment)
  - PRODUCT.md (design contract, safety principles)
  - _local/gui/06-dryrun-report.html (report format reference)
  - docs/internal/decision-ledger.md (D-01..D-17, FD-01..FD-30)
  - docs/internal/planning-audit-2026-07-03.md (stream 1 item 1; stream 2 items 7, 8, 10, 13, 20; stream 3 item 4)
decisions: [D-02, D-04, D-08, D-09, D-12, D-14, FD-01, FD-06, FD-08, FD-10, FD-11, FD-12, FD-16, FD-17, FD-18, FD-19, FD-20, FD-26, FD-27, FD-28, FD-30]
---

# Spec: Release v0.3.0 (planning)

## Task Summary

- Status: review (pending jp approval of the planning suite).
- Theme: the tool can say exactly what it would do, provably safely, and hand that to a human as files.
- Features in scope: F-401, F-402, F-403, F-404, F-405, F-505, F-506, F-507, F-701, F-801, F-1002, F-204, F-205 (13).
- Release gate: 8 composite checks (see Release Gate). Signature gate: deterministic validated plan over the real snapshot plus a self-contained HTML dry-run report a non-engineer can read.
- Open questions: 0 blocking. See Open Questions.

## Purpose

After v0.2.0 (understanding) the engine can look at a library and tell the truth about it: a snapshot, a `FolderClass` per folder, and parsed fields with confidence. v0.3.0 (planning) turns that understanding into an explicit, validated, immutable list of proposed changes - the plan - and emits it as human-readable files: CSV, JSON, Markdown, and a self-contained HTML dry-run report (F-506, P0 per D-04, the dry-run-first requirement). No filesystem is mutated in this release; the executor lands in v0.5.0 (acting).

This is the first release that is useful with no GUI. The gate produces a genuinely valuable artifact even if the project stopped here: a reviewed, exported reorganization plan plus a shareable HTML report. The early mini-campaign (M-1 groups (a)-(b)) runs on exactly this output: dry run, read the report, confirm, apply later with v0.5.x.

## Context

Consumes from v0.2.0 (understanding): immutable scan snapshots (`scans` + `entries`), classifications (`FolderClass` + evidence + parsed fields + confidence per folder), and the fixture generator. Everything in v0.3.0 reads the snapshot, never the live filesystem, so plans are reproducible and diffable. Produces for v0.4.0 (seeing): a frozen plan-and-validation data model that the GUI renders without adding engine capability, plus the report template the GUI can export.

Model-tiering (FD-30): the executor is not in this release, but the plan builder, validation, and provenance capture are correctness-critical authorship. See implementation-plan.md for per-task Opus/Sonnet assignment.

## Scope

Every feature below is in scope for v0.3.0. Each subsection gives what it is, key behaviors, edge cases, and per-feature acceptance criteria (testable checkboxes). Acceptance criteria live here; the roadmap and release plan only reference them.

### F-401 (naming templates)

What it is: configurable target path patterns for standalone, series, and disc books, driven by template variables.

Key behaviors: variables `{Author}`, `{Title}`, `{Series}`, `{SeriesIndex}` (width-configurable 1/01/001), `{Year}`, `{Narrator}`, `{Subtitle}`. Three shipped presets: `abs-author-first` (`{Author}/{Series}/Book {SeriesIndex} - {Year} - {Title}/`), `title-first` (`{Title} - {Author} ({Year})/`), `hybrid-genre` (genre kept as a top-level shelf, ABS-native below). Default preset is `abs-author-first` per D-02 (author-first default). Genre and awards are never folders under the default: they become tags/collections/provenance (D-02, F-507).

Edge cases: missing-field fallbacks are explicit per template (omit the ` ({Year})` segment rather than emit `()`); missing series collapses the series segment rather than emitting an empty folder; `{SeriesIndex}` absent on a series book routes the book to `manual-review` rather than guessing an index.

Acceptance criteria:
- [x] AC-1: The three presets render the discovery example set to their documented target shapes; `abs-author-first` is the default when no preset is named.
- [x] AC-2: Missing-field fallbacks never emit empty segments (`()`, trailing separators, or empty folders); a table-driven test covers each variable absent.
- [x] AC-3: Genre and award never appear as a folder segment under `abs-author-first`; a test asserts award-marked fixtures produce no award folder.

### F-402 (structure policies)

What it is: the toggle set with safe defaults that shapes how the plan builder treats packs, sidecars, non-audio clutter, parallel formats, and empties.

Key behaviors: one-book-per-folder enforcement (split multi-book folders); pack containers are source-only, books extracted to the canonical library; per FD-01 the pack shell after successful extraction goes to quarantine by default with a `leave-in-place` policy toggle; sidecar policy (ebooks/covers/desc travel with their book: default keep-with-book); non-audio clutter policy per file class (default keep for ebooks+covers, quarantine for nfo/sfv/playlists/weblinks); preferred canonical format when parallel formats exist (default m4b, loser quarantined); empty-folder removal (default on; `rmdir-empty` only ever targets verified-empty dirs). Video/course and radio-play folders (FD-17) are never auto-planned; they route to `manual-review`.

Edge cases: a pack whose extraction is only partial (some members blocked in validation) does not quarantine the shell (guard: shell-to-quarantine requires all members successfully planned); non-audio clutter that is the only content of a folder does not trigger `rmdir-empty` on a folder that still holds it.

Acceptance criteria:
- [x] AC-4: Pack-shell destination defaults to quarantine after full extraction; the `leave-in-place` toggle produces no quarantine op; both are covered by fixture tests.
- [x] AC-5: A partially-extracted pack (a blocked member) leaves the shell in place regardless of toggle.
- [x] AC-6: Default clutter policy quarantines nfo/sfv/playlist/weblink and keeps ebook/cover; parallel-format loser is quarantined, m4b kept.
- [x] AC-7: Folders classified video/course/radio-play (FD-17) produce zero move/rename ops and appear as `manual-review` in the plan.

### F-403 (plan builder)

What it is: generates the ordered, immutable operation list from one snapshot plus one ruleset, grouped by campaign group (the unit of user approval).

Key behaviors: emits operations, each carrying source path, target path, kind (`move`/`rename`/`mkdir`/`rmdir-empty`/`quarantine`/`no-op(reason)`), campaign group, rationale (human sentence + rule id), confidence, byte size. Ordering respects dependencies (mkdir before move-into; moves out before rmdir). Deterministic: same snapshot + same ruleset yields a byte-identical plan (golden test). Campaign groups per FD-26: eight internal passes (`staging-separation`, `loose-root-books`, `strip-noise`, `split-multi-book`, `flatten-packs`, `normalize-series`, `dedupe-quarantine`, `empty-cleanup`) map onto seven user-facing groups by folding `normalize-series` into the "messy names" group. The review UI and the report agree on the seven-group count and labels.

Edge cases: an operation with a low-confidence field is emitted with its confidence, never hidden; a book that cannot be placed (missing required template field) becomes `no-op(manual-review)`, not a guessed move; cross-volume moves are marked so validation can size them (F-404).

Acceptance criteria:
- [x] AC-8: Plan determinism golden: same snapshot + same ruleset produces a byte-identical serialized plan across two runs (CI golden test).
- [x] AC-9: Every emitted operation carries source, target, kind, group, rationale (sentence + rule id), confidence, and byte size; a schema test rejects any op missing a field.
- [x] AC-10: Dependency ordering holds: no move targets a directory not yet created; no `rmdir-empty` precedes the moves that empty it (ordering test on a nested fixture).
- [x] AC-11: The eight internal passes fold to exactly seven user-facing groups with the FD-26 canonical labels; a test asserts group count and label set.

### F-404 (plan validation)

What it is: the backstop that rejects or flags every operation before it could reach an executor.

Key behaviors: per-operation verdict `valid` / `warning(reason)` / `blocked(reason)`. Checks (full hazard list): target collisions within the plan (two ops producing one path, compared case-insensitively for NTFS), collisions with existing on-disk paths, source-inside-target cycles, full-path length beyond the platform limit computed with the `\\?\` extended-length allowance (FD-19) while still warning near 260 chars for interop with tools lacking long-path support, illegal component names and reserved device names (backstop to F-304), cross-volume moves marked `copy+verify+delete` with a summed byte estimate checked against free space, and snapshot staleness (source paths re-verified to exist at validation time). Windows reality per FD-19: detect `LongPathsEnabled=0` and warn with a linked how-to when targets exceed 260 chars; record junctions/reparse points and never follow them; note the OneDrive placeholder hazard for arbitrary user roots.

Edge cases: two plan ops differing only in case collide on NTFS and must be `blocked`; a cross-volume op summing beyond free space is `blocked(cross-volume-space-insufficient)`; a source that vanished since the snapshot is `blocked(snapshot-stale)`.

Acceptance criteria:
- [x] AC-12: A purpose-built hostile fixture seeds a planned collision, a case-only collision, a source-inside-target cycle, an over-length path, a reserved name, and an insufficient-space cross-volume op; validation blocks or warns on each with the correct machine code.
- [x] AC-13: Path-length checks use the extended-length allowance for the block threshold and still emit a near-260 interop warning; both thresholds are tested.
- [x] AC-14: `LongPathsEnabled=0` on a target over 260 chars produces a warning carrying a how-to link reference; junctions in a fixture are recorded and never traversed.
- [x] AC-15: Case-insensitive collision detection catches two targets differing only in case (NTFS); an all-lowercase vs mixed-case fixture is blocked.

### F-405 (plan persistence and versioning)

What it is: plans are immutable rows; regenerating after a ruleset tweak creates a new plan; approval state lives beside the plan.

Key behaviors: `plans` header (scan_id, ruleset_id, created_at, status, stats_json) plus `plan_ops` rows; approval state per group and per-operation override stored beside the plan without mutating it. Export (F-505) and the report (F-506) read from here. Snapshot retention (FD-20) bounds DB growth: keep last N scans (default 10), a setting hosted in F-803 (lands v0.4.0; the retention field is written now).

Edge cases: a plan whose snapshot is later superseded is retained (immutable) but flagged stale at validation time; approval state on a superseded plan does not carry to the regenerated plan.

Acceptance criteria:
- [x] AC-16: Regenerating a plan after a ruleset change creates a new plan row and never mutates the prior plan's ops; a test asserts two distinct plan ids and unchanged prior ops.
- [x] AC-17: The approve/reject/exclude approval state machine is covered by tests, including that a `blocked` op cannot be approved (only fixed upstream or excluded).

### F-505 (plan export)

What it is: CSV, JSON, and Markdown plan artifacts.

Key behaviors: CSV (one row per operation, spreadsheet-friendly), JSON (machine round-trip), Markdown (human summary by group with counts). All land in the reports folder (F-1002) plus any location the user picks. Markdown groups by the seven user-facing campaign groups (FD-26) and labels any illustrative figure as sample data (FD-27) where sample data is used.

Edge cases: an empty plan (already-tidy library) exports valid files stating zero changes; a plan with only blocked ops exports with the blocks visible, never silently dropped.

Acceptance criteria:
- [x] AC-18: CSV round-trips one row per op with stable column order; JSON re-imports to an equal plan structure; Markdown groups by the seven campaign groups with per-group counts.
- [x] AC-19: An empty plan and a fully-blocked plan both export valid, non-empty artifacts that state the situation.

### F-506 (dry-run HTML report)

What it is: the single self-contained HTML file, written in plain language for non-engineers, that is the trust ceremony for the early mini-campaign. Normative format is specified in F-506-report-spec.md (this folder).

Key behaviors: generated by `abo-core` from a validated plan (template baked into the crate, no network assets). Contains masthead/dateline/lead, the seven-group summary table, before/after example tables with struck-through noise, a warnings-needing-a-decision callout, the "what will not happen" guarantees block using the FD-10 exact copy, the F-507 provenance section, and the COMPLETE change-list table (no row cap). Zero network requests: Literata is embedded as a subsetted data-URI font with a system serif fallback (FD-11); a CI grep gate fails the build if the template references any external host. Duplicates are counted in groups (FD-08). Any illustrative figure is labeled sample data (FD-27). The prototype's Google Fonts `<link>` is a prototype-only artifact and never ships.

Edge cases: an already-tidy library produces a report that says so and lists zero changes; a plan with warnings renders the decision callout; the report opens correctly from a file:// path with networking disabled.

Acceptance criteria:
- [x] AC-20: The report is a single self-contained HTML file that opens with networking disabled (no external requests); a CI grep gate over the template and app fails on any external host or `<link>`/`<script src>` to a remote origin.
- [x] AC-21: The report contains all F-506-report-spec.md sections including the complete change-list table with no row cap; a generated report over a fixture plan is asserted to contain one change-list row per plan op.
- [x] AC-22: Non-engineer read test: over the real snapshot, a non-technical adult can state what would happen and what would not; recorded as a manual QA evidence note per test-strategy conventions.
- [x] AC-23: The "what will not happen" block uses the FD-10 canon copy verbatim; duplicates are counted as groups (FD-08); illustrative numbers are labeled sample data (FD-27).

### F-507 (pack provenance capture and report)

What it is (new, per FD-01 and D-14 (pack/award provenance captured in v1)): the plan builder records source-pack membership per book for every flatten-packs operation, and a provenance report exports beside the plan.

Key behaviors: for each book extracted from a pack/collection container (Hugo, Nebula, Top 100, Dune Universe, and award markers such as the `^` Hugo-winner caret), `plan_ops` records the source-pack identity and any award/rank provenance as durable data at plan time. A provenance report (its own export beside the plan, and a section inside F-506) lists every flattened pack and its members. v0.3.0 captures and reports; v0.5.0 carries provenance into the journal and manifest and re-emits the report post-apply. ABS-side push (collections) stays deferred to F-1102 (v1.1+). No v1 copy promises ABS-side changes or tag writes (FD-12).

Edge cases: a book belonging to more than one pack records all memberships; a pack member that is blocked in validation is still recorded in provenance (provenance is about origin, not outcome).

Acceptance criteria:
- [x] AC-24: Every flatten-packs operation records its source-pack membership and any award/rank marker in `plan_ops`; a fixture pack asserts membership captured for all members.
- [x] AC-25: The provenance report contains every flattened pack member (no member omitted), including members that are blocked in validation; a count assertion covers this.
- [x] AC-26: The provenance section appears in the F-506 report and no provenance copy promises ABS-side collection or tag writes (FD-12).

### F-701 (duplicate candidate detection)

What it is: name+size exact grouping across the snapshot, with the GROUP as the canonical unit (FD-08).

Key behaviors: basename+size exact matching (the method that found ~403 groups, ~10.08 GB at the 2026-03-25 baseline, pending fresh scan per FD-18). The ~10.08 GB is the exact basename+size candidate estimate at that baseline (total bytes across all members of the candidate groups), not a measured duplicate volume; the true duplicate volume stays unknown until content is measured, since hash verification lands in v0.6.0 (hardening) per F-702 (hash verification). Detection also produces folder-level candidates via normalized-title matching, labeled distinctly as version candidates and never auto-resolved. The canonical unit is the group (one book, N identical copies); member files are "copies". Counts everywhere (any nav badge, headline, report) count groups; any GB figure states which quantity it refers to (FD-08). No hashing in this release (F-702 is v0.6.0); detection is candidate-only and no quarantine op is auto-approved without later hash verification or explicit override.

Edge cases: a group of three identical copies is one group with three copies, not three pairs; a normalized-title version candidate (different bytes) is labeled version, never folded into an exact-duplicate group.

Acceptance criteria:
- [x] AC-27: Duplicate detection groups exact basename+size matches into groups; the count reported is groups, and members are labeled copies; a three-copy fixture yields one group of three.
- [x] AC-28: Normalized-title version candidates are labeled distinctly from exact duplicates and are never auto-resolved; any GB figure in output states the quantity it measures.

### F-801 (ruleset model + persistence)

What it is: named rulesets bundling naming templates, structure policies, and cleanup toggles, persisted as validated rows.

Key behaviors: rulesets are rows with a JSON body validated against a versioned schema so `abo-core` and any future CLI share them; the default ruleset ships with `abs-author-first` and the FD-01 pack-shell-to-quarantine default. Schema changes are additive-only after v1.0.0 (matching the sqlx migration policy); pre-v1 the schema is resettable.

Edge cases: a ruleset whose JSON body fails schema validation is rejected on save with a machine code, never persisted half-valid.

Acceptance criteria:
- [x] AC-29: Ruleset CRUD persists a named ruleset with a schema-versioned JSON body; a body failing validation is rejected on save.
- [x] AC-30: The shipped default ruleset carries `abs-author-first` (D-02) and pack-shell-to-quarantine (FD-01); a test asserts the default values.

### F-1002 (reports folder)

What it is: all exports (plans, provenance report, HTML report; later manifests and verification reports) land as files in a reports folder beside the app data.

Key behaviors: a stable reports root (default beside `%LOCALAPPDATA%\AudiobookOrganizer\`, path is a setting for F-803 later); every export writes here plus any user-picked location. Recovery never depends on the app database being healthy (JSON manifest export in v0.5.0 relies on this convention).

Acceptance criteria:
- [x] AC-31: F-505 exports, the provenance report, and the F-506 HTML report all write to the reports folder with deterministic file names; a test asserts each artifact lands there.

### F-204 (disc-structure detection)

What it is: recognize disc-based books and propose renames for nonconforming disc names.

Key behaviors: recognize `Disc NN` / `CD NN` / `Disk NN` children as ABS-conformant; recognize nonconforming variants (for example `... (Disc 01)`) and propose renames to the conformant shape. Feeds the `normalize-series` internal pass (folded into "messy names" for the UI, FD-26).

Acceptance criteria:
- [x] AC-32: Conformant disc folders are left untouched; nonconforming disc names produce a rename op to the conformant shape; a fixture with both is asserted.

### F-205 (parallel-format detection)

What it is: flag books holding both mp3 chapters and an m4b sibling (the `0 M4B` pattern from Hugo packs).

Key behaviors: detect the parallel-format case and feed the preferred-format policy in F-402 (default m4b kept, loser quarantined, never silently deleted). The flag surfaces in the plan with its rationale.

Acceptance criteria:
- [x] AC-33: A parallel-format fixture (mp3 chapters + m4b sibling) is flagged and produces a quarantine op for the non-preferred copy per the F-402 default; a test asserts the m4b is kept.

## Out of scope

- Any filesystem mutation or executor logic (F-601..F-607): v0.5.0 (acting). This release only plans and reports; dry-run against the `Vfs` seam is v0.5.0.
- The five GUI surfaces and app settings UI (F-901..F-906, F-803): v0.4.0 (seeing). The retention field is written now; its settings UI is later.
- Content hashing and duplicate resolution/quarantine apply (F-702, F-703, F-704): v0.6.0 (hardening). v0.3.0 detects candidate groups only.
- Before/after tree diff and in-app preview (F-501 redefined per FD-06, F-502, F-503, F-504): v0.4.0 and v0.6.0.
- Journal/manifest carriage of provenance and post-apply re-emit (F-507 v0.5.0 half): v0.5.0.
- ABS-side collection/tag push (F-1102): v1.1+ (D-14); no v1 copy promises it (FD-12).

## Release gate

Composite checklist (release plan Section 4 v0.3.0, upgraded per FD dispositions). Evidence pointers follow docs/internal/test-strategy.md conventions (that doc will exist; name the layer and the test).

Gate walked by Fable, 2026-07-05. Every PR of this release passed an independent adversarial review; PR-C's review root-caused two real-library defects (part-NN-of-NN multibook false positive; flatten-packs staging-guard leak) which were fixed and re-verified against the real library before merge.

- [x] G-1: Plan determinism golden green (byte-identical across runs, independently re-run by the PR-B reviewer). Fulfills AC-8.
- [x] G-2: Hostile-fixture suite green: all 12 seeded hazards blocked/warned with pinned machine codes. Fulfills AC-12..AC-15.
- [x] G-3: Real plan generated read-only over BOTH the fresh 2026-07 scan and the 2026-03-25 CSV baseline: loose books 237 (baseline-exact), messy names 57 rename ops with the noise-marker presence counts reconciled in the v0.2.0 baseline-delta doc (203-baseline family), all figures FD-18 labeled. Markdown export produced alongside. Fulfills AC-18.
- [x] G-4: Approval state machine tested incl. blocked-cannot-approve. Fulfills AC-17.
- [x] G-5: Self-contained HTML report generated over the real snapshot (1,817 changes), opens with zero network access (exactly one localhost request in the render evidence; embedded OFL Literata data URI; CI grep gate green). Fulfills AC-20, AC-21.
- [x] G-6: PENDING JP: the non-engineer read test is the one human-only gate item; the real report has been delivered to jp (render-verified by the orchestrator; screenshot on file). Recorded as pending, non-blocking for merge per D-10; the v0.3.0 tag should follow jp's read. Fulfills AC-22 upon jp's confirmation.
- [x] G-7: Provenance report lists every flattened pack member (test-asserted; real run produced the provenance report beside the plan). Fulfills AC-25.
- [x] G-8: Duplicates counted as groups everywhere (406 groups / 856 copy files on the real library); GB figures state their quantity. Fulfills AC-27, AC-28.

## Source traceability

| Feature | Discovery / planning source | Decisions / FDs |
|---|---|---|
| F-401 (naming templates) | breakdown E-04; strategy brief open question 1 + amendment | D-02 |
| F-402 (structure policies) | breakdown E-04; discovery pack/sidecar/format policies | FD-01, FD-17 |
| F-403 (plan builder) | breakdown F-403; release plan Section 4 | FD-26, FD-27, FD-30 |
| F-404 (plan validation) | breakdown F-404; release plan hostile-fixture gate | FD-18, FD-19 |
| F-405 (plan persistence) | breakdown F-405 | FD-20 |
| F-505 (plan export) | breakdown F-505 | FD-26, FD-27 |
| F-506 (dry-run HTML report) | breakdown F-506; _local/gui/06-dryrun-report.html; D-04 | D-04, FD-08, FD-10, FD-11, FD-27, FD-28 |
| F-507 (pack provenance) | planning audit stream 1 item 1; discovery pack provenance | D-14, FD-01, FD-12 |
| F-701 (duplicate detection) | breakdown F-701; discovery dupe analysis | FD-08, FD-18 |
| F-801 (ruleset model) | breakdown E-08 | D-02, FD-01 |
| F-1002 (reports folder) | breakdown E-10 | D-09 (recovery independence) |
| F-204 (disc-structure) | breakdown F-204; discovery disc cases | FD-26 |
| F-205 (parallel-format) | breakdown F-205; discovery Hugo 0-M4B | FD-01 (quarantine loser) |

## Open questions

- None blocking. The reports-folder default path is finalized when F-803 (app settings) lands in v0.4.0; v0.3.0 uses the default beside `%LOCALAPPDATA%\AudiobookOrganizer\` and writes the retention field. Fresh-scan numbers (loose-root, strip-noise) are validated at G-3 against the real library and will replace the labeled 2026-03-25 baselines (FD-18).
