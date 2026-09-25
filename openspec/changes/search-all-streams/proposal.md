## Why

FastTail is often used with many files open at once — a service's app, access and error logs, or one log per host. Searching is per stream: to find where a request id appears, the user types it into every stream's search box in turn and reads each counter. klogg and lnav search one file (or one merged view) at a time; a "find in all open files" results list, familiar from editors, is the missing step between per-stream search and a full merged timeline.

## What Changes

- **Search all streams**: `Ctrl+Shift+F` (or a button next to each stream's search box) opens a single "Find results" dock tab with a query box, prefilled with the focused stream's query. Running it searches every open stream with the same case-insensitive text match as the per-stream search, over the lines each stream currently shows under its own filters.
- **One background job per stream**, independent of the stream's own filter/search job, with at most `min(4, available cores)` running at once and the rest queued; each group shows its progress. A new query, the Stop button or closing the tab cancels every job; closing a stream cancels its job and drops its group.
- **Results grouped by stream** with a header per stream (name, match count, progress, a capped note past 100,000 listed hits) and the matching lines below it, in one virtualized list with collapsible groups.
- **Clicking a result** (or `Enter` on the keyboard selection) activates the stream's dock tab, focuses its panel, centres the line and pauses follow; the stream's own search query is not changed. A result whose stream was reloaded since the search is flagged stale instead of jumping to the wrong line.
- Results are a snapshot: lines appended after a stream's job started are not added; a Refresh button reruns the query.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `search-and-navigation`: new Search All Streams and Find Results Tab requirements.
- `cyber-ui-docking`: Keyboard Navigation & Hotkeys gains `Ctrl+Shift+F`.

## Impact

- New `src/find_all.rs`: `FindAllSession` (query, snapshot time, per-stream `StreamFind { path, reload_generation, job: Option<ScanJob>, hits, total, progress, state }`, queue and concurrency limit), polled once per frame from the app.
- Reuses `scan_job::ScanJob` with `JobSpec::Search { query_lower, filter, limit, count_past_limit }` unchanged in semantics; the time window is applied to drained hits through `TailEngine::in_time_range`, as in the `background-timestamp-scan` change.
- Reuses `src/ui/hit_list.rs` from the `search-results-pane` change, extended with group header rows; this change depends on that widget and on `count_past_limit`.
- `src/ui/dock.rs`: `FastTailTab::FindResults`, its tab body, a focus request (`(path, line)`) handed back to the app, which calls `find_tab` / `set_active_tab` and sets `focused_stream` after the dock is drawn; the stream-bar button.
- `src/tail_engine.rs`: a public `reload_generation` bumped on reload, truncation, rewrite and pattern switch; `goto_target_for` made reusable for the jump.
- `src/ui/app.rs`: the tab is not written to the saved dock layout.
- `src/i18n.rs`: new keys in all 16 languages. README shortcuts and features, CHANGELOG.
