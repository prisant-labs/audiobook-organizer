# audiobook-organizer

A local-first Windows desktop utility that scans a messy audiobook library, explains what is untidy in plain language, proposes a reorganization as a reviewable dry run, and applies it safely with full undo. It complements Audiobookshelf; it never replaces it. A dry run is not a mode buried behind a flag, it is the product: a browsable confirmation screen in the app plus an exportable, self-contained HTML report that reads on any machine before a single file moves.

## How it works

Every campaign follows the same three steps, in the same order, every time:

1. **Scan.** The app reads a chosen library folder and classifies what it finds: tidy books, loose files, messy names, box sets, bundles, duplicate copies, and empty folders left behind by other tools.
2. **Review.** A plan is built and shown as a set of grouped, curated cards, plus a full, exportable HTML report anyone can read without opening the app. Nothing has moved yet.
3. **Tidy up.** Only after a person confirms the plan does anything execute, one journaled step at a time, with a real stop control and full undo.

## Status

Planning complete, pre-v0.1.0. This repository currently holds the product contract, the decision record, and the release-by-release plan. No engine or GUI code has landed yet; the build starts at the v0.1.0 spine release. See the doc map below for what exists today.

## The safety promise

No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.

Every apply is journaled before it acts, duplicates are set aside rather than removed, and the same validate, preview, and apply pipeline that runs a tidy-up also runs its own rollback.

## Doc map

- [PRODUCT.md](PRODUCT.md): the design contract. Users, purpose, brand personality, anti-references, and the design principles every surface must satisfy. Authoritative for look, tone, and product intent.
- `docs/internal/product-requirements.md`: the PRD, features, epics, IPC surface, schema, and error taxonomy.
- `docs/internal/architecture.md`: the technical architecture, adapted from the repo-sync-tool reference architecture.
- `docs/internal/program-roadmap.md`: the release ladder from v0.1.0 through v0.9.0, with gates and descope triggers.
- [EXECUTION.md](EXECUTION.md): governance, branching, CI shape, and the human-only approval gates.
- `docs/internal/releases/`: one tracked folder per release, each with its own spec and implementation plan.
- `docs/internal/decisions/`: architecture decision records (MADR v4) explaining why, not just what.

## Stack

Tauri v2, Rust, React, TypeScript, shadcn/ui, and SQLite via sqlx and tauri-specta, locked to the repo-sync-tool common stack so the two projects share one architecture and one CI shape.

## Look and feel

Calm, bookish, trustworthy. Two sanctioned moods of one design system ship as themes, Day and Evening, not as separate designs. Covers and spines carry the visual warmth; the app chrome around them stays quiet. See `PRODUCT.md` for the brand personality and anti-references, and `docs/internal/design-system.md` (once authored) for tokens and components.

## Who this is for

Three tiers, all real: a technical owner who runs campaigns and wants full detail on request; household members who should never see a file path, an exit code, or a jargon term as the primary interface; and, eventually, public users with their own messy libraries and their own copy of Audiobookshelf. See `PRODUCT.md` for the full design contract that this bar produces.

## What exists today

- `PRODUCT.md`, the design contract.
- `EXECUTION.md`, governance and CI shape.
- Ratified decision records in `docs/internal/decisions/`.
- The PRD, architecture doc, and program roadmap in `docs/internal/`.
- Per-release spec and implementation-plan folders in `docs/internal/releases/`, filled in as each release is planned in turn.

Nothing here is a promise about a shipping date; it is the map the build follows once it starts.

## Contributing

This repository is private during the planning and early build phases. There is no external contribution process yet; that decision, along with the license, is made at the public flip (see `docs/internal/program-roadmap.md`).

## License

License: to be decided before any public release.
