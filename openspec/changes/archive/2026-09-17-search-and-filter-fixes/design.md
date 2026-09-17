## Context

Following operational feedback, several functional and UX areas require refinement:
1. Moving the application window displays an arrow pointer rather than the standard 4-directional move cursor.
2. In filtered view, virtual scrolling previously operated over all raw lines while discarding non-matching lines inside the visible window range. This left the viewport half-empty and gave the impression that exclude filtering only affected a subset of rows.
3. The "All / Filtered" button is redundant because entering text in the include or exclude inputs naturally signifies filtering intent.
4. F3 / Shift+F3 shortcuts were unhandled, preventing quick traversal of search matches.
5. The active search match line lacked visual prominence over other matches.
6. Search wrap-around had no sound alert to signal when navigation loops back to the start or end of the file.
7. Search queries were not recorded in a history list.
8. The window title lacked the FastTail version string.
9. The About dialog lacked clickable hyperlinks for GitHub and www.baccan.it.

## Goals / Non-Goals

**Goals:**
- Set `CursorIcon::Move` on draggable titlebar areas and maintain it during window repositioning.
- Map filtered rows through `filtered_lines: Vec<usize>` so `show_rows` renders continuous, gap-free lines utilizing 100% of the viewport height.
- Eliminate the "All / Filtered" toggle button, triggering filtering automatically based on whether the include or exclude text is non-empty.
- Implement F3 (next) and Shift+F3 (prev) search match navigation with automatic scrolling and match count display (`[X / Y]`).
- Visually highlight the currently focused search match line with a distinct border/fill and pointer arrow (`▶`).
- Emit an audio beep when search wraps around the buffer boundaries.
- Maintain and persist the last 10 search queries in `fasttail.ini` with a dropdown history selector (`🕒`).
- Display `v{CARGO_PKG_VERSION}` in the top titlebar.
- Add clickable hyperlinks to GitHub and `https://www.baccan.it` in the About dialog.

**Non-Goals:**
- Modifying underlying file reading / memory mapping mechanics.
- Adding third-party audio decoding libraries (Windows `MessageBeep` is used).

## Decisions

### Decision 1: Filtered Row Virtualization via Index Mapping
- **Choice**: Maintain `filtered_lines: Vec<usize>` in `TailEngine`. When `is_filter_active()` is true, `show_rows` receives `filtered_lines.len()` and maps `row_idx` to `filtered_lines[row_idx]`.
- **Rationale**: Egui's `show_rows` expects every row in `0..row_count` to take `row_height` vertical space. Skipping rows inside the closure breaks virtual scrolling geometry, causing empty screen areas. An index vector provides $O(1)$ lookup per visible row while keeping the viewport fully filled.
- **Alternatives considered**:
  - *Dynamic row height estimation*: Too complex and causes scrollbar jitter.

### Decision 2: Automatic Filter Detection
- **Choice**: Remove `ViewMode::Filtered` button from UI. `TailEngine::is_filter_active()` returns `!self.include_filter.is_empty() || !self.exclude_filter.is_empty()`.
- **Rationale**: Eliminates unnecessary user clicks. If a filter is typed, filtering is active; if both filters are empty, all lines are visible.

### Decision 3: Search Navigation State in `TailEngine`
- **Choice**: Store `search_matches: Vec<usize>`, `current_match_idx: Option<usize>`, and `last_searched_query: String` in `TailEngine`.
- **Rationale**: Centralizes search position with the engine buffer. Enables F3 / Shift+F3, match count (`[3 / 42]`), wrap-around detection, and synchronized viewport jumping.

### Decision 4: Wrap-Around Sound Alert
- **Choice**: When `search_next` or `search_prev` wraps around (end-to-start or start-to-end), invoke `SoundAlertPreset::Beep.play()`.
- **Rationale**: Familiar behavior from standard text editors (Notepad++, VS Code), confirming to the operator that the buffer bounds have been traversed.

### Decision 5: Search History in Config & INI
- **Choice**: Add `search_history: Vec<String>` (max 10 entries) to `FastTailConfig`, serialized in `fasttail.ini` under `[search_history]`. Render a `🕒` dropdown button next to the search input.
- **Rationale**: Matches `recent_files` pattern and provides instant access to previous queries across sessions.

### Decision 6: Cursor Icon Move
- **Choice**: Set `ctx.set_cursor_icon(CursorIcon::Move)` when hovering or dragging over the titlebar draggable areas.
- **Rationale**: Gives operators immediate visual feedback that the window can be moved.

### Decision 7: Version and Clickable Links
- **Choice**: Format titlebar label with `env!("CARGO_PKG_VERSION")`. Use `ui.hyperlink_to` in the About dialog for GitHub and `https://www.baccan.it`.
- **Rationale**: Clear version visibility and standard OS browser opening on click.

## Risks / Trade-offs

- **[Risk] Recomputing `filtered_lines` on massive files**:
  → *Mitigation*: Recomputation only occurs when filter text actually changes or new data arrives. For files up to millions of lines, linear scanning of line offsets in Rust takes a few milliseconds and does not block rendering.
