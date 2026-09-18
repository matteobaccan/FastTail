## 1. Engine

- [x] 1.1 Add `selection` (BTreeSet of line indices), `selection_anchor`, `select_row`, `toggle_row`, `extend_selection_to` (over visible ordering), `select_all_visible`, `clear_selection` and `selected_lines()` to `TailEngine`
- [x] 1.2 Clear the selection in the truncation/reopen path of the engine
- [x] 1.3 Add `export_lines(&self, indices, sink: &mut dyn Write)` plus `export_visible` and `export_search_matches` helpers using `BufWriter`
- [x] 1.4 Tests: range selection under filter, toggle, select all, clearing on truncation, export content for visible and matches

## 2. UI

- [x] 2.1 Handle click / Shift+click / Ctrl+click on rows in `show_rows` and paint the selection tint (theme accent, 25% alpha)
- [x] 2.2 Handle Ctrl+A and Ctrl+C for the focused stream when no text field has focus; copy via `ctx.copy_text`
- [x] 2.3 Add "Export visible lines..." and "Export search matches..." to the stream menu with `rfd::FileDialog::save_file`
- [x] 2.4 Add i18n keys (menu entries, tooltips) in en/it/fr/es/zh and extend the exhaustive i18n test

## 3. Docs

- [x] 3.1 Document the shortcuts and export entries in README and the F1 help dialog
