## 1. Engine

- [ ] 1.1 `CharSelection { line, anchor, head }` per stream; cleared on `Esc`, click on another row, truncation, reload and when a filter hides the line
- [ ] 1.2 Shown-text range extraction (ANSI mode, long-line cap, tabs kept)
- [ ] 1.3 Tests: range extraction with ANSI render / strip / raw; cap boundary; clearing on truncation and on filter change

## 2. View

- [ ] 2.1 Text area senses click-and-drag; hit-test with the row galley in unwrapped and wrapped layout, gutter excluded, horizontal scroll included
- [ ] 2.2 Drag clamped to the row, edge auto-scroll; selection painted between tints and text
- [ ] 2.3 Caret on click; `SHIFT + ←/→`, `CTRL + SHIFT + ←/→`, `SHIFT + Home/End`; keys consumed only with a caret in the focused stream
- [ ] 2.4 `CTRL + C` precedence; "Copy selected text" in the row menu; `CTRL + F` fills the search box (at most 256 characters)
- [ ] 2.5 Double-click word, triple-click row text; coordinate with the `quick-wins-0-13` selection highlight
- [ ] 2.6 With `structured-fields` shipped: in the column view the selection stays inside one cell
- [ ] 2.7 Tests (egui kittest or logic-level): drag range, word selection, copy precedence, row selection unchanged by a plain click

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (menu item, help entries) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README shortcuts table and help dialog; CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `selection-and-export` gains the new requirement
