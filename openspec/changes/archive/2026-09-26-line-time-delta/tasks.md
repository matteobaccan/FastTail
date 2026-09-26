## 1. Engine and formatting

- [x] 1.1 `timestamp::format_delta` (sign, `S.mmm`, `M:SS.mmm`, `H:MM:SS`, `Nd HH:MM`) with unit tests at each boundary and for negative values
- [x] 1.2 `TailEngine`: `time_anchor` (set, clear, toggle on the same line, cleared on truncation/rewrite); `row_time_delta(visible_row)` from the cache and the previous visible row (or the anchor); `selection_elapsed()` from the selection ends or the first/last visible rows under `selection_all`
- [x] 1.3 Tests: delta follows the include filter (previous visible row), continuation lines blank, first row blank, anchor signed values above and below, anchor survives a filter change and is cleared on truncation, selection elapsed with a range and with Ctrl+A, zone suffix not applied

## 2. UI

- [x] 2.1 Δt column (10 characters, right-aligned) in `render_log_stream` and `render_wrapped_rows`; own-timestamp check on on-screen rows; `…` for rows not timed yet; bounded `fill_timestamps` per frame with repaint while incomplete (implemented through `request_timestamps`: synchronous up to the job threshold, a background `Timestamps` scan above it, see `TailEngine::want_timestamps`)
- [x] 2.2 Gap tint at `time_delta_gap_ms` in previous-row mode; `⚓` on the anchor row
- [x] 2.3 Toolbar `Δt` toggle beside the line-number button, hidden column with an explanatory tooltip when timestamps are not usable
- [x] 2.4 Row context menu: "Set time anchor here" / "Clear time anchor"
- [x] 2.5 Stream status bar: `Δ … · N rows` for a selection of two or more timed rows
- [x] 2.6 Settings: gap threshold (ms, 0 = off)

## 3. Config

- [x] 3.1 `show_time_delta` and `time_delta_gap_ms` in `fasttail.ini` with defaults (off, 1000); round-trip test and old-config test

## 4. i18n and docs

- [x] 4.1 New keys (button tooltip, no-timestamps hint, context-menu entries, status label, setting label) in all 16 languages; i18n coverage test passes
- [x] 4.2 README: the Δt column, anchor and selection elapsed time, with the "clock the log printed" note
- [x] 4.3 CHANGELOG `[Unreleased]` entry
