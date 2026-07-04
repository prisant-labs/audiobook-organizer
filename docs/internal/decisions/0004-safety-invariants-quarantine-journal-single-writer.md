---
status: accepted
date: 2026-07-03
decision-makers: [jprisant]
---

# 4. Safety Invariants: Quarantine, Journal, Single-Writer

## Context and Problem Statement

The tool's whole purpose is moving 297 GB of files that are annoying to reacquire. The strategy brief names data loss as the catastrophic risk, with concrete failure shapes: collision overwrite, a partial move interrupted mid-campaign, Windows path-limit failures, reserved device names, trailing dots or spaces, and case-insensitive collisions on NTFS. The mitigation had to be architectural, not procedural, given a solo agent-driven build with no human code review.

## Considered Options

- Rely on careful review and manual spot-checks per campaign, with no structural guarantees.
- Adopt a fixed set of architectural safety invariants enforced mechanically by the executor and validated in CI (chosen).

## Decision Outcome

Chosen: **four safety invariants plus two structural mechanisms** (D-09 (safety invariants), 2026-07-02):

- **Quarantine-only:** no delete of audio anywhere in the product; removal is always "set aside," never destroyed.
- **Journal-before-act:** every operation is journaled before it executes, so interruption at any point leaves a reconcilable trail.
- **Single-writer rule:** exactly one apply job may run process-wide at a time.
- **Never-overwrite:** an apply never overwrites an existing target; a target appearing mid-apply halts that operation.
- **Vfs seam (`MemFs`/`RealFs`):** dry-run is the same executor code running against an in-memory filesystem, not a separate simulated code path.
- **Rollback is "just another plan":** undo is not bespoke logic; it is a generated plan that runs through the same validate, preview, apply pipeline as any forward plan.

### Consequences

- Good, because the safety story is mechanical and testable: the rollback round-trip test (v0.5.0 (acting) gate) and the never-overwrite adversarial test are CI gates, not conventions a reviewer must remember to check.
- Good, because the Vfs seam means dry-run and Real share one code path, closing the class of bug where "the simulation lied."
- Good, because reusing the plan pipeline for rollback avoids a second, less-tested code path for the second most dangerous operation in the product.
- Bad, because quarantine-only means disk space is never reclaimed by the tool itself; users must clear quarantine manually, which is a deliberate tradeoff of safety over tidiness.
- Bad, because the single-writer rule means no concurrent campaigns; acceptable for a personal/family-scale tool, would need revisiting for multi-user use.
