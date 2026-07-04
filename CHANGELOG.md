# Changelog

All notable changes to Audiobook Organizer are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/).

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
