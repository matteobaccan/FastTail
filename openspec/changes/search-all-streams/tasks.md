## 1. Session and jobs

- [ ] 1.1 Add `src/find_all.rs`: `FindAllSession` with the query, snapshot time, per-stream state (path, `reload_generation`, job, hits, total, progress, queued / running / done / failed / skipped / stale) and a FIFO with a concurrency limit of `min(4, available_parallelism)`
- [ ] 1.2 Spawn `ScanJob`s with `JobSpec::Search { query_lower, filter, limit: 100_000, count_past_limit: true }` over each stream's current file; wait for streams whose index is still being built; skip HEX-view streams
- [ ] 1.3 Poll once per frame from the app: drain batches, apply the stream's time window with `in_time_range`, start queued jobs, detect closed and reloaded streams
- [ ] 1.4 Add a public `reload_generation` to `TailEngine`, bumped on reload, truncation, rewrite and pattern switch
- [ ] 1.5 Tests: results equal each stream's own search on the same files and filters; concurrency never exceeds the limit; cancellation on new query / Stop / tab close / stream close; stale marking after truncation; cap with true total; time window honoured

## 2. UI

- [ ] 2.1 `FastTailTab::FindResults` with title, close handling, and exclusion from the saved layout; `Ctrl+Shift+F` opens or focuses it (split below the first leaf), prefilled with the focused stream's query; button next to each stream's search box
- [ ] 2.2 Tab body: query box (runs on `Enter` / Find), Stop, Refresh, summary line with counts and snapshot time
- [ ] 2.3 Extend `src/ui/hit_list.rs` (from `search-results-pane`) with header rows; flatten groups into one `show_rows` list with collapsible headers, per-group progress, capped and stale notes
- [ ] 2.4 Commit writes a focus request; the app activates the stream's tab, sets `focused_stream`, sets `scroll_to_line` (next visible line if hidden), pauses follow and selects the row; the stream's search is unchanged
- [ ] 2.5 i18n keys (tab title, query hint, Find / Stop / Refresh, summary, per-group counts, capped, skipped HEX, stale, button tooltip) in all 16 languages; i18n coverage test passes

## 3. Docs

- [ ] 3.1 README: feature list, `Ctrl+Shift+F` in the shortcuts table and in the Help dialog text, comparison table row if applicable
- [ ] 3.2 CHANGELOG `[Unreleased]` Added entry
