# Security policy

## Reporting a vulnerability

Please report security issues privately, not as a public issue.

Use GitHub's private vulnerability reporting on this repository: **Security -> Report a vulnerability**. That opens a private thread visible only to the maintainer.

This is a single-maintainer project, so there is no on-call rotation and no guaranteed response window. Expect a reply within a couple of weeks. If an issue is being actively exploited, say so in the first line.

## What is in scope

Audiobook Organizer is a local-first desktop application. It has no server, no accounts, no telemetry, and makes no network requests at runtime (there is a CI check that fails the build if a network-reaching string appears in the built output). So the interesting surface is local:

- Anything that causes the app to read or write a path outside the library root or the configured set-aside root.
- Anything that lets the WebView layer reach the filesystem directly, or invoke a real (non-rehearsal) change without going through the intended authorization path.
- Anything that corrupts or bypasses the journal, so that a change happens without a durable record of it.
- Path-handling defects: traversal, reparse-point or junction substitution, case-folding collisions, extended-length prefix handling.
- Anything that causes an audiobook file to be deleted or overwritten. The product's central guarantee is that no audiobook is ever deleted and no move ever overwrites an existing file.

## Known gaps, already documented

These are recorded in the repository rather than being undisclosed. Please do not file them as new reports, though reports that *deepen* them are welcome.

- **Real changes are gated procedurally, not mechanically.** The shipped frontend pins every run to rehearsal, but the underlying command still accepts the mode over IPC. A modified frontend or a compromised WebView could therefore reach the real-change path. A backend authorization boundary is planned work, and until it lands the app should be treated as a rehearsal tool.
- **Cross-volume moves verify by size, not content.** A move between drives copies, compares byte length, then removes the source. Equal length is not equal content. Content hashing before source removal is required work.
- **The durability boundary is process-kill, not power-loss.** The journal is written before each action and survives the process dying. It is not proven to survive a power cut between the write and the action.
- **macOS builds are not safe for real changes.** The no-overwrite guarantee is enforced with a Windows-specific API; the macOS path has a known race. macOS is a compile check only.

See the "What works today, and what does not" section of [README.md](README.md) for the fuller picture.

## Out of scope

- Anything requiring an attacker who already has code execution as the user. A desktop app cannot defend against another program on the same account editing its files or its database directly.
- The unsigned installer and the resulting SmartScreen warning. That is a deliberate, documented posture for the current stage, not a defect.
- Dependency advisories that are already surfaced by Dependabot on this repository.

## Supported versions

No version has been released or tagged yet. Only the current `main` is supported.
