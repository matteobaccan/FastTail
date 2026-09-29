## 1. Engine

- [x] 1.1 `CharSelection { line, anchor, head }` per stream; cleared on `Esc`, click on another row, truncation, reload and when a filter hides the line
- [x] 1.2 Shown-text range extraction (ANSI mode, long-line cap, tabs kept)
- [x] 1.3 Tests: range extraction with ANSI render / strip / raw; cap boundary; clearing on truncation and on filter change

## 2. View

- [x] 2.1 Text area senses click-and-drag; hit-test with the row galley in unwrapped and wrapped layout, gutter excluded, horizontal scroll included
- [x] 2.2 Drag clamped to the row; selection painted over the text (translucent). Edge auto-scroll not done: the row can be scrolled with the wheel while selecting; revisit on feedback
- [x] 2.3 Caret on click; `SHIFT + ←/→`, `CTRL + SHIFT + ←/→`, `SHIFT + Home/End`; keys consumed only with a caret in the focused stream
- [x] 2.4 `CTRL + C` precedence; "Copy selected text" in the row menu; `CTRL + F` fills the search box (at most 256 characters)
- [x] 2.5 Double-click word, triple-click row text; coordinate with the `quick-wins-0-13` selection highlight
- [x] 2.6 (moved to `structured-field-terms` task 3.4) With `structured-fields` shipped: in the column view the selection stays inside one cell
- [x] 2.7 Tests (egui kittest or logic-level): drag range, word selection, copy precedence, row selection unchanged by a plain click

## 3. Texts and documentation

- [x] 3.1 New i18n keys (menu item, help entries) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README shortcuts table and help dialog; CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the release
- [x] 4.3 After the release, archive the change so `selection-and-export` gains the new requirement
