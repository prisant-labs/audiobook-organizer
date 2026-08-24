# Changelog

All notable changes to Audiobook Organizer are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

## [0.6.0] - unreleased

### Added

- **A Duplicates screen.** Everything the app had learned about repeated books
  was, until now, reachable from nothing: the comparison engine, the three ways
  of choosing which copy to keep, and the confirm step all existed and no screen
  called any of them. There is now a screen that does. It opens instantly and
  says plainly that nothing has been compared yet, because comparing means
  reading every candidate byte by byte and that is a thing you ask for rather
  than something that happens to you. When you do ask, it shows progress and can
  be stopped, and it keeps every comparison it finished. Copies are listed with
  three parts of their location rather than their name, because two copies of one
  book are called the same thing and the name is the one detail that cannot tell
  them apart.
- **A way to say which copy to keep, and to see what that would change.** Three
  choices, with a note on screen saying out loud that switching between them
  usually changes nothing. That note is deliberate: on copies that are genuinely
  identical the choices cannot tell them apart, and the alternative was letting
  the first person who switched conclude the app was broken.
- Development only: **the app now has one type scale instead of eighteen
  improvised sizes.** Seven sizes, derived from what the app already used most
  rather than chosen from taste, each sized so that text and spacing line up on
  the same underlying grid. Spacing deliberately got no new names, because
  measurement showed it was already consistent and only lacked a rule about which
  steps were allowed. Nothing a reader sees changes yet: the new scale is used by
  the Duplicates screen only, and the rest of the app moves onto it one screen at
  a time so that each change can be looked at.

- **Duplicate copies can now be resolved into real, undoable changes** (`P3`,
  `F-704`). Three ways to choose which copy to keep are available (keep the
  bigger one, prefer a single-file copy, or just flag them and decide later,
  which stays the default), and nothing is ever chosen for you: a copy only
  moves to the Archive after you confirm that group. Confirmed copies move as
  ordinary moves, so undoing a run puts every one of them back exactly where it
  was, proven byte for byte. Worth knowing about the three ways to choose: on
  copies that are genuinely identical they usually cannot tell the copies apart,
  because identical is what makes them a duplicate, so the honest answer in that
  common case is "these were equivalent" and the copy nearest the top of the
  library is kept.
- **The app can now read a book's contents to prove two copies really are the
  same.** Until this release nothing in the shipped product could read a file's
  bytes at all; the check existed but had only ever been run against test data.
  Measured on a real 299 GB library: checking one duplicate takes about a second,
  and checking every duplicate in the library takes a few minutes, once, in the
  background, and can be stopped. Files whose names are longer than Windows'
  old limit are read correctly rather than reported as unreadable.
- **A confirm step for archiving copies that were never compared.** Deliberately
  two presses rather than one, and it says plainly that the copies have not been
  compared file by file before it asks. **It is now reachable in the app**, on the
  Duplicates screen. The refusal behind it is not a matter of the screen
  remembering to ask: the engine itself declines to record a decision about
  copies it never compared unless that override is written down alongside it, so
  the answer to "were these compared when this was decided?" cannot be lost.
- Development only: the component gallery is now checked for words the product
  has retired, and so are this file and the status page. Retired wording had
  reached the gallery twice because nothing was looking.

- **Two copies of the same book are now found even when the book is split across
  many files** (`P2b`, `F-1110`). Every earlier duplicate check assumed one book
  is one file, so a book split across twelve mp3s was twelve unrelated files and
  its copies were never grouped at all. Book folders are now compared on what
  the scan already knows (title, how many audio files, how many bytes), then on
  the sizes of those files, and on request by reading the bytes themselves.
  A one-file copy and a twelve-file copy of the same book are shown together but
  never resolved automatically: choosing between them is a preference, not a
  ranking a tool should make.
- **A run is refused while the previous one cannot be accounted for.** If a run
  was cut short and the app could not work out what it had already done, it will
  not start another one from a library it cannot read. Putting the interrupted
  run back is still offered, because that is the remedy.
- **A component gallery** for reviewing the app's real components side by side in
  both themes. Development only; it is not part of any build a user installs.
- The interruption recovery surface (`P1c`): after a run is cut short, the app
  now says so on next launch instead of recovering in silence. Three states from one
  reconciler result: an interrupted practice run (nothing in the library was touched),
  a real run stopped early with a verified outcome (carry on, or put the changes back),
  and one whose last step could not be confirmed (carrying on is not offered). The
  surface renders the engine's own answers and derives none of them.
- `F-606` (interruption safety): after a process kill mid-run, startup finds the
  single in-doubt operation, verifies what actually happened on disk, repairs the
  journal with the terminal record the kill prevented, and reports what can be done
  next. Reads the filesystem; never mutates it.
- History screen and its engine read model: every past run, what it changed, and
  the one honest action available for it. Undo is prepared as a reviewable plan, not
  executed by a button.
- `history_list` command; `AppError::HistoryUnavailable` with family-safe copy.
- Kill-process recovery tests: a feature-gated binary runs a real apply against a
  real temp library and then calls `abort()` mid-operation, so recovery is proven
  against a genuinely killed process rather than a hand-built journal state. Covers
  intent-then-kill (the source is still in place) and act-then-kill (the rename
  landed, so the journal is repaired as done, not failed).
- `docs/internal/backlog/`: deferred, raised, and answered, with a README stating that
  nothing leaves the backlog by being forgotten. Created because "I will add it to the
  backlog" had been said when there was nowhere to add it.
- **You can open any folder the app shows you, in Windows Explorer.** Wherever the app
  names a location - beside a book you are reviewing, beside a copy on the Duplicates
  screen - there is now a way to open it, and two permanent links in the sidebar go
  straight to your library and your Archive. The app opens folders in those two places
  and nowhere else: a location it cannot prove belongs to one of them is refused,
  including one that has stopped existing. That refusal lives in the engine rather than
  in the button, so it holds no matter which part of the app asks next, and nothing
  about what the app is allowed to reach on your machine had to change to make any of
  this work.
- The duplicates approach audit (`docs/internal/audits/`), which re-derives P2 through
  P5 against the shipped code and finds the specs sound for single-file books and
  silently narrow for multi-file ones.

### Changed

- The app's own styling values are frozen at their measured count and checked on
  every push, in **both** directions. A ratchet that only fails upward goes stale:
  remove ten values and the slack is silently available for ten new ones.
- **Non-audio leftovers now stay where they are** (`FD-40`). `.nfo`, `.sfv`, playlist
  and web-link files join ebooks and cover art in defaulting to Keep. The per-type
  setting was always there; only the starting point moved. The reason: a library is not
  only an Audiobookshelf feed, it is also the owner's filing system, and companion
  material is often sitting beside a book on purpose.
- **"Set Aside" is now "Archive"** (`FD-42`), with the folder on disk named
  "Audiobook Archive". Two names for two contexts: short enough for a button inside an
  app that is entirely about audiobooks, and self-explanatory as a directory sitting
  beside your library with no app running. "Backup" was considered and rejected: it
  implies the original is still in your library and this is a spare, which invites
  deleting the one folder that makes undo possible.
- **The action is "organize", and the noun is retired rather than replaced** (`FD-48`,
  superseding `FD-43`). Measured before it was decided: in the shipped copy the noun
  outnumbered the verb roughly three to one, so a straight swap would have broken most
  strings, not one. Where copy needs a noun it now uses one the register already
  carried: "the plan" for what is being reviewed, "the changes" for what it would do,
  and "run" for one past execution, so History rows read "Real run" beside "Practice
  run". Engineering identifiers do not move; renaming those is a migration, not a copy
  change.
- `keep-higher-bitrate` is cut from v1 (`F-1108`). Not because bitrate cannot be read,
  but because file size already tells you the same thing for free: for the same book, a
  higher-bitrate copy is a larger file.
- README states plainly which parts of the app work today and which are aspiration.
  Real changes remain unreachable from the UI by design.
- The project is now a public, MIT-licensed repository (`FD-38`). It was republished as
  a new repository from clean history rather than switching visibility, because
  server-side pull-request refs on the original could not be rewritten. That original
  was deleted on 2026-08-02 once a verified local backup existed, which removed the
  last server-side copy of those refs.
- Documentation reconciled with that change: the governance docs no longer tell an
  agent it may self-merge, and a SECURITY.md records the disclosure path along with
  the four known safety gaps.
- Dependencies updated across the Rust, JavaScript, and GitHub Actions groups.
  TypeScript is held on 6.x: typescript-eslint 8.x refuses to run against TypeScript 7
  and exits rather than degrading, which takes the whole lint gate down while every
  other check still passes. The constraint and its removal condition are recorded in
  `.github/dependabot.yml`.

### Fixed

- **Stopping a comparison left the screen stuck.** Pressing Stop while copies were being
  checked did stop the work, and then left a progress bar and a Stop button on screen for
  something that had already finished stopping. It also would not start another
  comparison until you navigated away and came back. The cause: a job that is cancelled
  records that it stopped without announcing it, so the screen sat waiting for a message
  that was never coming. Stop now clears the screen and re-reads what finished, which
  matters on its own, because a stopped comparison keeps every copy it had already
  checked and that work used to stay invisible until something else caused a reload.
- **The duplicate count no longer inflates for a book split across many files.**
  Two identical copies of a twelve-part book carry twelve identically named,
  identically sized parts, so each part was counted as its own set of copies: a
  library with two duplicated books reported fourteen. One duplicated book is
  one entry now. Found by generating the dry-run report and reading it; every
  test passed the whole time.
- A run no longer halts partway through on an ordinary multi-book folder. The
  planner decided a folder had been emptied by counting only its audio files, so once
  leftovers began being kept, it would ask to remove a folder that still had one in it.
  The executor refuses that and stops the run, after earlier moves have already been
  applied. Found by an adversarial review before release; the planner now checks every
  child and keeps the folder whenever it cannot prove otherwise.
- Startup reconciliation was mode-blind: it probed the real library to classify
  interrupted jobs without checking whether the job was a real run or a practice
  run. Since the UI pins practice runs, this affected every interruption in practice
  and could offer a recovery with no connection to the run that was lost. Now gated on
  the recorded mode, failing closed when the mode is unreadable or when more than one
  run is stranded.
- The `reconcile-failed` error reached the frontend with no plain-language copy,
  breaking the type check and the error-copy exhaustiveness test.
- Interruption recovery treated a failed filesystem probe as evidence. Checking
  existence with a boolean answers "false" for a permission denial exactly as it
  does for a genuine absence, so a cross-volume move whose source was momentarily
  unreadable could be recorded as completed. Probing now separates "not there" from
  "could not tell", and anything unreadable is treated as ambiguous.
- A recovery that refused to act could not be retried. Refusing left the run marked
  as running, the startup reclaim then marked every running run failed, and the
  sweep only looked at running runs, so the refusal erased its own retry condition.
  The disposition is now recorded durably before the reclaim.
- History could offer to put back a run that was still going, one recovery had
  refused to explain, one with an operation whose outcome was never recorded, or one
  whose undo file says it is not fully reversible. All four now say the run needs a
  look instead.
- Preparing an undo left the review screen pinned to it, so navigating away and back
  reopened the undo and "build the plan again" rebuilt the undo rather than a fresh
  forward plan.

## [0.5.0] - unreleased

### Added

- F-601 (executor): applies approved plans in dependency order on Windows;
  rename-first for same-volume moves, copy+verify+delete for cross-volume.
- F-602 (journal + undo manifest): append-only journal-before-act per operation;
  JSON manifest exported to Reports so recovery never depends on a healthy database.
- F-603 (rollback): full or partial undo from a manifest, run through the same
  validate and apply pipeline as a forward run.
- F-604 (post-apply verification): verifies moved trees, refreshes snapshot, and
  blocks further campaign groups until any discrepancy is acknowledged.
- F-605 (Archive / quarantine): move-not-delete holding area outside the library
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
- F-902 (library home): cover-forward library rendering with health facts inside
  sentences; no stat bands or hero metrics.
- F-903 (plan preview surface): hosts campaign group review, search, and
  explainability per change.
- F-502 (campaign group review): approve, reject, or defer per campaign group;
  per-change override available inside group detail.
- F-503 (search and filter): find any book, path, or rule across the plan.
- F-504 (explainability): every change shows matched pattern, rationale, and
  confidence tier.
- F-906 (settings + ruleset editor): ruleset CRUD with live re-plan preview counts.
- F-803 (app settings): library roots, Archive root, reports folder, theme, and
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
