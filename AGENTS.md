# audiobook-organizer

> A local-first Windows desktop utility that scans a messy audiobook library, explains what is wrong in plain language, proposes a reorganization as a reviewable dry run, and applies it safely with full undo. Complements Audiobookshelf; never replaces it.

This file follows the agents.md open standard: agent-neutral instructions for any AI agent working in this repository (Claude, Codex, and others). Claude Code also reads `CLAUDE.md` for Claude-specific overlays.

## Repository layout

- `PRODUCT.md`: the design contract (users, purpose, brand personality, design principles).
- `EXECUTION.md`: governance, branching, CI shape, human-only approval gates.
- `docs/internal/`: tracked project documentation, including `product-requirements.md`, `architecture.md`, `program-roadmap.md`, `decisions/` (MADR v4 ADRs), and `releases/<version>-<codename>/` (one folder per release, each with `spec.md` and `implementation-plan.md`).
- `crates/abo-core/`: the Rust engine crate (Tauri-free; zero tauri dependency in abo-core). `src-tauri/`: the Tauri v2 shell (commands, events, capability config). `src/`: the React/TypeScript frontend (Vite, shadcn/ui).
- `_local/`: reference-only local scratch (prototypes, discovery notes, prior-work rescues). Gitignored, never committed, read-only input for planning.
- `_local/_agent-context/session-log/`: chronological session logs from all agents. Gitignored with the rest of `_local/` (session logs carry personal context and stay out of the repo; a root-level `_agent-context/` gitignore guard catches tooling that writes to the old path).

## Build and test commands

Stack: Tauri v2, Rust, React, TypeScript, shadcn/ui, SQLite via sqlx, tauri-specta. Releases v0.1.0 through v0.5.0 are built and merged to main. Common commands: `cargo build --workspace` (build engine and shell), `cargo test --workspace` (run all tests), `pnpm tauri dev` (dev server + hot-reload on Windows), `pnpm tauri build` (release installer). See `docs/internal/releases/` for per-release details.

## Conventions

- Never use em-dashes (U+2014) or en-dashes (U+2013) anywhere: docs, code, commits. Use " - ", a comma, a colon, or a sentence break. Numeric ranges use plain hyphens (2-5).
- Every reference ID carries its handle on first use per section: "F-506 (dry-run HTML report)", not a bare ID.
- Plain-language register in all user-facing copy: books, library, duplicates, organize, Archive. Never operations, ops, dedupe, manifest, or dashboard in anything a user sees. (Revised by FD-47: "shelves" retired for "library", "copies" for "duplicates" per FD-46, "set aside" for "Archive" per FD-42, and the whole "tidy" family for "organize" per FD-48.)
- Branch-first: short-lived feature branches off `main`, opened as PRs, per `EXECUTION.md`. Do not commit directly to `main`.
- Acceptance criteria live only in release specs (`docs/internal/releases/<version>/spec.md`). Roadmap and release plans aggregate and reference AC; they never author it.
- Use conventional commits (`feat:`, `fix:`, `docs:`, `chore:`).
- Record architectural decisions as MADR v4 ADRs in `docs/internal/decisions/`.
- See `CLAUDE.md` for Claude Code-specific rules and model-tiering assignment.

## Safety invariants

These are non-negotiable product invariants, not suggestions:

- Quarantine-only: no audio file is ever deleted anywhere in the product. Only empty folders are removed, and every change can be undone.
- Journal-before-act: every apply operation is written to a journal before it executes.
- Single-writer rule: exactly one apply job may run process-wide at a time.
- Vfs seam: a dry run is the same executor running against an in-memory filesystem (MemFs), not a separate code path from a real apply (RealFs).
- Rollback is "just another plan": undo runs through the same validate, preview, apply pipeline as a forward run.
- Never-overwrite: the executor never overwrites an existing file at a destination path.

## Human-only gates

Some actions require explicit human approval and must never be taken autonomously: any Real (non-dry-run) apply against the actual library, publishing releases or tags, the public-repo flip, spending money, and rewriting git history. See `EXECUTION.md` for the full allowlist and governance detail.

## Windows-first

This is a Windows 11 desktop product first. Write examples with Windows paths. macOS support is compiles-in-CI honesty only, not a current design target, unless a decision record says otherwise.

## Vocabulary discipline

The product-facing vocabulary is narrow and deliberate: books, library, duplicates, organize, Archive, undo. Internal engineering terms such as operations, dedupe, manifest, journal, quarantine, or dashboard belong in code and internal docs, never on a screen, dialog, toast, or exported report a user reads.

"Organize" is a verb with no noun form, and FD-48 retires the noun rather than substituting one. Where copy needs a noun it uses one the register already carries: "the plan", "the changes", or "run". Engineering identifiers such as `needs_tidy_books` and the "tidying-blocked" error code are out of scope: they are IPC and schema names no user reads.

## When sources disagree

If an instruction, a spec, or a prototype conflicts with `PRODUCT.md` or a ratified decision record (`D-nn` or `FD-nn`, cited in a release spec or the roadmap), the decision record wins. Flag the conflict rather than silently picking a side, unless it is already resolved in a decision record you can cite.
