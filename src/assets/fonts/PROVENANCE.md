# Embedded font provenance (F-901, FD-11)

`Literata-latin.woff2` and `OFL.txt` here are a copy of the same font asset
already vetted and committed at `crates/abo-core/assets/fonts/` for the F-506
dry-run HTML report (see that directory's own `PROVENANCE.md` for the full
fetch history: Literata, latin subset, variable weight 400-700, Google Fonts
static hosting, SIL Open Font License 1.1). This copy exists so the frontend
bundle is self-contained under `src/` (Vite only bundles assets it can see
under the project root it builds) without the app depending on a path outside
`src/`. The bytes are identical; do not re-fetch or re-subset independently of
the core copy without updating both.

Used here for `--serif` (Literata) headings and cover titles per
design-system.md Section 3.1-3.2, loaded via a local `@font-face` in
`src/styles/tokens.css` (no network `<link>`, FD-11). System fallback stack is
`Georgia, serif`.
