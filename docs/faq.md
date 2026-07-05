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

## Where did the name a non-technical member of the household in the design documents come from?

From the library itself. The March 2026 discovery scan of `E:\Books - Audio` found an empty Audible profile folder, `_audible\[us]a non-technical member of the household`, and the discovery analysis inferred household usage from it. When the product decisions were ratified on 2026-07-03 (audience = all tiers, with the family tier setting the UI bar), PRODUCT.md encoded the design bar as: if a non-technical member of the household could not confidently review and confirm a tidy-up, the surface is wrong. The PRD quotes it as the tier-2 standard.

The user guide deliberately does not use the name; it says "everyone in the house." Privacy note on record: PRODUCT.md and the PRD do carry the name, and the repo is written public-ready for a possible flip at v0.9.0 (D-13). If family names should stay out of a potentially public repo, the references generalize to "a household member" in a two-minute change; the decision is the operator's.

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
