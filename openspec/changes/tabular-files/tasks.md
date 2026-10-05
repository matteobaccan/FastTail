## 1. Parser

- [ ] 1.1 `src/fields.rs`: delimited parser (`,` `;` tab `|`), RFC 4180 quoting, cell spans; unit tests
- [ ] 1.2 Separator and header detection on the first 64 lines; the extension as tie-break
- [ ] 1.3 Quoted multi-line records joined as one row (cap 1,000 lines); tests on broken quotes

## 2. View

- [ ] 2.1 Window: parser selector entry, header names, `⏎` marks, column choice and widths
- [ ] 2.2 Terminal interface: the same columns
- [ ] 2.3 Field filter terms, copy as shown and export on cells
- [ ] 2.4 `csv_separator=`, `csv_header=` per stream; round-trip test

## 3. Texts, docs, wrap-up

- [ ] 3.1 i18n keys in every language; README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG
- [ ] 3.2 Integration tests (comma, semicolon, tab, no header, quoted newlines, growing file)
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
