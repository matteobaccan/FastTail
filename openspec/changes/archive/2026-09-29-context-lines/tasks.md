## 1. Range core

- [x] 1.1 `src/context_lines.rs`: `Range { first, last, row }`, `ContextRanges { n, ranges, rows }` with `build(n, matches)`, `extend(new_matches)`, `visible_count(total_lines)`, `line_at(pos, total)`, `pos_of_line(line, total)`, `gap_before(range_idx)` (hidden lines before a range)
- [x] 1.2 Unit tests: merge of touching and overlapping ranges, clamp at line 0, unclamped last range clipped to the file end, incremental `extend` equal to a full `build`, 10 million matches built within 100 ms

## 2. Engine integration

- [x] 2.1 `TailEngine.context_lines` and `set_context_lines(n)`; `visible_lines`, `line_at`, `pos_of_line`, `row_of_line_or_next` use the ranges when `n > 0 && rows_filtered()`; `is_context_row(line)` by binary search in `filtered_lines`
- [x] 2.2 Build on filter result, extend on `ScanBatch::Lines` and on appended matches, clear with `filtered_lines` on reload / truncation / rotation / re-decode; keep the top line in place on a change of `n`; rebuild on a worker above 10 million matches
- [x] 2.3 `VisibleSet { All, Filter, Lines, Ranges }` for `JobSpec::Search` and `JobSpec::Collapse`; worker cursor over ranges; search refresh on a change of `n`, no filter refresh
- [x] 2.4 Engine tests (thresholds at 0 for the job paths): rows equal `grep -C` output on fixtures; synchronous and background results identical; appended lines within `n` of the last match shown without recomputation; global filter matches get context; show in context bypasses and restores; collapse over context rows; time delta over context rows

## 3. UI and persistence

- [x] 3.1 `±N` drag value (0–100) in the stream bar, Text view only, dimmed without an active filter, tooltip saying context ignores the filters
- [x] 3.2 Dimmed context rows (text and colours at 55 %) and the separator rule with "N lines hidden" tooltip, normal and wrap layouts
- [x] 3.3 Overview strip and scroll bar over the rows shown; search markers on context rows
- [x] 3.4 `StreamEntry.context_lines` read and written as `context_lines=N` (only when `N > 0`) in workspace and session files; round-trip and old-file tests

## 4. Texts and documentation

- [x] 4.1 New i18n keys (control tooltip, separator tooltip, help entry) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 README (filters section, feature list, comparison table) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 5.2 Local preview exe for the maintainer before the 0.13.0 release
- [x] 5.3 After the release, archive the change so `filters-and-highlighting`, `search-and-navigation` and `selection-and-export` gain the new requirements
