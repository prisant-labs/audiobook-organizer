---
title: Audiobook Organizer - Design System
date: 2026-07-03
status: review
owner: jprisant
produced-by: AUTHOR agent (design-system)
sources:
  - PRODUCT.md
  - _local/gui/README.md
  - _local/gui/04-library.html
  - _local/gui/05-review.html
  - _local/gui/06-dryrun-report.html
  - _local/gui/07-complete-flow.html
  - _local/planning/feature-function-breakdown_2026-07-02.md
  - docs/internal/decision-ledger.md
  - docs/internal/planning-audit-2026-07-03.md
---

# Audiobook Organizer - Design System

This document freezes the set-2 prototype visual language into normative text that implementation renders in v0.4.0 "seeing" with shadcn/ui against the tauri-specta bindings. The prototypes in `_local/gui/` (04, 05, 06, 07) are the visual reference; this document is the canon. `PRODUCT.md` principles govern. Where a prototype conflicts with `PRODUCT.md` or a decision in the decision ledger (D-nn / FD-nn), this document records the corrected canon and the prototype is treated as superseded on that point.

Precedence when sources disagree: the decision ledger (docs/internal/decision-ledger.md, D/FD) > PRODUCT.md > planning docs > discovery docs > prototypes.

Section 4 (component inventory) and Section 5 (state definitions) are written to be lifted directly into the v0.4.0 spec (F-908 (error/empty/loading states), F-909 (first-run and root selection)) as acceptance criteria. They are DESIGNED here so the spec carries AC rather than inheriting the prototypes' happy-path gap (planning audit stream 2 items 1 and 6, docs/internal/planning-audit-2026-07-03.md).

---

## 1. Design principles and anti-references

### 1.1 Principles (from PRODUCT.md, normative here)

The five `PRODUCT.md` principles govern every surface. Restated as design-system obligations, not at length:

1. Plain language first. Books, library, duplicates, organize. Technical detail lives behind one consistent "Show file details" disclosure (Section 6, Section 7).
2. Preview before touch. Every destructive-adjacent flow is scan then review then confirm. The review screen and the exported HTML report are the trust ceremony.
3. Nothing irreversible. No deletion vocabulary in the UI. Duplicates are "moved to the Archive", changes are "undoable", and the interface says so where the anxiety happens (Section 6 reassurance canon).
4. The books are the interface. Covers and spines carry warmth and recognition; app chrome stays quiet and neutral so the collection is the color.
5. One calm primary action per screen. Every surface answers "what should I do next" with exactly one obvious button (Section 7).

The design bar is set by tier 2 (household non-engineers, D-03 (audience: all three tiers)): if a household member could not confidently review and confirm a run, the surface is wrong.

### 1.2 Anti-references (D-06 (anti-reference: AI-dashboard look))

The following read as AI-generated and are prohibited on primary surfaces. The first prototype set `_local/gui/01-03` is the recorded example of each and is a deliberate anti-reference:

- Hero-metric stat bands and big-number KPI tiles.
- Uppercase tracked "eyebrow" labels on every section.
- Editorial-serif-plus-noise-texture staging; glassmorphism; identical repeating card grids.
- Dev-tool aesthetics on primary surfaces: monospace everywhere, streaming journals, SHAs, "operations" vocabulary in front of non-engineers.
- Heavy Electron chrome and busy multi-panel density. The tool is occasional-use and must feel light.

Two consequences carried as hard rules:

- The word "dashboard" never appears in a user-facing surface or feature name. F-902 is renamed "library home" (FD-07 (F-902 rename to library home)).
- Audiobook cover art is SQUARE (1:1), never 2:3 portrait, and never carries fake spine-edge shading. Series spine CLUSTERS are the single sanctioned exception: they keep a stylized vertical-spine metaphor deliberately (D-06; planning audit stream 4 item 3).

---

## 2. Theme system

### 2.1 Canonical identifiers (FD-09 (theme identifiers))

Two themes, one design system, shipped as themes and not as separate designs (D-05 (two moods of one system)):

| Canonical `data-theme` | UI label | Mood | Prototype id (superseded) |
|---|---|---|---|
| `day` | Day | Calm daytime utility, light | `calm` |
| `evening` | Evening | Warm cover-forward library, dark | `evening` |

The prototypes ship `data-theme="calm"` for the light theme; the canonical attribute value is `day`. The prototypes' "calm"/"evening" and the report's "prose" naming triple (planning audit stream 2 item 16) is resolved here: `day` and `evening` are the only theme identifiers in code; `Day` / `Evening` are the only UI labels. The exported report's paper theme (Section 3.4) is a third, non-toggleable register, not a fourth theme id.

Default theme is `day`, persisted via F-803 (app settings) and re-applied at startup. First run selects `day` (FD-05 (first-run and root selection)).

### 2.2 Token tables (extracted verbatim from the prototypes)

Values below are extracted from the `:root` and `html[data-theme=...]` blocks of 04, 05, and 07. Where a token appears in some prototypes and not others (`--link`, `--shelf-rail`, `--cover-shadow`, `--titlebar`, `--mono`), the consolidated canon is the union; implementation defines all of them in both themes.

Shared, theme-independent:

| Token | Value | Purpose |
|---|---|---|
| `--sans` | `"Segoe UI Variable Text","Segoe UI",system-ui,sans-serif` | UI body and controls |
| `--serif` | `"Literata",Georgia,serif` | Headings, cover titles |
| `--mono` | `"Cascadia Code",Consolas,monospace` | Path disclosures only |
| `--r` | `6px` | Standard control radius |

Day theme (`data-theme="day"`):

| Token | Value | Token | Value |
|---|---|---|---|
| `--bg` | `#fafafb` | `--good` | `#2f7a4c` |
| `--surface` | `#ffffff` | `--good-bg` | `#e7f2ea` |
| `--surface-2` | `#f1f1f4` | `--warn` | `#7c5a10` |
| `--border` | `#e3e3e9` | `--warn-bg` | `#f5edd8` |
| `--border-2` | `#cfcfd8` | `--alert` | `#a34b2a` |
| `--ink` | `#26252e` | `--alert-bg` | `#f7e7dd` |
| `--ink-2` | `#4b4a56` | `--shelf-rail` | `linear-gradient(180deg,#dddde3,#cbcbd4)` |
| `--ink-3` | `#6d6c79` | `--cover-shadow` | `0 2px 5px rgba(30,28,45,.18)` |
| `--primary` | `#4a3fb0` | `--titlebar` | `#f3f3f6` |
| `--primary-hover` | `#3d3399` | `--link` | `#4a3fb0` |
| `--primary-ink` | `#ffffff` | | |

Evening theme (`data-theme="evening"`):

| Token | Value | Token | Value |
|---|---|---|---|
| `--bg` | `#211b15` | `--good` | `#7fc492` |
| `--surface` | `#2b241d` | `--good-bg` | `rgba(127,196,146,.13)` |
| `--surface-2` | `#352d24` | `--warn` | `#dcb25e` |
| `--border` | `#3d352b` | `--warn-bg` | `rgba(220,178,94,.13)` |
| `--border-2` | `#4f4436` | `--alert` | `#e08d63` |
| `--ink` | `#f0e9dd` | `--alert-bg` | `rgba(224,141,99,.14)` |
| `--ink-2` | `#cfc4b2` | `--shelf-rail` | `linear-gradient(180deg,#5a4630,#3f2f1d)` |
| `--ink-3` | `#a2967f` | `--cover-shadow` | `0 3px 8px rgba(0,0,0,.45)` |
| `--primary` | `#6a5cd0` | `--titlebar` | `#1b1611` |
| `--primary-hover` | `#7264d8` (corrected from the prototype's `#7a6ce0`; see Section 8 note) | `--link` | `#b3a6f2` |
| `--primary-ink` | `#ffffff` | | |

Semantic status pairs map to product meaning: `--good` = organized / included / verified; `--warn` = held / checking / needs-you-soft; `--alert` = structural attention (box set, multiple copies). `--alert` (terracotta) is NOT an error color; error/danger gets its own pair below.

### 2.3 New error/danger token pair (FD-09)

The prototypes have no error color. The one red present is the window-close hover `#c4262e`, a chrome affordance, not a content token. FD-09 requires a dedicated error/danger pair distinct from `--alert` terracotta, WCAG AA compliant in both themes. Proposed values, with computed contrast (Section 8):

| Token | Day value | Evening value |
|---|---|---|
| `--danger` | `#b3261e` | `#f2857a` |
| `--danger-bg` | `#fbeae8` | `rgba(242,133,122,.15)` |
| `--danger-ink` | `#ffffff` | `#211b15` |

`--danger` is reserved for genuine failure and irreversible-adjacent warnings (scan failure, apply failure, corrupt-DB notice, permission denied, the dupes-override confirm). It is visually redder and colder than `--alert` so a family reader distinguishes "something broke" from "this book needs a structural decision" by hue plus the always-present icon and label (Section 8, never color-alone). `--danger-ink` is the text color for solid danger fills (a destructive confirm button, used sparingly and never as the one calm primary action).

---

## 3. Typography

### 3.1 Type stacks and roles

| Role | Stack | Notes |
|---|---|---|
| UI body, controls, metadata | `--sans` (Segoe UI Variable) | Native Windows text; no webfont |
| Headings, cover titles, big percent | `--serif` (Literata) | Bundled webfont, FD-11 |
| Path disclosures only | `--mono` (Cascadia Code) | Inside "Show file details" `<pre>` only; never a primary surface (D-06 dev-tool anti-reference) |

Observed scale from the prototypes: `h1` 26-30px Literata weight 500, letter-spacing -.01em, `text-wrap:balance`; section `h2` 19-21px Literata weight 500; body lede 13.5-14.5px `--ink-2` with `max-width` 46-56ch and `text-wrap:pretty`; control text 13px weight 600; metadata 11.5-12.5px `--ink-3` with `font-variant-numeric:tabular-nums` on all counts. The scan and organize "big percent" is Literata 64px weight 500, tabular-nums, with a 0.35em `--ink-3` percent sign.

### 3.2 Literata bundling (FD-11 (fonts bundled, zero network))

Literata (SIL OFL) is bundled in-app as self-hosted `woff2`, loaded via `@font-face`, not a network `<link>`. The prototypes' `<link rel="preconnect">` and Google Fonts stylesheet `<link>` (present in all four files) are prototype-only artifacts and NEVER ship (planning audit stream 2 item 10). Zero network requests in the app or the exported report is an invariant; a CI check greps the app bundle and the report template for external hosts and fails on any (owned by the ci-plan; this document states the requirement). System fallback stack is `Georgia, serif`.

### 3.3 Font-variant discipline

All numeric counts (change counts, GB figures, series counts, nav badges, progress percent) use `font-variant-numeric:tabular-nums` so digits do not jitter as values update during scan and organize progress.

### 3.4 Report register (FD-28 (report format spec))

The exported dry-run HTML report (F-506 (dry-run HTML report), 06-dryrun-report.html) uses a single light "paper" theme, deliberately distinct from the app's Day/Evening themes: serif body throughout (Literata as the body face, not just headings), a printed-document look on `#f2f2f5` behind a white `.sheet`, `--sans` reserved for tables, captions, and chrome. This paper register is by design and does not follow the app token themes. The report embeds a subsetted Literata as a `data:` URI with a system serif fallback (FD-11). Print rules (`@media print`), the full change-list table, and the deletion-guarantee block are specified in the F-506 report spec, not here; this document only fixes the register.

---

## 4. Component inventory (normative)

Each component cites the prototype file it is extracted from. Measurements are the prototype values and are the default target; shadcn/ui primitives may back them as long as the rendered result matches.

### 4.1 Titlebar (custom, `decorations:false`)
Source: 04, 05, 07. 40px tall custom titlebar (Tauri window decorations off). Left: bookshelf logo glyph plus "Audiobook Organizer" (optionally " - Organize" on the review window) at 12.5px `--ink-2`. Right, before caption buttons: the theme segmented control. Far right: minimize / maximize / close caption buttons, 46px wide; close hovers to `#c4262e` white. Background `--titlebar`, bottom `1px solid --border`.

### 4.2 Theme segmented control
Source: 04, 05, 07. Pill (`border-radius:999px`) with two buttons Day / Evening. Selected button carries `aria-pressed="true"`, `--surface-2` background, `--ink`, weight 600; unselected `--ink-3` transparent. `:focus-visible` outline 2px `--primary`. This is the canonical theme toggle (Section 7 interaction AC).

### 4.3 Sidebar nav with count badges
Source: 04, 07. 212px column, `--titlebar` background, `1px` right border. Items: Library, Organize, Duplicates, History, Settings. Active item carries `aria-current="page"`, `--surface-2`, weight 600. Right-aligned count badge `.n` in `--ink-3` tabular-nums (e.g. Duplicates count, Organize "ready"/"done", History count). Footer `.end` block: two quiet lines, e.g. "Everything stays on this computer." / "Nothing is ever deleted." Nav counts follow the duplicates-unit canon (FD-08, Section 6): the Duplicates badge counts GROUPS, not copies. Settings is a real destination (FD-05), never a dead link (the prototype 07 disabled Settings is a prototype limitation).

### 4.4 Cover tile (square 1:1)
Source: 04, 05, 07. Square cover, `aspect-ratio:1/1`, `border-radius` 4-6px, `--cover-shadow`. Title in `--serif` weight 600; author in uppercase 0.04em tracked micro-caps at bottom (`margin-top:auto`, opacity .85). Size scale: `sz-lg` 112px (home shelf), `sz-md` 64px (review examples, duplicates). Deterministic decorative "pattern variants" drawn in CSS, never bitmap: `f-band` (horizontal band), `f-circle` (corner disc), `f-frame` (inset frame), `f-lines` (stacked rules). These stand in for real cover art until F-907 (cover extraction) lands. Covers are never 2:3 and never carry fake spine shading (D-06).

### 4.5 No-cover fallback tile (FD-03 (F-907 covers + fallback))
When no embedded art or `cover.jpg` sidecar is found (F-907 reads both, read-only), render a fallback tile: same square 1:1 footprint, background color derived deterministically from a hash of the title (so the same book always gets the same color and the shelf stays stable across scans), the title text set in `--serif`, author micro-caps as usual. No pattern glyph. The fallback is a designed state, an acceptance criterion for F-907, not an error. A shelf of all-fallback tiles must still read as a warm bookshelf, not a broken grid.

### 4.6 Book slot plus reason chip
Source: 04, 07. `.bookslot` is a 118px column: cover on top, one `.why` reason chip below. Chip is a pill with a 10px inline icon plus a short reason. Variants: `.why.warn` (messy name, loose file) and `.why.alert` (structural: "7 books, 1 folder", "2 copies"). Icon plus label always; never color-alone (Section 8).

### 4.7 Shelf row plus rail
Source: 04, 07. `.row` is a horizontal, overflow-x scrolling flex row of book slots or clusters, aligned to `flex-end`. Beneath it, `.rail` is a 7px bar filled with `--shelf-rail` gradient, giving the bookshelf-edge metaphor. Section head (`.shelfhead`): Literata `h2`, a quiet `--ink-3` sub-line, and a right-aligned link (e.g. "See all 412"), links colored `--link`.

### 4.8 Series spine cluster
Source: 04. `.cluster` renders a series as a group of stylized vertical spines (`.spine`, 17px wide, deterministic height jitter, occasional `.lean` tilt) with the series name written vertically on the center spine. Below: `.clab` caption "**Series** by Author - N books (M not shown)". Real series only (Dresden Files, Wheel of Time, Dune, Harry Potter, Wings of Fire in the prototype). The stylized spine metaphor is the deliberate exception to the no-spine-shading rule (D-06).

### 4.9 Group card plus include/skip switch plus status tag (the "bundle")
Source: 05, 07. Review left column. `.bundle` card: title `h3` (weight 600), plain-language "why" paragraph, a `.meta` line (tabular-nums count plus GB, e.g. "238 books - 67.9 GB"). Selected card carries `aria-pressed="true"` (corrected from the prototype's `aria-selected`, v0.4.0 Phase 8 T-36: `aria-selected` is not an allowed attribute on `role="button"`, and the roles that do allow it require a `listbox`/`option` container the switch column cannot share without also becoming an illegal listbox child - see GroupCard.tsx) with a `--primary` ring. Right column of the card: a `role="switch"` toggle (`.sw`, `aria-checked`) and a status tag `.stag`, rendered as a SIBLING of the selectable region, never nested inside it (nested-interactive). Tag variants: `.in` "included" (`--good`), `.out` "left out" (`--ink-3`/`--surface-2`), `.hold` "checking copies" (`--warn`, switch disabled). Cards are keyboard-selectable (`tabindex`, Enter selects; Section 7). The seven cards are the seven campaign groups (FD-26 (seven campaign groups), Section 6.7).

### 4.10 Example card with Now/After breadcrumbs
Source: 05, 07. Right column detail. `.example` row: `sz-md` cover plus a body with the book name `h4`, then a two-line `.ba` block labeled "Now" and "After". "Now" is a plain-language description of the current mess in `--ink-2`; "After" is a breadcrumb (`.crumb`) of the destination folder path rendered as words joined by a subtle `>` separator in `--ink-3`. Paths are NOT shown here; they live behind the disclosure (4.12). A `.morenote` foot line ("...and 234 more books just like these. The full list is in the report.") closes the group.

### 4.11 Warning pill (needs-you flag)
Source: 05, 07. `.flag` inline pill in `--warn`/`--warn-bg` with a warning glyph, for a per-example caution such as "Waits for the duplicate check, so you never end up with two folders for one book." Distinct from `--danger`: this is a soft "held / needs you" state, not a failure (Section 6 "needs you" register).

### 4.12 "Show file details" disclosure (extended, FD-13 (path exception + disclosure content))
Source: 05, 07 (`<details><summary>`). A single consistent disclosure per example, closed by default, summary in `--link`. Open reveals a `--mono` `<pre>` with `--surface-2` background. Prototype content is the raw before/after path. Extended canon (F-504 (explainability), FD-13): the disclosure holds (1) the raw source and target paths, (2) the matched pattern in plain terms (e.g. "Matched pattern: year-author-title"), and (3) a confidence indication. This is the one sanctioned place raw paths appear on primary surfaces, for tier 1 (Section 9 path-exception rule). Everything outside the disclosure stays plain-language.

### 4.13 Sticky footer action bar
Source: 05 (`.footer`), 07 (`.footbar`). Bottom bar, `--titlebar` background, top border. Left: a reassurance line with a shield-check glyph in `--good`: "Nothing is deleted. Every change can be undone." Right cluster: a count summary ("6 of 7 groups - 982 changes", tabular-nums), a "Save report" secondary button, and one primary button "Organize now". Exactly one primary action (Section 7).

### 4.14 Two-step inline confirm strip
Source: 07 (`.confirm`). Clicking "Organize now" does NOT open a modal. It swaps the primary button for an inline confirm strip in the same footer: "Ready: 982 changes, undo will be available." plus a primary "Go ahead" and a quiet "Not yet". This is the canonical confirm pattern: inline, reversible, never a modal dialog (Section 7).

### 4.15 Toast
Source: 07 (`.toast`). Transient bottom-center pill, `--ink` background / `--bg` text, fades in and auto-dismisses (~3.6s), `pointer-events:none`, `max-width` ~520px, centered. For lightweight confirmations ("Saved: Reports\dry-run-report.html"). Not for errors that need a decision; those are surfaces (Section 5), not toasts.

### 4.16 Progress set
Source: 07 (scan and organize screens). A `.centercol` (max ~660px) holding, top to bottom: the big percent (`.bigpct`, Literata 64px, tabular-nums), a thin progress bar (`.pbar` with a `--primary` `.pbar i` fill, transform-scaled), a one-line status (`.pline`, e.g. "8,540 of 13,970 files read" using a friendly location name, not a raw path, FD-13), an optional step checklist (`.plist`/`.pitem` with done/active states and per-step counts, ✓ marks in `--good`), and a rolling feed (`.feed`, last ~4 human-readable lines, e.g. "Moved **Sapiens** into Yuval Noah Harari > Sapiens (2011)"). The prototype's "Skip ahead" link is demo-only and NEVER ships (FD-02 (pause/resume + stop)). Every progress screen carries a real Stop control; the organize screen also carries Pause (Section 5.4, Section 7).

### 4.17 Delta list (done screen)
Source: 07 (`.deltas`/`.delta`). Post-run summary rows: a bold label plus a right-aligned before-to-after count where the old value is struck (`<s>582</s>1,013`), tabular-nums. Includes the guarantee row "Books deleted - 0, as always". Numbers live inside labeled rows, not in a stat band (D-06).

### 4.18 Next-step card
Source: 07 (`.nextcard`). A single bordered card on the done screen: "One step left, in Audiobookshelf", with the one place a raw path is allowed on a primary surface outside the disclosure (the ABS library path in a `code` span, FD-13), so tier 2 can configure ABS. Copy corrected per FD-12 (genre replacement copy, Section 6.5): no "old genre view lives on as tags" claim.

### 4.19 History item
Source: 07 (`.hitem`). A bordered row per run: title with date ("Real run - today, 6:31 pm"), a plain summary ("982 changes across 6 groups - verified afterwards - 0 deletions - undo available"), and right-aligned "Report" and "Undo" buttons. Undo stays available until cleared.

### 4.20 Duplicate row
Source: 07 (`.dupe`). A row per duplicate GROUP (FD-08): `sz-md` cover, the book title `h4`, a plain description of the overlap ("Identical in the Hugo and Nebula bundles. Keeper: the Locked Tomb library copy."), and a status tag ("checked, identical" `--good`; "waiting" `--ink-3`; "needs you" `--warn`). The unit is the group; member files are "copies" in the description (Section 6.3).

---

## 5. Error, empty, and loading states (NEW, FD-04 (F-908 states))

The prototypes are happy-path only (planning audit stream 2 items 1, 6). These states are designed here so the F-908 spec carries acceptance criteria. Each maps an AppError family or edge condition to a family-safe surface. Every one: uses the icon-plus-label rule, offers exactly one calm primary action (or an explicitly disabled one with a reason), and stays in plain language. Error surfaces use the `--danger` pair (Section 2.3); "held / needs you" states use `--warn`.

### 5.1 Error states

Blocked campaign group (in review). Layout: the affected group card shows the `.hold` treatment with the switch disabled and a one-line reason inside the card ("This group can't be included yet - one folder needs a decision first."). Tone: calm, not alarming. Primary action: none on the card; the fix is upstream (adjust and re-scan) or exclude. Tokens: `--warn`. This is a held state, not a failure.

Scan failure. Layout: the scan screen's percent and feed are replaced by a centered short message with a `--danger` icon: "The scan stopped before it finished." One plain sentence on what is safe ("Nothing was changed - a scan only reads."). Primary action: "Try the scan again". Secondary: "Show file details" disclosure with the technical error and the folder involved. Tokens: `--danger`, `--danger-bg`.

Apply failure plus resume choice. Layout: the organize screen stops; a `--danger` surface states which book the run stopped on and that everything up to that point is safe and already recorded for undo ("Everything done so far is saved and can be undone."). Primary action: "Resume the run" (F-608 (pause/resume apply) resume semantics). Secondary: "Undo what was done" and a "Show file details" disclosure. Retry-once-then-halt-group executor behavior on access-denied (FD-19 (Windows path and Defender reality)) surfaces here as the halt message. Tokens: `--danger`.

Interrupted run, recovery choice (v0.6.0 P1c, F-606 (interruption safety)). Trigger: a previous session was killed mid-apply, and the startup reconciler verified the outcome and repaired the record before the app opened. Distinct from "Apply failure plus resume choice" above, which is the in-session halt using F-608 (pause/resume apply) semantics; this is the across-a-restart case, where no live job exists to resume. Layout: replaces the screen area, sidebar left live. Navigation is deliberately NOT blocked: the dangerous action is starting a new run rather than using the app, and a navigation block would be a procedural gate that stops nothing an IPC caller can reach. The gate that matters is engine-side, beside `ensure_forward_tidying_allowed`, and is recorded in STATUS.md as a precondition for enabling real changes. Three states from one component. (a) An interrupted practice run: `--warn`, flask glyph, "Nothing in your library was touched", one calm action back to the library, because a rehearsal has nothing to carry on from and nothing to put back. (b) A real run stopped early with a verified outcome: `--warn` (this is "needs you", not a failure), warning glyph, the count of books moved, primary "Carry on organizing" which re-scans and re-plans (FD-39 (carry-on by re-planning)) and never replays, secondary the engine-resolved undo. (c) A real run stopped early with an unconfirmed outcome: `--danger`, warning glyph, carrying on is NOT offered, because a cross-volume copy killed mid-write leaves a target that exists but may be truncated and a fresh scan would read it as a whole book; the only actions are whatever undo the engine allows and opening History. All three carry a "Show details" disclosure holding plain facts only: no paths, no ids, no journal (AC-6 in v0.6.0 hardening, FD-13). The surface renders the engine's answers (`resume_offered`, `UndoOffer`) and derives neither, per FD-36 (History and undo).

Snapshot-stale re-validation prompt. Trigger: the library changed on disk since the plan was built. Layout: a calm inline banner above the review, `--warn`: "Your library changed since this plan was made. A quick re-check keeps the plan accurate." Primary action: "Re-check now" (re-validate). The plan is not applied until re-validated. Tokens: `--warn` (this is caution, not failure).

Corrupt-DB recovery notice. Trigger: the local database cannot be read at startup. Layout: a first-surface `--danger` notice: "The app's memory of your last scan couldn't be opened." Reassurance: "Your audiobooks are untouched - this is only the app's own notes." Primary action: "Start a fresh scan" (rebuilds from disk). Tokens: `--danger`.

Permission denied. Trigger: a target path cannot be accessed (Defender / Controlled Folder Access, FD-19). Layout: `--danger` surface naming the location in a friendly name, with a "Show file details" disclosure for the raw path and a linked how-to. Primary action: context-dependent ("Try again" after the user grants access). Tokens: `--danger`.

### 5.2 Empty and edge states

Nothing-to-do library (zero changes). Layout: review/home shows a warm, positive state: "Your library is already in good shape - nothing to change." A `--good` check glyph, no group cards. Primary action: "Back to the library". Not an error; a success.

Empty library root. Trigger: the chosen root has no audio. Layout: a gentle first-run-adjacent state: "This folder doesn't have any audiobooks yet." Primary action: "Choose a different folder" (opens the picker, F-909). Tokens: neutral `--ink-2`.

All-groups-excluded. Trigger: the user has toggled every group off in review. Layout: the footer count reads "0 of 7 groups - 0 changes"; the primary "Organize now" button is DISABLED with an explanatory line beside it: "Turn on at least one group to organize." Tokens: neutral; disabled button uses the standard disabled treatment. The primary action is present but explicitly disabled with a reason (Section 7).

No duplicates found. Layout: the Duplicates screen shows "No duplicate copies found - every book is unique." `--good` glyph, empty list. Nav badge reads 0 groups (FD-08).

### 5.3 Loading states

Plan-building (between scan and review). This is a DISTINCT state, not reuse of the scan screen. Layout: the `.centercol` progress pattern with copy "Building the plan" and a `--ink-2` sub-line "Working out the safest set of changes." A Stop control is present. This closes the gap the prototypes leave (scan jumps straight to review).

Re-scan progress (from home). Layout: the scan progress pattern (4.16) reachable from the home "Scan again" action, with the friendly-location status line (FD-13) and a real Stop control.

### 5.4 Stop and pause on progress (FD-02)

Every progress screen (scan, plan-building, organize) carries a real Stop control: cooperative cancel at safe boundaries only (F-104 (job progress + cancel) semantics; never mid-file-move). The organize screen additionally carries Pause (F-608), which takes effect between books only and leaves the undo record intact. The prototype's "Pause between books" button is F-608; the prototype's "Skip ahead" is demo-only and never ships.

---

## 6. Copy register (normative)

All user-facing copy is centralized in one strings module (FD-23 (localization: English-only v1, centralized strings)) so later localization is possible; the plain-language vocabulary is part of this design system. English-only in v1.

### 6.1 Vocabulary map

| Use this | Not this |
|---|---|
| organize | operation, op, job (in UI) |
| changes | operations, mutations |
| groups | batches |
| move to the Archive | delete, remove, trash, quarantine, set aside |
| copies (within a group) | duplicates-as-files, pairs |
| held, checking, needs you | blocked, error (for soft states) |
| library home, your library | dashboard |

**Corrected 2026-08-16.** This document had drifted 34 instances behind the
vocabulary it defines, including the first design principle in Section 1, which
read "Books, shelves, copies, tidy-up" and named three retired terms in the
sentence that sets the register. `FD-46` renamed the group to Duplicates,
`FD-47` replaced shelves with library, and `FD-48` retired the whole tidy family
for organize. A design system that teaches the wrong words is worse than no
design system, because it is quotable.

**What the word "shelf" still legitimately names here, and why it stays.**
`FD-47` retired "shelves" as the word for WHERE BOOKS LIVE; that is the library.
It did not rename the shelf-row component, its CSS (`--shelf-rail`,
`.shelfhead`), or the bookshelf-edge visual metaphor those produce, any more
than `FD-48` renamed `ensure_forward_tidying_allowed`. The rule is the same one
every vocabulary decision here follows: copy a user reads moves, engineering and
component names do not. Sentences a user reads have been changed; identifiers
and the metaphor have not.

**Not yet gated by CI, deliberately.** `STATUS.md` and `CHANGELOG.md` joined the
governance vocabulary gate on 2026-08-15, and this file did not. The gate strips
quoted spans to tell a mention from a use, which is not enough here: `--shelf-rail`
and "home shelf" are legitimate and would fail it. Gating this file needs the
pattern to understand the component-versus-copy distinction described above,
which is a larger change than the sweep it would protect. Recorded so the gap is
a known one rather than an oversight.
| Show file details | show paths, show journal, show logs |

Forbidden on primary surfaces: operations, ops, dedupe, manifest, quarantine, dashboard (plain-language register per the suite's standing rules; D-06). "quarantine" is an engine/internal term only; the UI says "set aside".

### 6.2 Numbers inside sentences

Counts and sizes live inside sentences, not in stat bands or KPI tiles (D-06). Example (home lede, 04): "**1,022 audiobooks**, about 297 GB. Most are already in good shape for Audiobookshelf. **412** could use organizing." Any GB figure states which quantity it refers to (FD-08): "10.1 GB across 403 duplicate groups", not a bare "10.1 GB".

### 6.3 Duplicates unit canon (FD-08 (duplicates canonical unit))

The canonical unit is the GROUP: one book, N identical copies. Nav badge, headlines, and the report all count GROUPS; member files are "copies". Fix the prototype's mixed "403 copies / pairs / groups" language (05, 06, 07 vary): say "403 duplicate groups" for the count of books-with-duplicates, and "copies" for the member files within a group. State which quantity any GB figure refers to.

### 6.4 Reassurance canon and the deletion guarantee (FD-10 (deletion guarantee canon copy))

The deletion guarantee, used verbatim wherever the guarantee appears (report, review footer, done screen):

> No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.

This exact register replaces the prototypes' overclaim "no delete anywhere" (which conflicts with the 20 empty-folder removals, planning audit stream 2 item 7). Primary quarantine vocabulary stays "set aside". Negated "deleted" wording is allowed ONLY in reassurance contexts that enumerate this guarantee (e.g. the done-screen "Books deleted - 0, as always" delta row); everywhere else the vocabulary is "set aside" (FD-10, planning audit stream 2 item 21).

Standing reassurance lines (from 04, 05, 07), sanctioned: "Nothing is deleted. Every change can be undone." (footer); "Organizing never deletes anything. Duplicate copies are set aside, and every change can be undone." (home); "Everything stays on this computer." (nav / provenance).

### 6.5 Genre replacement copy (FD-12 (genre replacement copy))

The prototype done-screen claim "The old genre view lives on as tags" is REMOVED: it promised tag writing, a non-goal. Replacement copy: genre folders are not carried into the new author-first layout; pack and award membership is preserved in the provenance report (F-507 (pack provenance capture and report)). No v1 copy may promise any ABS-side change or tag write (FD-12, D-14 (provenance in v1)).

### 6.6 "Needs you" register for manual review

Books that route to manual review (unclear names, video/course content per FD-17, ambiguous duplicates) are described in the "needs you" register, never as errors: "One unclear file is left for you to look at.", "The War of Art has a v1 and a v2 that would land in the same folder; pick a keeper, or keep both as editions." Tone: the tool defers to the person, it did not fail.

### 6.7 Campaign groups canon (FD-26 (seven campaign groups))

Seven user-facing groups, exactly as prototyped in 05/07, and the review UI and report agree on count and labels:

1. Move the sorting piles out of the library (staging)
2. Give loose books their own folders (loose)
3. Clean up messy folder names (names)
4. Split box-set folders into separate books (box sets)
5. Unpack books out of collection bundles (bundles)
6. Move duplicate copies to the Archive (duplicates)
7. Sweep out empty folders (empties)

Series-index normalization folds into "messy names" for the UI while remaining a distinct internal plan pass (FD-26). The internal group list of F-403 (plan builder) maps onto these seven.

---

## 7. Interaction patterns (AC-ready)

Written as testable statements for F-908 / the v0.4.0 surfaces.

- The theme control is a two-button segmented control; the active theme's button has `aria-pressed="true"` and the other `false`; toggling updates `data-theme` on the root and persists via F-803 (app settings).
- Group include/skip toggles are `role="switch"` with `aria-checked` reflecting state; a held group's switch is `disabled` with a visible reason.
- Group cards are keyboard-selectable: `tabindex="0"`, `aria-pressed` reflects selection (corrected from `aria-selected`, v0.4.0 Phase 8 T-36; see Section 4.9), Enter selects the card and updates the detail pane.
- All interactive controls show a visible `:focus-visible` outline: 2px solid `--primary`, offset 2px.
- `prefers-reduced-motion:reduce` collapses all transitions and animations to ~0ms (crossfade or instant alternatives); the theme-change and progress-bar transitions honor it.
- Confirming a run is a two-step inline confirm strip in the footer (4.14), NEVER a modal dialog.
- Exactly one calm primary action per screen. Where the only reasonable primary action is unavailable (all-groups-excluded, 5.2), the primary button is present but DISABLED with an adjacent explanatory line, not hidden.
- Every progress screen (scan, plan-building, organize) has a real Stop control (cooperative cancel at safe boundaries, F-104); the organize screen also has Pause (F-608). "Skip ahead" is demo-only and never ships (FD-02).
- Toasts are for transient confirmations only and auto-dismiss; anything requiring a decision is a surface, not a toast.

---

## 8. Accessibility (WCAG AA, verified not promised - FD-21)

WCAG AA (4.5:1 body text) in both themes is verified, not merely asserted (FD-21 (accessibility verification method)). Verification method, carried as design-system requirements:

1. Mechanical contrast check of ALL token pairs in both themes, run as a script and in CI from v0.4.0. The table below shows computed values for the load-bearing pairs; the CI script covers the full matrix.
2. axe-core smoke test in Vitest on primary surfaces.
3. A keyboard-walkthrough item in the per-release manual QA checklist.

Computed contrast (sRGB relative luminance, rounded):

| Pair | Theme | Ratio | AA body (4.5:1) |
|---|---|---|---|
| `--ink` on `--bg` | Day | ~14:1 | pass |
| `--ink-2` on `--bg` | Day | ~8.9:1 | pass |
| `--ink-3` on `--bg` | Day | ~4.95:1 | pass |
| `--ink-3` on `--surface-2` | Day | ~4.5:1 | borderline pass |
| `--primary-ink` on `--primary` | Day | ~8.0:1 | pass |
| `--danger` on `--surface` | Day | ~6.5:1 | pass |
| `--ink-2` on `--bg` | Evening | ~9:1 | pass |
| `--ink-3` on `--bg` | Evening | ~5.8:1 | pass |
| `--ink-3` on `--surface` | Evening | ~5.3:1 | pass |
| `--primary-ink` on `--primary` | Evening | ~5.2:1 | pass |
| `--primary-ink` on `--primary-hover` | Evening | ~4.64:1 | pass (corrected, see note) |
| `--danger` on `--bg` | Evening | ~6.8:1 | pass |
| `--danger` on `--surface` | Evening | ~6.1:1 | pass |

v0.4.0 Phase 8 correction (T-35): the prototype's Evening `--primary-hover`
(`#7a6ce0`) measured 4.16:1 against `--primary-ink`, a real AA failure the
mechanical contrast script (`scripts/check-contrast.mjs`) caught on its first
run. Darkened slightly to `#7264d8` (4.64:1), keeping the Evening theme's
deliberate lighter-on-hover direction (the opposite of Day's darken-on-hover)
rather than reversing it.

`--ink-3` restriction rule (FD-21, planning audit stream 2 item 19): `--ink-3` on `--surface-2` in Day is borderline (~4.5:1). Therefore `--ink-3` is restricted to decorative or non-information content (section sub-lines that repeat visible information, rail chrome). Where `--ink-3` conveys information a reader must act on, it is darkened (Day) or lightened (Evening) to clear 4.5:1 against its actual background, or promoted to `--ink-2`. Count badges (`.n`, Section 4.3) and `.meta` counts (Section 4.9) are information-bearing: `--ink-3` is permitted for them only on token-pair surfaces measured at or above 4.5:1 in the table above, and on any other surface they are promoted to `--ink-2`.

Additional requirements:

- Status is never color-alone: every status chip, reason chip, and state surface pairs color with an icon AND a text label (`.why`, `.stag`, `.flag`, the Section 5 surfaces). This lets `--warn` (held) and `--danger` (failure) be distinguished without relying on hue.
- All interactive controls are keyboard-reachable with a visible focus ring (Section 7).
- The `--danger` pair (Section 2.3) meets AA in both themes so error surfaces are legible for the tier-2 reader.

---

## 9. Sample-data rule and path-exception rule

### 9.1 Sample-data rule (FD-27 (sample-data rule))

Every number in the prototypes is SAMPLE data. Specs and implementation NEVER hardcode prototype numbers (1,022 vs 994 on home, 1,385, 982, 403, 238, 203, etc.); real targets derive from the discovery baselines (FD-18 (2026-03-25 baselines)). The prototypes are internally inconsistent on purpose-of-illustration (e.g. 04 home shows 1,022 while 07 uses different figures, planning audit stream 2 item 18). Any surface or doc citing library figures labels them "2026-03-25 baseline, pending fresh scan". The internally consistent demo arithmetic (1,385 = 4 + 238 + 203 + 31 + 486 + 403 + 20; 982 applied when the 403-group copies bundle is excluded) may be reused for illustration only, always labeled sample.

### 9.2 Path-exception rule (FD-13)

Raw filesystem paths do not appear on primary surfaces, with exactly two sanctioned exceptions:

1. Inside the "Show file details" disclosure (4.12), for tier 1, where paths appear alongside the matched pattern and confidence (F-504).
2. The Audiobookshelf setup path on the done-screen next-step card (4.18), because tier 2 needs it to configure ABS.

Everywhere else, including the scanning and organizing status line, a friendly location name is used, never a raw path (FD-13, planning audit stream 2 item 17). The frontend never touches the filesystem directly; folder access is via `tauri-plugin-dialog` and all paths originate from the backend (FD-29 (Tauri capability/security model), F-909).

---

## 10. Traceability

| This document defines | Consumed by |
|---|---|
| Theme tokens, danger pair (Sec 2), typography (Sec 3) | v0.4.0 spec; ci-plan (contrast script, zero-network grep) |
| Component inventory (Sec 4) | v0.4.0 spec (F-902 library home, F-903 plan preview surface); F-907 fallback tile |
| Error/empty/loading states (Sec 5) | F-908 spec AC; F-608 (pause/resume), F-104 (stop) |
| Copy register (Sec 6) | strings module (FD-23); F-506 report spec (FD-10, FD-28); all surfaces |
| Interaction patterns (Sec 7) | v0.4.0 spec AC |
| Accessibility method (Sec 8) | FD-21 CI contrast script (from v0.4.0); manual QA checklist |
| Sample-data and path rules (Sec 9) | all specs (FD-27, FD-13) |
