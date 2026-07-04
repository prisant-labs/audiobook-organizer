---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 14. Offline-First: No Auto-Update, No Telemetry, Bundled Fonts

## Context and Problem Statement

The prototypes' dry-run report used a Google Fonts `<link>` tag, which violates a zero-network posture for a tool that handles a personal library and produces reports meant to be opened anywhere, including offline. Separately, the product's distribution and update posture (auto-update, telemetry, code signing) had not been fixed, and the feature-function breakdown's epic E-10 implied an offline-leaning posture without stating it as a hard rule.

## Considered Options

- Allow external network calls where convenient (web fonts, telemetry, auto-update checks), matching common desktop-app defaults.
- **Commit to a fully offline-first posture: bundled fonts, zero network requests from the app or the report, no telemetry, no auto-update in v1, unsigned installer until the public flip (chosen).**

## Decision Outcome

Chosen: **offline-first posture** (FD-11 (bundled fonts, zero network) and FD-22 (unsigned installer, no auto-update), plus the feature-function breakdown's epic E-10 posture, 2026-07-03). Literata is bundled in-app (SIL OFL, self-hosted woff2); the exported HTML report embeds a subsetted Literata as a data URI with a system serif fallback stack. Zero network requests occur in the app or the report; a CI check greps the report template and app for external hosts. The prototypes' Google Fonts `<link>` never ships. Distribution stays an unsigned installer through v0.9.0 (packaged) (private and family distribution only; the install doc explains the Windows SmartScreen "More info, then Run anyway" flow). Code signing (Azure Trusted Signing) is decided together with the public flip at v0.9.0+, per D-13 (OSS posture). There is no auto-update in v1: distribution is fully offline, updates are manual installer downloads, revisited post-1.0.

### Consequences

- Good, because it resolves the audit's Stream 2, finding 10 (IMPORTANT): the Google Fonts dependency is eliminated with a CI-enforced grep gate, not just a stated intention.
- Good, because a personal-library tool that never phones home matches the trust model the product needs: a family member should be able to believe "nothing about my library leaves this machine."
- Good, because deferring code signing and auto-update to the public-flip decision (D-13) keeps v1 scope bounded and avoids spending money before the OSS posture question is resolved.
- Bad, because an unsigned installer means every install triggers a SmartScreen warning, which is friction for the family-tier audience the UI is otherwise designed to protect (0006 (family tier sets UI bar)); the install doc must carry this weight instead.
- Neutral, because localization (FD-23 (English-only v1)) is a related but separate constraint: centralizing user-facing copy in one strings module is compatible with, but not required by, this offline-first decision.
