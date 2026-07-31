# audiobook-organizer

A local-first Windows desktop utility that scans a messy audiobook library, explains what is untidy in plain language, and proposes a reorganization as a reviewable dry run. It complements Audiobookshelf; it never replaces it. A dry run is not a mode buried behind a flag, it is the product: a browsable confirmation screen in the app plus an exportable, self-contained HTML report that reads on any machine before a single file moves.

**This is an in-progress build, not a finished tool.** The section below is deliberately specific about which parts you can use today and which are still aspiration. Please read it before pointing this at a library you care about.

## What works today, and what does not

The repository is at two maturity levels at once. The read-and-rehearse path is a working alpha. The make-real-changes path is not yet a complete loop a normal person should operate.

### Works today

- **Scan.** Reads a chosen library folder and classifies what it finds: tidy books, loose files, messy names, box sets, bundles, duplicate copies, and empty folders left behind by other tools. Read-only; it writes nothing.
- **Review.** Builds a plan and shows it as grouped, curated cards with per-group and per-operation control over what is included.
- **The dry-run report.** A full, exportable, self-contained HTML report that opens on any machine, with no network access of any kind.
- **Rehearsal.** Runs the whole plan through the *same* executor a real change would use, against an in-memory filesystem. This is what makes the preview faithful rather than a guess.
- **History and undo preparation.** Every past run is listed with what it did and the one honest action available for it. Preparing an undo builds a real reverse plan you review before anything moves.
- **Settings, first run, and the rule editor.**

### Not yet, despite what the engine can do

- **Real changes are not reachable from the app.** The UI pins every run to rehearsal. The engine can execute against the real filesystem and is extensively tested, but there is deliberately no button that reaches it, and there will not be one until the items below are closed. Treat the app as a rehearsal tool.
- **Cross-volume moves verify by size, not content.** Moving between drives copies, compares byte length, then deletes the source. Equal length is not equal content. Content hashing before source removal is required work, not a nice-to-have, and until it lands a real cross-volume move is not safe to offer.
- **Power-loss durability is an open decision.** The journal is written before each action, which survives a process kill and is what interruption recovery is built on. It is not proven to survive a power cut between the write and the action. The threat model needs to be stated and, for real changes, probably strengthened.
- **The Duplicates screen is not built.** Duplicate detection exists in the engine; the screen is a placeholder.
- **There are no releases, tags, or installers.** Nothing has been packaged. You build it from source.
- **macOS builds compile but are not safe for real changes.** The no-overwrite guarantee that makes moves safe is enforced with a Windows-specific API. The macOS path has a known race. Do not use a macOS build for anything but compilation checks.

### The safety promise, stated precisely

No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.

That promise is enforced where it can be: every change is journaled before it acts, moves never overwrite an existing file, duplicates are set aside rather than removed, and a rollback runs through the same validate-preview-apply pipeline as a forward change. The honest caveats are the two above (cross-volume content verification and power-loss durability), both of which are open before any real-library use.

## How it works

Every campaign follows the same three steps, in the same order, every time:

1. **Scan.** The app reads a chosen library folder and classifies what it finds.
2. **Review.** A plan is built and shown as grouped, curated cards, plus the exportable HTML report. Nothing has moved yet.
3. **Tidy up.** Only after a person confirms the plan does anything execute, one journaled step at a time, with a real stop control. Today this step runs as a rehearsal only.

## Status

Active build. Releases v0.1.0 (spine) through v0.5.0 (acting) are implemented and merged to `main`: the live scanner and classifier, field parsing and name normalization, the plan builder and validator, the dry-run HTML report, the GUI review app, and the executor with journaled undo and post-apply verification. v0.6.0 (hardening) is in progress: interruption recovery and the History and undo surface.

No version is tagged and no release has been published; tagging is a human-only gate.

To run the app locally, see [RUNNING.md](RUNNING.md).

## Stack

Tauri v2, Rust, React, TypeScript, shadcn/ui, and SQLite via sqlx and tauri-specta, locked to the repo-sync-tool common stack so the two projects share one architecture and one CI shape.

The engine lives in `crates/abo-core` and is Tauri-free by contract, enforced in CI: all classification, parsing, plan building, validation, execution, journaling, and recovery is testable without a GUI.

## Look and feel

Calm, bookish, trustworthy. Two sanctioned moods of one design system ship as themes, Day and Evening, not as separate designs. Covers and spines carry the visual warmth; the app chrome around them stays quiet. See `PRODUCT.md` for the brand personality and anti-references, and `docs/internal/design-system.md` for tokens and components.

## Who this is for

Three tiers, all real: a technical owner who runs campaigns and wants full detail on request; household members who should never see a file path, an exit code, or a jargon term as the primary interface; and, eventually, public users with their own messy libraries and their own copy of Audiobookshelf. See `PRODUCT.md` for the full design contract that this bar produces.

## Doc map

- [PRODUCT.md](PRODUCT.md): the design contract. Users, purpose, brand personality, anti-references, and the design principles every surface must satisfy. Authoritative for look, tone, and product intent.
- [EXECUTION.md](EXECUTION.md): governance, branching, CI shape, and the human-only approval gates.
- `docs/internal/product-requirements.md`: the PRD, features, epics, IPC surface, schema, and error taxonomy.
- `docs/internal/architecture.md`: the technical architecture, adapted from the repo-sync-tool reference architecture.
- `docs/internal/program-roadmap.md`: the release ladder from v0.1.0 through v0.9.0, with gates and descope triggers.
- `docs/internal/design-system.md`: tokens, components, and the copy register.
- `docs/internal/releases/`: one tracked folder per release, each with its own spec and implementation plan.
- `docs/internal/decisions/`: architecture decision records (MADR v4) explaining why, not just what.

## Contributing

This repository is private during the planning and early build phases. There is no external contribution process yet; that decision, along with the license, is made at the public flip (see `docs/internal/program-roadmap.md`).

## License

License: to be decided before any public release. The `Cargo.toml` and `package.json` metadata is not yet authoritative and is reconciled as part of that decision.
