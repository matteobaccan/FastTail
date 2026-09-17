## Why

Refine FastTail UI/UX ergonomics and fix filtering and configuration persistence bugs:
1. Modal popup windows are permanently locked to screen center and cannot be repositioned.
2. Borderless window control buttons and stream action toggle buttons visibly shift/expand in footprint on mouse hover.
3. Font size scaling below 16pt does not reduce row spacing due to an artificial 16px minimum floor, leaving large gaps between small lines.
4. Highlight rule sound alerts are lost between application sessions because they are omitted from INI persistence.
5. In Filtered view mode, empty include/exclude fields hide all non-highlighted lines instead of applying no filter constraints.
6. The UI contains redundant information: the file name and path are duplicated inside the tab pane despite already existing in the tab header, and the bottom status bar is static and cluttered with unused status/line/filter labels instead of showing the active file's absolute path.

## What Changes

- **Window Modals Draggability**: Change modal dialogs (`Settings`, `Highlights`, `About`, `Help`) to open centered by default via `.pivot(CENTER_CENTER).default_pos(...)` while allowing user dragging and repositioning.
- **Stable Hover Button Footprints**: Set `expansion = 0.0` in the cyber theme visuals and provide uniform, stable button sizing to eliminate layout jumps on hover in borderless window controls and stream toolbar toggles.
- **Proportional Font Row Height**: Remove the `.max(16.0)` row height floor in virtual scrolling, measuring row height directly from font metrics and reducing item spacing for compact line spacing at all font sizes (8pt - 32pt).
- **Sound Alert INI Persistence**: Save and restore `sound_alert` preset strings (`Beep`, `Chime`, `Warning`, `Critical`, `None`) in `fasttail.ini`.
- **Filtered Mode Field Semantics**: Ensure that empty `include_filter` and empty `exclude_filter` indicate no filtering for that criterion, and that filters only apply when non-empty.
- **UI De-duplication & Dynamic Footer**: Remove the redundant file header banner inside the tab content, clean up the bottom footer by removing static `Status`, `Lines`, and `Filter` fields, and dynamically display only the absolute path of the currently active tab.

## Capabilities

### Modified Capabilities
- `cyber-ui-docking`: Draggable modal windows, dynamic active-file footer, removed in-tab file banner duplicate, proportional row height for all font sizes, and zero-expansion hover button visuals.
- `filters-and-highlighting`: Empty include/exclude filter fields mean no filtering; persist filter sound alerts across sessions in `fasttail.ini`.

## Impact

- `src/theme.rs`: Set widget hover, active, and open expansion to `0.0`.
- `src/config.rs`: Add `sound_alert` serialization and deserialization in `to_ini` and `from_ini`.
- `src/tail_engine.rs`: Fix `is_line_visible_filtered` so empty include/exclude filters do not block lines.
- `src/ui/app.rs`: Replace `.anchor` with `.pivot(...).default_pos(...)` on modal windows; update footer to dynamically track `self.dock_state.find_active()` displaying only the active file path.
- `src/ui/dock.rs`: Remove redundant file banner from `render_stream_tab`, compute row height dynamically from font metrics, set compact row spacing, and ensure stable button sizing.
- `tests/integration_tests.rs`: Update and add unit/integration tests for sound alert persistence, filtered view with empty filters, and row height scaling.
