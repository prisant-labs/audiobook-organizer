---
title: Audiobook Organizer - FAQ
date: 2026-07-05
status: living document
owner: jprisant
---

# FAQ

Questions asked of the project, answered from what is actually built and ratified. Companion documents: the [user guide](user-guide.md) (family-facing), the [functionality breakdown](internal/functionality.md) (technical), and the [decision ledger](internal/decision-ledger.md) (every D-nn and FD-nn cited here).

## Does the tool use any third-party database for book names?

No. Zero third-party databases, by deliberate design. Every title, author, series, and year comes from two local sources only: your own folder and file names (the nine-pattern parser leads), plus a one-time, read-only check of the labels inside a 300-file sample of the audio files, which existed purely to validate that folder names are the more reliable source. They won decisively: 100% coverage versus 85% (title) and 96% (author) for embedded tags, with only 31.8% author agreement where both exist.

No Audible, Goodreads, or Open Library. No web search. No telemetry. This is enforced, not just promised: the CI build fails on any external reference (FD-11, the zero-network gate), and the tag-reading code sits behind an opt-in build flag absent from every shipped build. Online metadata lookup (F-1103) is a ratified non-goal from the original discovery consensus.

## Where did the household-member references in the design documents come from?

From the library itself. The March 2026 discovery scan of `E:\Books - Audio` found an empty Audible profile folder belonging to another member of the household, and the discovery analysis inferred household usage from it. When the product decisions were ratified on 2026-07-03 (audience = all tiers, with the family tier setting the UI bar), PRODUCT.md encoded the design bar as: if a non-technical household member could not confidently review and confirm a tidy-up, the surface is wrong. The PRD quotes it as the tier-2 standard.

The user guide says "everyone in the house." Privacy note on record: earlier revisions of these documents named the household member; the references were generalized on 2026-07-20 (the operator's recorded decision) so no family name appears in the repo.

## What would adding tag writing (labels inside the files) involve?

Very feasible, and the project's own data says writing is the valuable direction. The FD-14 probe showed the library's embedded tags are incomplete and frequently disagree with folder names, so reading tags adds little, but writing the folder-derived truth into the files would make every book self-describing in any player.

What it takes:

1. **One ratified product decision.** Tag writing (F-1106) is currently an explicit non-goal from discovery, and the user guide promises "never rewrite the labels inside them." Superseding that needs a new D-number from the operator, plus a revision of that promise.
2. **The real engineering substance is undo.** Everything built so far reverses because moves and renames are inherently reversible. A tag write changes file contents, so the journal must capture before-values (every original tag frame) per file to keep a tidy-up that includes retags fully undoable. That is the one genuinely new safety mechanism; it also interacts with duplicate detection, since file hashes change by design.
3. **The rest slots into existing seams.** lofty (already pinned in-tree, feature-gated) handles writing; a new retag operation kind flows through the same plan, validate, preview, apply, journal pipeline; it appears as an eighth group in the review and the report ("Label the files - write each book's name inside it so any player shows it correctly").

Sizing: roughly one release-sized effort. Natural home is v1.1, exactly where the roadmap already parks F-1101 (embedded tag reader) and F-1106 (tag writing); about one to two working sessions at the current cadence.

## Could it become a local audiobook player?

The honest product take first: "not a media player, complements Audiobookshelf, never replaces it" is ratified product identity (PRODUCT.md; Section 10 non-goals). ABS already does playback, progress sync, and phone apps well, so a full player inside the organizer would compete with the thing it exists to serve. Pivoting that is allowed, but it is a v1.0-scale decision, not a bolt-on.

Technical reality if pursued: playback is tractable on this stack. WebView2 decodes m4b, m4a, and mp3 natively, so audio would ride the webview via a scoped asset protocol rather than Rust-side decoding. But a good player (m4b chapter support, per-book resume positions, multi-file mp3 books played seamlessly, speed control, sleep timer, media keys) is honestly another product roughly the size of everything built so far: two to four release-sized efforts. It also reopens the security model, since the webview currently has zero filesystem access by design (FD-29).

The recommended middle path: an audition button in the review screens, playing thirty seconds of a book right where a decision is being made. It directly serves the tidy-up job (is this "version 2" actually the better recording?), costs days rather than months, keeps the ABS-complement identity intact, and is the first brick of a player if one is ever wanted.

## Can the user define preferences for the folder organization structure?

Yes. This is a built, tested capability at the engine level as of v0.3.0; the point-and-click editor arrives with the v0.4.0 GUI.

- **Three shipped layout presets** (F-401, naming templates): `abs-author-first`, the ratified default per D-02 (`Author\Series\Book 1 - Year - Title`); `title-first` (`Title - Author (Year)`, the operator's documented historical preference); and `hybrid-genre` (genre kept as a top-level shelf, ABS-native below). Template variables exist for Author, Title, Series, SeriesIndex (padding width configurable; default is unpadded "Book 1", matching the ratified prototypes), Year, Narrator, and Subtitle, with explicit missing-field fallbacks: no empty parentheses ever, and a book missing critical fields routes to "needs your eyes" rather than receiving a guessed name.
- **Structure policy toggles** (F-402), each with a safe default: one-book-per-folder enforcement; what happens to emptied bundle shells (set aside by default, leave-in-place available, FD-01); sidecars (covers, ebooks) travel with their book; per-file-type clutter handling; preferred format when a book exists in two formats (m4b by default); empty-folder sweeping; name cleanup on or off.
- **Everything bundles into named, saved rulesets** (F-801): versioned, strictly validated (unknown fields rejected), stored locally in SQLite. Changing a ruleset regenerates the plan; plans are immutable, so it is always "the plan under ruleset A" versus "under ruleset B", never a mutated plan. The v0.4.0 settings and ruleset editor (F-906) adds live re-plan counts as switches flip; portable ruleset files for sharing land in v0.6.0 (F-802).
- **The honest boundary:** v1 ships presets plus knobs, not a freeform template language for typing arbitrary patterns. The variables exist internally, so a custom template editor is a straightforward later addition if wanted; it would enter through a spec like everything else.

## Does the user pick source folders and an output folder where everything is copied?

Neither; the model is deliberately different. The user selects one library folder, and the tidy-up reorganizes it in place. Files are renamed and moved within the same drive, a metadata-only operation on Windows: instant per book, no bytes duplicated, no second copy of the library ever created. The "after" structure in the report materializes inside the selected library, not in a separate destination.

Why (D-08, rename-first executor): the real library is 297 GB on a drive at roughly 93% capacity. A copy-to-output model would demand another ~300 GB and hours of disk I/O per tidy-up and leave two libraries to reconcile. Renames cost seconds, are atomic per entry, and make full undo cheap, since reversing a rename is just another rename. The one full copy the drive can afford is reserved for the optional pre-tidy backup, which is the user's choice (D-17).

What the user actually selects (v0.4.0 first-run and settings, F-909 and F-803): the library folder (one root per tidy-up; multiple libraries is deferred, F-1107); where the Set Aside holding folder lives (defaults to beside the library, outside it so Audiobookshelf never scans it); and where Reports go.

Two honest exceptions where real copying occurs: a move that crosses drives becomes copy, verify, then remove, explicitly marked in the plan and checked against free space before it is allowed (F-404); and the backup itself, if a copy-based option is chosen.

## How do new audiobooks get added to an already-tidied library?

Two answers: one that works by design already, and one planned refinement awaiting an operator decision.

Built into the design: the pipeline is re-runnable, and repeatability is a ratified success criterion (the tool "prevents re-messification rather than performing a one-time miracle"). Drop new books anywhere in the library, re-scan (sub-second at this library's scale), and the new plan contains only the new mess: five new loose files produce a five-change plan with the same review, report, and undo ceremony. Once v0.5.0 lands the apply step, adding books is simply drop-then-small-tidy-up. Small plans are the intended steady state after the M-1 campaign.

The planned refinement, F-1105 (intake mode), the discovery docs' "anti-re-messification play": a designated Intake folder outside the scanned library, with a focused "file these new books" flow that classifies just the newcomers, proposes each one's shelf destination, and files them per-book with accept-or-fix. Two flavors: check-on-launch (recommended; fits the launch-when-needed identity, reuses nearly all existing machinery, roughly one release-sized effort) versus a watched always-on folder (a residency change to a tray app, a bigger identity shift). Either flavor must solve duplicate detection against the existing library (a new copy of an owned book flags as a copy across snapshots) and cross-volume arrivals (Intake on another drive means real copy-verify-remove, space-checked).

Status: deferred to v1.1 pending the operator's posture decision (strategy brief open question 4, one of the ledger's standing human-only items). The re-scan loop covers the need in the meantime.
### Would an `_unsorted` folder in the root help?

Yes, and it is already recognized: the classifier's staging rule explicitly matches `_unsorted` (alongside `_sort`, `_process`, `_incoming`, `_inbox`, `_staging`, `_new`, and `to sort` variants, case-insensitive, underscore optional). Any tidy-up treats it as a sorting pile: protected, never scrambled, shown as "Leave the sorting piles for a later step."

The nuance is parking versus filing. By deliberate design, no pass touches anything inside a sorting pile, so `_unsorted` parks books safely but the tidy-up will not shelve them from there. To have newcomers filed today, drop them loose in the root: the loose-books group folderizes them, messy-names cleans them, and the copies group checks them against the existing collection. Intake mode (F-1105) is what unites the two: the drop pile plus a "file these new books" flow that empties it onto the shelves with per-book review, with the pile ultimately living beside the library rather than inside it so Audiobookshelf never scans half-sorted books.
