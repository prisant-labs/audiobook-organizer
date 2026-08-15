# Product

## Register

product

## Users

Three tiers, all real: (1) jp, a technical product person on Windows 11 who runs the campaigns; (2) household members (non-engineers) who may organize the library or review a plan, and who must never be shown a file path, exit code, or jargon term as the primary interface; (3) eventually public open-source users with messy audiobook libraries and Audiobookshelf. The design bar is set by tier 2: if a non-technical member of the household could not confidently review and confirm the changes, the surface is wrong. Technical truth (paths, operation detail) stays available behind explicit disclosure for tier 1.

Context of use: at a desk or in an evening chair, occasionally (campaigns and intake batches, not daily), with real anxiety about a tool that moves hundreds of gigabytes of personally collected audiobooks.

## Product Purpose

A local-first Windows (later macOS) desktop utility that scans an audiobook library, explains what is messy in plain language, proposes a reorganization as a reviewable dry run, and applies it safely with full undo. It complements Audiobookshelf, never replaces it. Success: a user runs a dry run, reads the report, presses one button, and their library imports cleanly into Audiobookshelf, with nothing deleted and everything reversible.

The dry run is a first-class product, not a mode: it produces a browsable confirmation screen in the app and an exportable, self-contained HTML report that can be read anywhere before anything moves.

## Brand Personality

Calm, bookish, trustworthy. The tool feels like caring for a collection, not operating machinery. Two sanctioned moods of one system: a warm, cover-forward "evening library" and a quiet, neutral "daytime utility" (shipped as themes, not separate designs). Copy is plain, short, and reassuring; numbers live inside sentences; the interface never brags about its own engineering.

## Anti-references

- The AI-generated dashboard look: hero-metric stat bands, uppercase tracked eyebrow labels on every section, editorial-serif-plus-noise-texture staging, identical card grids, glassmorphism. (The first prototype set in `_local/gui/01-03` reads this way; it is the recorded anti-reference.)
- Dev-tool aesthetics for primary surfaces: monospace-everywhere, journals, SHAs, "operations" vocabulary in front of non-engineers.
- Heavy Electron-app chrome and busy multi-panel density (the tool is occasional-use; it should feel light).

## Design Principles

1. **Plain language first.** Books, library, duplicates, organize, Archive. Never operations, ops, dedupe, manifest in a primary surface. Technical detail exists behind one consistent "show file details" disclosure. The action has no noun form, so where one is needed the copy says "the plan", "the changes", or "run" (FD-48, superseding FD-43's "tidy up").
2. **Preview before touch.** Every destructive-adjacent flow runs scan -> review -> confirm. The review screen and the HTML report are the product's trust ceremony.
3. **Nothing irreversible.** No deletions exist in the UI vocabulary. Duplicates are "moved to the Archive," changes are "undoable," and the interface says so where the anxiety happens.
4. **The books are the interface.** Covers and spines carry the visual warmth and recognition; app chrome stays quiet and neutral so the collection is the color.
5. **One calm primary action per screen.** Every surface answers "what should I do next" with exactly one obvious button.

## Accessibility & Inclusion

WCAG AA contrast (4.5:1 body text) in both themes. Status is never color-alone: icon plus label always. Respect `prefers-reduced-motion` (crossfade or instant alternatives). Keyboard-reachable controls and visible focus states. Language readable by a non-technical adult with no file-system mental model.
