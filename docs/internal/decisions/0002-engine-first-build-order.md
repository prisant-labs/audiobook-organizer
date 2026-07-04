---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
consulted: [claude, codex]
---

# 2. Engine-First Build Order

## Context and Problem Statement

The strategy brief's Approaches section weighed four ways to sequence the build. The catastrophic risk in this product is the file mover, not the screens: a collision overwrite, a partial move, or a Windows path-length failure on 297 GB of hard-to-reacquire files. The build order must put the riskiest logic under test before any pixel exists.

## Considered Options

- **Approach A, script-and-hands cleanup, no app:** PowerShell scripts and Bulk Rename Utility recipes, no engine or GUI built at all.
- **Approach B, engine-first on the common stack, GUI second (recommended):** build `abo-core` (pure Rust, zero Tauri deps) against synthetic fixtures, freeze the typed IPC contract, then build the GUI against that seam.
- **Approach C, GUI-first full MVP in one push:** scaffold the Tauri app and build scan/patterns/templates/preview/apply/rollback as one integrated effort.
- **Approach D, adopt existing tools, build only glue:** survey the OSS renamer/tagger ecosystem and write only bridging scripts.

## Decision Outcome

Chosen: **Approach B, engine-first** (D-07 (engine-first order), 2026-07-02, agent-recommended and jp-ratified via the planning docs). `abo-core` hardens on fixtures before any GUI; the GUI renders a frozen tauri-specta contract. Approach A's first two phases (staging separation, ABS repoint) still happen by hand immediately, independent of the build; Approach D is retained only as a timeboxed pre-build check (FD-15 (OSS-landscape check), one hour) rather than as the primary strategy, since no shelf tool was found that plans structural moves with manifests.

### Consequences

- Good, because the dangerous logic (parsing, planning, moving) becomes deterministic, unit-tested, and provable before any UI decision can pressure it.
- Good, because a CLI/JSON harness means the real-library campaign can start before the GUI is finished (v0.3.0 already produces a useful exported plan and HTML report).
- Good, because the seam pattern (core crate plus frozen IPC contract) is already proven in repo-sync-tool, so scaffolding is largely transplantation.
- Bad, because it is slower to first screenshot than Approach C, and requires discipline to keep the engine truly UI-agnostic.
- Bad, because over-engineering the engine for hypothetical future media types is a live risk that must be actively resisted.
