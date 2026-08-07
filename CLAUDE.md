# Claude Code Instructions: audiobook-organizer

This file is the Claude-specific overlay. `AGENTS.md` at repo root carries the tool-agnostic version of these rules; read this one first when you are Claude Code.

## Read first

Before any planning or code work, read in this order:

1. `PRODUCT.md`: the design contract. Authoritative for users, purpose, tone, and design principles.
2. `EXECUTION.md`: governance, branching model, CI shape, and the human-only approval gates.
3. `docs/internal/product-requirements.md`: features, epics, IPC surface, schema, error taxonomy.
4. `docs/internal/program-roadmap.md`: the release ladder, gates, and descope triggers.
5. The current release folder under `docs/internal/releases/<version>-<codename>/`: its `spec.md` and `implementation-plan.md` are the acceptance criteria and the task breakdown for whatever you are building right now.

## Standing rules

- Never use em-dashes (U+2014) or en-dashes (U+2013) anywhere: chat, docs, code, commits. Use " - ", a comma, a colon, or a sentence break. Numeric ranges use plain hyphens (2-5).
- Every reference ID carries its handle on first use per section: "F-506 (dry-run HTML report)", "D-14 (provenance in v1)", never a bare ID.
- Plain-language register in all user-facing copy: books, library, duplicates, tidy-up, Archive. Never operations, ops, dedupe, manifest, or dashboard in anything a user sees. (Revised by FD-47: "shelves" retired for "library", "copies" for "duplicates" per FD-46, "set aside" for "Archive" per FD-42.)
- Branch-first: work happens on short-lived feature branches off `main`, per `EXECUTION.md`. Do not commit directly to `main`.
- Acceptance criteria live only in release specs (`docs/internal/releases/<version>/spec.md`). The roadmap and release plans aggregate and reference AC; they never author it.
- Human-only gates (never cross without explicit approval): any Real, non-dry-run apply against the actual library; publishing releases or tags; the public-repo flip; spending money; rewriting history. See `EXECUTION.md` for the full allowlist.

## UI work

Any surface, component, or copy change touching the GUI must conform to the design system document (once authored, at `docs/internal/design-system.md`) as well as `PRODUCT.md`'s anti-references and design principles. When the two seem to disagree, `PRODUCT.md` wins.

## Model tiering

FD-30 (model-tiering execution policy) governs agent assignment on this project: the highest-capability model does top-level planning, synthesis, gate reviews, and final verification. Safety-critical implementation (the executor, journal and rollback, validation) and complex authorship get a strong reasoning subagent. Mechanical work (boilerplate, formatting, table-driven tests, doc conversions) goes to a lighter subagent. See `EXECUTION.md` for the concrete assignment.

## Local-only material

`_local/` (and any `_LOCAL/`) is reference-only scratch: prototypes, discovery notes, prior-work rescues, planning drafts. It is gitignored and never committed. Treat it as input to read, not a place to write anything that needs to survive review.

## Windows-first

This is a Windows 11 desktop product; write examples with Windows paths. macOS support is compiles-in-CI honesty only, not a design target, unless a decision record says otherwise.

## Vocabulary discipline

The product-facing vocabulary is deliberate and narrow: books, library, duplicates, tidy up, Archive, undo. Internal engineering terms (operations, ops, dedupe, manifest, journal, quarantine, dashboard) belong in code, ADRs, and `docs/internal/`, never in a screen, dialog, toast, or exported report a user reads. If a spec or a prototype uses one of these terms on a user-facing surface, that is a defect to flag, not a pattern to copy.

## When sources disagree

If an instruction here, in a spec, or in a prototype conflicts with `PRODUCT.md` or with a ratified decision (`D-nn` or `FD-nn` in a release's spec or the roadmap), the decision record wins. Do not silently pick a side; note the conflict and ask, unless the conflict is already resolved in a decision record you can cite.

## Commits and PRs

Use conventional commit prefixes (`feat:`, `fix:`, `docs:`, `chore:`). Never commit secrets, credentials, or `.env` files. Prefer editing an existing file over creating a new one unless the release plan calls for a new file.
