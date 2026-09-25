## 1. Engine

- [ ] 1.1 Raise `MAX_SEARCH_MATCHES` for text hits to 1,000,000 (HEX byte hits keep 20,000); add `search_total` counted past the cap by `find_matches_from`, `refresh_search_from` and the `Search` job (`JobSpec::Search { count_past_limit }`, a final count in `ScanBatch::Done` or a dedicated batch)
- [ ] 1.2 Add a search generation counter bumped whenever `search_matches` changes, for UI caches
- [ ] 1.3 Maintain per-4096-line ERROR/FATAL counts in `push_levels` / `truncate_levels`; expose a range query
- [ ] 1.4 Tests: total past the cap in the sync and job paths and on append; wrap-around within the stored hits; error block counts after append, truncation and a resumed Levels job

## 2. Hit list widget and pane

- [ ] 2.1 Add `src/ui/hit_list.rs`: virtualized rows over a slice of line indices (line number, level colour, truncated text with query tinting), current-hit marker, click / `Enter` commit, arrow / page / `Ctrl+Home` / `Ctrl+End` selection, focus outline, `Esc` hand-back
- [ ] 2.2 Resizable bottom pane inside the stream tab in `src/ui/dock.rs`, shown while a query is active and the pane is enabled; header with total, capped note and job progress; toggle button in the stream bar
- [ ] 2.3 Commit sets `current_match_idx`, centres the main view via `scroll_to_line` and pauses follow; the pane selection snaps to the current match on `F3` / `Shift+F3` / refresh; stick-to-bottom while following
- [ ] 2.4 Keyboard routing: pane keys only while the pane has keyboard focus; stream shortcuts unchanged; clicking in the pane focuses the stream's panel
- [ ] 2.5 Persist `search_pane` and `search_pane_height` in `fasttail.ini`; hide the pane in HEX view with a notice

## 3. Overview strip

- [ ] 3.1 Add `src/ui/overview_strip.rs`: per-pixel flag cache, rebuild on input change throttled to 4 Hz while growing; exact hits and bookmarks; errors exact from block counts (no filter) or visible rows (≤ 4,000,000), sampled above with a tooltip note
- [ ] 3.2 Place the strip beside the vertical scrollbar in Text view (plain and wrap mode); viewport box; click / drag scrolls and pauses follow; hover shows the line number; hidden in HEX and rendered Markdown views and when empty
- [ ] 3.3 Settings checkbox `overview_strip` (default on) persisted in `fasttail.ini`
- [ ] 3.4 Tests for the mark mapping (unfiltered, filtered, sampled threshold) as pure functions

## 4. i18n and docs

- [ ] 4.1 i18n keys (pane toggle and tooltip, header with total, capped note, no matches, HEX notice, strip setting and tooltips, sampled note) in all 16 languages; i18n coverage test passes
- [ ] 4.2 README: feature list, keyboard shortcuts (pane keys, `Esc`), comparison table row if applicable
- [ ] 4.3 CHANGELOG `[Unreleased]` Added entry (results pane, overview strip) and Changed entry (match cap 20,000 to 1,000,000, true total shown)
