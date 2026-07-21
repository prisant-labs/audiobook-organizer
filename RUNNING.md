---
title: "Audiobook Organizer - Running the app locally"
type: guide
project: audiobook-organizer
created: 2026-07-20
updated: 2026-07-20
status: active
---

# Running the app locally

The step-by-step to launch Audiobook Organizer on your Windows machine and see the current functionality. This is the developer run path (dev build). Packaged installer docs come later at v0.9.0.

## Prerequisites (one time)

All of these are already present on jp's machine as of 2026-07-20. Listed for a fresh machine.

- **Rust (stable).** The exact channel is pinned in `rust-toolchain.toml`; `rustup` picks it up automatically. Check: `cargo --version`.
- **pnpm + Node.** Node version pinned in `.nvmrc`; pnpm version pinned in `package.json` (`packageManager`). Check: `pnpm --version`.
- **Tauri v2 platform dependencies.** On Windows 11 the only real requirement is WebView2, which ships with the OS. See the Tauri prerequisites guide if a C toolchain is missing.

## Step by step

From the repo root (`E:\Projects\prisant-labs\audiobook-organizer`):

1. **Install frontend dependencies** (only needed the first time, or after a dependency change):

   ```
   pnpm install
   ```

2. **Launch the app in dev mode:**

   ```
   pnpm tauri dev
   ```

   This starts the frontend (Vite on `http://localhost:1420`), compiles the Rust backend, and opens the **Audiobook Organizer** window.

   - First launch compiles the whole Rust workspace: expect a few minutes with no build cache. Every launch after is near-instant.
   - The window is frameless by design (`decorations: false`); the app draws its own titlebar.

3. **First-run: choose a library folder.** With no saved library root, the only way forward is "Choose your library folder", which opens the OS folder picker. Pick one:
   - Your real library `E:\Books - Audio` is **safe to point at**. Scanning is strictly read-only, and every apply is a dry run unless you deliberately cross the human-only Real-apply gate. You will see real covers and real health facts.
   - Or a small fixture / copy folder for a faster, smaller walk.

4. **Walk the three steps.** Scan, then Review (grouped cards + the exportable HTML report), then the Tidy-up / apply surface (dry-run). For a detailed click-path, follow `docs/internal/qa/v0.4.0-manual-qa.md` (covers first-run through both themes; it predates the v0.5.0 apply surface).

5. **Stop the app.** Close the window, or press `Ctrl+C` in the terminal running `pnpm tauri dev`.

## Safety while running

- Scanning never writes to the scanned folder (read-only invariant).
- Applies are dry runs by default; nothing on disk moves.
- A Real (non-dry-run) apply against the actual library is a human-only gate and never happens without an explicit, deliberate confirmation (EXECUTION.md Section 3).

## Optional: build an installer

To produce an installer (for the G-1 apply-surface walk, or to run without a dev server):

```
pnpm tauri build
```

Output lands under `src-tauri/target/release/bundle/` (MSI and NSIS `.exe` on Windows). The installer is unsigned through v0.9.0 (FD-22), so Windows SmartScreen shows a "More info, then Run anyway" prompt on first install.

## Troubleshooting

- **`cargo` not found:** install Rust via `rustup` (https://rustup.rs); `rust-toolchain.toml` handles the channel.
- **Port 1420 already in use:** another Vite dev server is running; stop it, or close the other app.
- **First compile feels stuck:** it is compiling hundreds of Rust crates the first time. Give it a few minutes; subsequent runs are fast.

## Quality gates (optional, what CI runs)

Not needed to view the app, but this is how you check a change the way CI will:

```
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm lint
```
