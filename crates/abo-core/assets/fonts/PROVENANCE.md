# Embedded font provenance (F-506, FD-11/FD-28)

`Literata-latin.woff2` is the Literata typeface, latin subset, fetched once from
Google Fonts static hosting (`fonts.gstatic.com`, family version v40) during the
v0.3.0 Phase 8 build task on 2026-07-04.

Google serves Literata only as a VARIABLE font: the css2 and the legacy css
endpoints both return one woff2 file per unicode range covering the whole weight
axis. The latin-range file therefore provides both the regular (400) and bold
(700) weights the dry-run report uses, from a single 38 KB file. The report's
`@font-face` declares `font-weight: 400 700` so the browser interpolates both
weights out of this one embedded file.

Files:

- `Literata-latin.woff2` - the binary woff2 (latin subset, variable weight).
- `Literata-latin.woff2.b64` - the same bytes, standard base64, no line wrapping.
  This is what `crate::plan::report` embeds via `include_str!` into a
  `data:font/woff2;base64,...` URI, so the generated report is self-contained and
  makes zero network requests at runtime (FD-11). The report contains NO external
  URL of any kind.
- `OFL.txt` - the SIL Open Font License 1.1 the typeface ships under.

The report falls back to `Georgia, "Times New Roman", serif` if the embedded
font ever fails to load.
