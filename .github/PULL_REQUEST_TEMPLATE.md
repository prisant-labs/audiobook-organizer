<!-- Thanks for contributing to Audiobook Organizer. Please fill out the checklist below. -->

## What this PR does

<!-- A short description of the change and the release/phase it belongs to, e.g. "v0.1.0 Phase 3: scanner + file typing + persistence". -->

## Related release / issue

<!-- Link the release folder (docs/internal/releases/<version>/) and/or issue this addresses. -->

## Checklist

- [ ] Branch was created off the default branch (no direct commits to it)
- [ ] `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass locally
- [ ] `pnpm typecheck` and `pnpm lint` pass locally
- [ ] `abo-core` pulls no `tauri` dependency (core-purity gate stays green)
- [ ] No em-dashes (U+2014) or en-dashes (U+2013) anywhere in the diff
- [ ] Tests were added or updated for the behavior changed
- [ ] Docs / spec updated if the contract changed

## Notes for reviewers

<!-- Anything that needs a closer look, known gaps, or follow-ups. -->
