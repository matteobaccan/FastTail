## Why

A search in FastTail marks the matching rows in place and `F3` walks them one at a time. With a few hundred hits scattered through a large log, the user cannot see them together or tell where in the file they cluster. klogg's answer is a second pane under the main view that lists only the matching lines, plus marks next to the scrollbar; clicking a listed line brings the main view there with its surrounding context. FastTail already has everything underneath: the search job produces the ordered list of hit line indices (`search_matches`), and bookmarks and per-line levels are cached.

## What Changes

- A **search results pane** under the main rows of a stream, toggled from the stream bar, listing only the lines in `search_matches` (line number and text, query tinted), virtualized so only the rows on screen are read. Clicking a row, or selecting it with the arrow keys and pressing `Enter`, makes it the current match and centres the main view on it, pausing follow. The current match is marked `▶` in the pane and the pane follows `F3` / `Shift+F3`.
- An **overview strip** beside the main view's vertical scrollbar: marks for search hits, bookmarks and ERROR/FATAL lines at their proportional position among the visible rows, the viewport drawn as a box, click or drag to scroll there. Exact on ordinary files, sampled for the error marks on very large filtered views, recomputed only when its inputs change.
- **Match cap raised and made visible**: the stored match list grows from 20,000 to 1,000,000 hits per stream (8 MB at most); past the cap the search keeps counting, and the counter and the pane header show the true total.
- Focus rules: the pane belongs to its stream's dock panel; keys go to the pane only while it holds keyboard focus; `F3`, `Ctrl+F`, bookmarks and go-to keep acting on the stream.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `search-and-navigation`: new Search Results Pane and Overview Strip requirements; Match Navigation Scoped to the Focused Window gains the pane focus rules; Match Counter, Wrap-Around and History gains the match cap and the true total.
- `selection-and-export`: Export Visible Lines and Search Matches states that "Export search matches" writes the stored hits (at most 1,000,000), and that the exported text is the line without its ANSI escape sequences in ANSI render and strip modes (brought in line with the `ansi-color-codes` change).

## Impact

- New `src/ui/hit_list.rs`: a virtualized list of `(stream, line)` hits with query tinting and a click / `Enter` callback, shared with the `search-all-streams` change.
- New `src/ui/overview_strip.rs`: per-pixel mark cache and painter.
- `src/ui/dock.rs`: pane as a resizable bottom region inside the stream tab, toggle button, keyboard routing, strip placement next to the rows `ScrollArea`.
- `src/tail_engine.rs`: `MAX_SEARCH_MATCHES` to 1,000,000 (text hits; HEX byte hits keep 20,000), `search_total` counted past the cap in the sync path, `refresh_search_from` and the Search job (`JobSpec::Search` gains `count_past_limit`), a search generation counter for cache invalidation, per-4096-line ERROR/FATAL block counts maintained in `push_levels` / `truncate_levels`.
- `src/config.rs`: `search_pane` (open flag), `search_pane_height`, `overview_strip` (Settings checkbox, default on) in `fasttail.ini`.
- `src/i18n.rs`: new keys in all 16 languages. README shortcuts / features and CHANGELOG.
