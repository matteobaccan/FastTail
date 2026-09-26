## 0. Prerequisite

- [x] 0.1 `background-timestamp-scan` is merged (`request_timestamps()`, background `Timestamps` job, pending window)

## 1. Histogram model

- [x] 1.1 Add `src/time_histogram.rs`: absolute bucket numbering, `VecDeque` of per-level `u32` counts, at most 2,048 buckets, width doubling from 1 s with pair merge, `add`, `remove`, `reset`, untimed count, resample-to-columns helper
- [x] 1.2 Unit tests: doubling and merge preserve totals; out-of-order line before the first bucket; remove after add; resampling to fewer and to more columns than buckets

## 2. Engine integration

- [x] 2.1 `histogram_len` watermark fed from `min(levels.len(), timestamps.len())` after sync passes, drained `Levels` / `Timestamps` batches and appends
- [x] 2.2 Subtract lines before `truncate_timestamps` / `truncate_levels` drop them; reset on reload and pattern switch
- [x] 2.3 Tests: the incremental histogram equals one rebuilt from the caches after append, truncation, rewrite, a resumed timestamp scan and a Levels job finishing after the timestamps

## 3. UI

- [x] 3.1 Toggle next to the time range fields; opening the strip calls `request_timestamps()`; strip hidden with the "no usable timestamps" hint when unusable
- [x] 3.2 Painter: stacked level bars in theme colours, max label, shaded current window, timing progress while the scan runs, optional search lane
- [x] 3.3 Click / drag selection writes `format_millis` bounds (`to` = end bucket − 1 s) into the fields and calls `apply_time_range_text`; hover tooltip with span and counts per level
- [x] 3.4 Persist `timeline_histogram` and `timeline_search_lane` in `fasttail.ini`
- [x] 3.5 i18n keys (toggle tooltip, search lane toggle, tooltip lines, untimed count, search lane note) in all 16 languages; i18n coverage test passes

## 4. Docs

- [x] 4.1 README "Time range" section and feature list; comparison table row if applicable
- [x] 4.2 CHANGELOG `[Unreleased]` Added entry
