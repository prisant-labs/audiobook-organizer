---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 1. Stack Locked to the Common Stack

## Context and Problem Statement

The two independent discovery passes (`_local\initial-discovery\`) diverged on implementation stack: the Claude tech-stack deep dive leaned Python + PySide6 for speed to MVP, while the Codex approach memo recommended Tauri v2 + React + TypeScript + Rust for durability. jp resolved this by directive rather than re-analysis: audiobook-organizer must use the same stack as repo-sync-tool so that both projects share one architecture, one CI shape, and one set of hard-won conventions.

## Considered Options

- Python + PySide6 (fastest path to a working MVP, per the discovery tech-stack doc)
- Tauri v2 + Rust + React + TypeScript + shadcn/ui + SQLite (the repo-sync-tool common stack)
- A third, unevaluated stack chosen fresh for this project

## Decision Outcome

Chosen: **Tauri v2, Rust, React, TypeScript, shadcn/ui, SQLite (sqlx), tauri-specta** (D-01 (stack locked to the common stack), 2026-07-03), matching repo-sync-tool exactly. Reference architecture: `E:\Projects\product-on-purpose\repo-sync-tool\docs\internal\v1-architecture-and-decisions.md`.

This is a fixed constraint for the project, not an open question; it is not relitigated in any subsequent planning artifact.

### Consequences

- Good, because the architecture answers (workspace split with a zero-Tauri-dependency core crate, typed IPC via tauri-specta, sqlx migration policy, `%LOCALAPPDATA%` data placement, error taxonomy discipline, CI-as-honesty for macOS) transplant directly instead of being re-derived.
- Good, because a second, structurally different product (batch file operations rather than a resident tray app) validates the common stack's generality.
- Bad, because it trades away the discovery docs' fastest-prototype path; Rust filesystem code with rigorous safety semantics carries more ceremony than an equivalent Python script would have.
- Bad, because the project inherits a two-language (Rust plus TypeScript) context-switching cost that a single-language stack would avoid.
- Neutral, because macOS remains unverifiable beyond compiles-in-CI, exactly as it does for repo-sync-tool (same single-developer, single-machine reality).
