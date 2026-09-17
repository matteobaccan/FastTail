## 1. Tail Engine & Filter Virtualization

- [x] 1.1 Add `filtered_lines: Vec<usize>` and `recompute_filtered_lines` in `src/tail_engine.rs`
- [x] 1.2 Implement `search_matches`, `current_match_idx`, `search_next`, and `search_prev` with wrap-around beep in `src/tail_engine.rs`
- [x] 1.3 Add `search_history: Vec<String>` to `FastTailConfig` with `to_ini` and `from_ini` persistence in `src/config.rs`

## 2. UI & UX Implementation

- [x] 2.1 Set `CursorIcon::Move` on titlebar draggable areas in `src/ui/app.rs`
- [x] 2.2 Display version in window titlebar header and add clickable links to GitHub and `https://www.baccan.it` in About dialog in `src/ui/app.rs`
- [x] 2.3 Remove "All / Filtered" button and virtualize `show_rows` over `visible_line_count` in `src/ui/dock.rs`
- [x] 2.4 Implement F3 / Shift+F3 shortcuts and jump-to-line scrolling in `src/ui/dock.rs`
- [x] 2.5 Render active search match line highlight with `▶` pointer and match counter (`[X / Y]`) in `src/ui/dock.rs`
- [x] 2.6 Add search history dropdown (`🕒`) with recent 10 searches in `src/ui/dock.rs`

## 3. Verification & Build

- [x] 3.1 Add integration tests for filter virtualization and complete exclusion across all lines
- [x] 3.2 Add integration tests for search navigation, wrap-around detection, and search history INI persistence
- [x] 3.3 Run full test suite to verify zero regressions
- [x] 3.4 Build standalone Windows release binary (`fasttail.exe`)
