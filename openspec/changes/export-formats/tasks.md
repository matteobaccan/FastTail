## 1. Engine and writers

- [ ] 1.1 Export row iterator (line number, text without ANSI per mode, timestamp, level) for visible, selected and search-match scopes
- [ ] 1.2 `src/export.rs` CSV writer: RFC 4180 quoting, CRLF, separator, BOM, formula protection
- [ ] 1.3 CSV field columns when a `structured-fields` parser is active (shown fields in view order, then `message`)
- [ ] 1.4 HTML writer: escaping, class per style, theme colours, line-number spans, 200,000-line cap
- [ ] 1.5 Worker path above 1,000,000 lines and for HTML: shared-read handle, chunked ranges, snapshot of rules / labels / theme, progress and Cancel; stop on truncation
- [ ] 1.6 Tests: quoting and CRLF; BOM on / off; formula protection cases (`=cmd`, `-5`, `@x`); ISO time with and without zone; field columns and unparsed lines; HTML escaping of `<script>`; cap notice; export of a file growing during the export

## 2. UI and settings

- [ ] 2.1 Export dialog (what, format, CSV options) and the two existing menu items opening it preset
- [ ] 2.2 `export_format`, `export_csv_separator`, `export_csv_bom`, `export_csv_protect` in `[general]`

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (dialog, formats, options, cap notice) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (export section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `selection-and-export` gains the new requirements
