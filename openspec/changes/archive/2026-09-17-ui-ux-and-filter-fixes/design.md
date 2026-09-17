## Context

FastTail provides high-performance log tailing in Rust with an egui-based cyberpunk HUD. Following user testing and usage analysis, several UI ergonomics, layout stability, and state persistence issues were identified:
1. Modal popups were pinned to the center of the viewport via `.anchor()`, preventing users from repositioning them to view underlying logs.
2. Button hover states suffered from footprint jitter due to default widget expansion and unfixed dimensions.
3. Decreasing font size below 16pt did not decrease line row heights due to a hardcoded `.max(16.0)` floor, resulting in disproportionate empty space between rows.
4. Highlight rule sound alert choices were not saved or restored from `fasttail.ini`.
5. Empty include and exclude filter strings in `Filtered` view mode caused non-highlighted lines to be filtered out rather than shown unfiltered.
6. The UI contained redundant filename headers inside the log pane duplicate to the tab header, while the footer displayed static placeholders rather than the active tab's full path.

## Goals / Non-Goals

**Goals:**
- Enable draggable modal dialogs by replacing static anchors with centered pivots and default positions.
- Eliminate UI footprint jumps on button hover by setting `expansion = 0.0` and ensuring uniform button layout footprints.
- Enable proportional row heights for small font sizes (down to 8pt) without arbitrary minimum floors.
- Persist `sound_alert` presets for each highlight rule in `fasttail.ini`.
- Ensure empty filters represent no constraints in all view modes.
- Remove redundant internal tab banners and display the active tab's full filesystem path dynamically in the bottom footer.

**Non-Goals:**
- Completely rewriting the docking system or replacing `egui_dock`.
- Adding new audio codecs or external sound playback engines.
- Modifying the underlying `mmap` streaming performance or indexing algorithm.

## Decisions

### Decision 1: Draggable Modal Windows via Pivot & Default Pos
- **Choice**: Replace `.anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))` with `.pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center())` across all dialogs (`Settings`, `Highlights`, `About`, `Help`, `BareTail`).
- **Rationale**: In `egui`, calling `.anchor()` locks the window to a fixed screen coordinate on every frame, ignoring user drag operations. Using `.pivot(...).default_pos(...)` ensures the dialog centers upon first opening while retaining operator-dragged positions.
- **Alternatives considered**:
  - *Hardcoding window offsets*: Fails across different screen resolutions.
  - *Keeping `.anchor()`*: Prevents reading log lines hidden beneath dialogs.

### Decision 2: Zero Expansion for Hovered and Active Widgets
- **Choice**: Explicitly configure `visuals.widgets.hovered.expansion = 0.0`, `visuals.widgets.active.expansion = 0.0`, and `visuals.widgets.open.expansion = 0.0` in `CyberTheme::apply`.
- **Rationale**: Egui defaults to inflating widget boundaries by 1.0px on hover, which causes adjacent toolbar buttons and borderless window control icons to visibly shift or twitch. Zero expansion ensures a rock-solid, professional HUD layout.
- **Alternatives considered**:
  - *Adding outer padding wrappers to all buttons*: Verbose, fragile, and does not fix window control buttons.

### Decision 3: Proportional Virtual Row Height from Font Metrics
- **Choice**: Remove `.max(16.0)` from `render_log_stream` and `render_hex_stream`, deriving line height directly from `(font_size * 1.35)` or font metrics with compact spacing.
- **Rationale**: Eliminates excessive vertical gaps when viewing logs at 8pt, 10pt, or 12pt monospace fonts, maximizing the number of visible lines on screen.
- **Alternatives considered**:
  - *Fixed row height table*: Inflexible when custom font sizes or OS DPI scaling are applied.

### Decision 4: Sound Alert INI Serialization
- **Choice**: Add `sec.set("sound_alert", rule.sound_alert.name())` in `FastTailConfig::to_ini` and deserialize with `SoundAlertPreset::from_name` in `FastTailConfig::from_ini`. Default to `SoundAlertPreset::None` if unspecified.
- **Rationale**: Matches existing RGB and boolean serialization patterns in `fasttail.ini` without breaking backward compatibility with older INI files.
- **Alternatives considered**:
  - *Numeric integer serialization*: Less readable and brittle if enum variants are reordered.

### Decision 5: Non-Blocking Empty Filter Semantics in Filtered Mode
- **Choice**: In `TailEngine::is_line_visible_filtered`, treat an empty `include_filter` as allowing lines to pass (subject to non-matching `exclude_filter`), while keeping highlight rule matches visible.
- **Rationale**: An empty search/filter box naturally represents "no filter applied", matching user expectations across command-line and GUI log tools.
- **Alternatives considered**:
  - *Requiring at least one highlight rule*: Counterintuitive and hides all content when opening a clean log.

### Decision 6: Tab De-duplication and Dynamic Footer Path
- **Choice**:
  1. Remove the file header banner from `render_log_stream`.
  2. Query `self.dock_state.find_active()` to dynamically obtain the active tab's file path, convert to absolute path, and render in the bottom status panel without static status/filter text.
- **Rationale**: The tab bar already clearly identifies the open file and stream state. Reclaiming vertical space allows more log lines to be visible, and showing the full path in the footer provides instant context for deeply nested files.

## Risks / Trade-offs

- **[Risk] Smaller row heights causing text clipping**:
  → *Mitigation*: Use a minimum multiplier of `1.3` to `1.35` times font size to safely accommodate font ascenders, descenders, and accent marks.
- **[Risk] Window opening off-screen after resolution change**:
  → *Mitigation*: `egui` automatically clamps window rectangles within screen bounds if the viewport shrinks.
