## Why

Enhance search navigation, viewport filter virtualization, window dragging ergonomics, and application branding:
1. Moving the window in borderless mode shows a standard arrow cursor instead of a 4-way move cursor.
2. In filtered view, virtual scrolling previously indexed over all lines while skipping non-matching rows inside the visible slice, resulting in half-empty windows and exclusion filters only appearing to work on a subset of lines.
3. The "All / Filtered" toggle button is redundant because the presence of text in the include or exclude input boxes naturally determines whether filtering is active.
4. F3 and Shift+F3 shortcuts were documented but not implemented for finding next/previous search matches.
5. The currently focused search match line lacked visual distinction from other matching lines.
6. Search wrap-around lacked acoustic feedback when looping back to the start or end of the buffer.
7. Search lacked a persistent history of recent queries.
8. The window titlebar omitted the FastTail version number.
9. The About dialog lacked clickable hyperlinks to the GitHub repository and Matteo Baccan's website (`https://www.baccan.it`).

## What Changes

- **Window Move Cursor**: Set `CursorIcon::Move` on draggable titlebar areas and maintain it during drag operations.
- **Filtered Row Virtualization**: Precompute `filtered_lines: Vec<usize>` in `TailEngine` whenever filters change; virtualize `show_rows` over `visible_line_count()`, eliminating blank gaps and ensuring 100% viewport utilization and accurate exclusion across all lines.
- **Automatic Filter Activation**: Remove the "All / Filtered" toggle button from the stream toolbar; filtering activates automatically whenever include or exclude fields contain text.
- **F3 / Shift+F3 Search Navigation**: Implement F3 (next match) and Shift+F3 (previous match) with viewport auto-scrolling to the target line.
- **Current Match Highlighting**: Highlight the active search line with a glowing accent background/border and a pointer indicator (`▶`), and display a match counter (`X / Y`) with next/prev buttons in the search bar.
- **Wrap-Around Audio Alert**: Play an audio chirp when search wraps around the buffer boundaries.
- **Search History (Last 10 Queries)**: Persist the 10 most recent search queries in `fasttail.ini` and provide a dropdown menu (`🕒`) to quickly re-run past queries.
- **Version in Header & Clickable Links in About**: Display `v{CARGO_PKG_VERSION}` in the titlebar, and provide clickable hyperlinks to the GitHub repository and `https://www.baccan.it` in the About dialog.

## Capabilities

### Modified Capabilities
- `cyber-ui-docking`: Window titlebar move cursor, header version display, clickable links in About dialog, and seamless virtual scroll rendering for filtered rows.
- `filters-and-highlighting`: Automatic filter activation based on include/exclude content, F3 / Shift+F3 search navigation, current match row highlighting, wrap-around audio alert, and search history persistence.

## Impact

- `src/tail_engine.rs`: Add `filtered_lines`, `search_matches`, `current_match_idx`, `search_next`, `search_prev`, `recompute_filtered_lines`.
- `src/config.rs`: Add `search_history` to `FastTailConfig`, serialize/deserialize in `to_ini`/`from_ini`.
- `src/ui/app.rs`: Set `CursorIcon::Move` on titlebar, add version to header, add clickable links in About dialog.
- `src/ui/dock.rs`: Remove "All / Filtered" button, virtualize `show_rows` over `visible_line_count`, render active search line highlight, add search history menu, handle F3 / Shift+F3.
- `tests/integration_tests.rs`: Comprehensive test suite for filtering virtualization, search navigation, wrap-around sound alert, and search history persistence.
