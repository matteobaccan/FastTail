## 1. Diff core

- [ ] 1.1 `src/compare.rs`: normalisation with span map (timestamp, numbers, ids, whitespace, case), word and line diff with `similar`, JSON canonicalisation, unified diff text
- [ ] 1.2 Worker path for regions with the 20,000-line cap and 2 s deadline fallback
- [ ] 1.3 Tests: one changed token in a long line; timestamps ignored; JSON with reordered keys and one changed value; region with inserted and removed lines; cap and timeout messages; unified diff output

## 2. UI

- [ ] 2.1 Row menu: "Compare selected lines" (exactly two rows), "Mark for compare", "Compare with marked line", "Compare selection with marked selection"; gutter marker for the mark
- [ ] 2.2 `FastTailTab::Compare`: side-by-side virtualized rows, word highlights, change count, `F7` / `SHIFT + F7`, headers, double-click back to the source line
- [ ] 2.3 Options bar (ignore toggles, Compare as JSON) and "Copy as unified diff"

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (menu items, tab, options, messages) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 Help dialog, README (feature list) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 4.3 After the release, archive the change so `selection-and-export` gains the new requirements
