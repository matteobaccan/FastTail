## 1. Engine

- [x] 1.1 `ContextView` state in `TailEngine` (anchor line, saved top line, saved follow, saved selection and anchor) with `enter_context(line)`, `leave_context()` and `in_context()`; bump `filter_generation` on both
- [x] 1.2 `rows_filtered()`; classify every `is_filter_active()` call site: row mapping, overview strip, copy and export of visible lines use `rows_filtered()`, filter maintenance and jobs keep `is_filter_active()`
- [x] 1.3 End the context view on any filter change (terms, toggles, level, time range, preset, global filter) keeping the anchor centred when visible; on reload, truncation or rotation without restoring
- [x] 1.4 Unit tests: enter / leave keep `filtered_lines` identical and start no job (thresholds at 0); appended lines filtered while in context; restore of top line, selection and follow; filter edit and truncation end it

## 2. UI

- [x] 2.1 Row context-menu item "Show in context" (Text view, active filter, index built) and `CTRL + K` on the selection anchor in `dock.rs`
- [x] 2.2 Banner above the rows with "Back to filtered view"; `Esc` when the rows have the keyboard; `CTRL + K` toggles back; `◆` marker on the anchor row; dimmed filter fields with the banner text as tooltip
- [x] 2.3 Restore the top row on return (binary search in `filtered_lines`); leave the context view before switching to HEX or Markdown view
- [x] 2.4 Find results: context-menu item and `CTRL + K` on the selected result; `apply_find_jump` enters the context view; no-op on stale groups, plain jump without a filter
- [x] 2.5 Integration tests: context view rows equal an unfiltered stream's; export in context view; Find results entry on a line hidden by the filter

## 3. Texts and documentation

- [x] 3.1 New i18n keys (menu item, banner text, back button, suspended tooltip, help entry) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 Help dialog: `CTRL + K` entry (modifiers in capitals)
- [x] 3.3 README (filters and search sections, shortcuts table), CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the 0.12.0 release
- [ ] 4.3 After the release, archive the change so `search-and-navigation` gains both requirements
