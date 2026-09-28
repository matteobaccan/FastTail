# FastTail — Graphical Interface Design Document

Describes the desktop UI of FastTail as of the 0.12.0 development cycle: v0.11.0 plus the global filter (#118), show in context (#122), more archive formats (#126), bookmark notes and triggers (#127) and the collapse of repeated lines (#128). Everything here comes from the source code; each section names the files it describes. Keep this document in step with the UI when it changes.

Stack: Rust, `eframe` / `egui` 0.36, `egui_dock` 0.21 (with `serde`), `egui_commonmark` 0.25 for the Markdown view, and `rfd` for the native file dialogs.

---

## Table of contents

1. [Overview and architecture](#1-overview-and-architecture)
2. [Main window](#2-main-window)
3. [The dock and the stream tabs](#3-the-dock-and-the-stream-tabs)
4. [Secondary windows, dialogs, popups and context menus](#4-secondary-windows-dialogs-popups-and-context-menus)
5. [Graphic elements inventory](#5-graphic-elements-inventory)
6. [Themes](#6-themes)
7. [Keyboard and mouse interaction model](#7-keyboard-and-mouse-interaction-model)
8. [Internationalization](#8-internationalization)
9. [Configuration that affects the UI](#9-configuration-that-affects-the-ui)
10. [Design conventions, principles and known limitations](#10-design-conventions-principles-and-known-limitations)

---

## 1. Overview and architecture

Sources: `src/main.rs`, `src/ui/mod.rs`, `src/ui/app.rs`, `src/renderer.rs`

### 1.1 Module map

| Module | Role |
|---|---|
| `src/ui/mod.rs` | Declares the UI modules and re-exports `FastTailApp`. |
| `src/ui/calendar.rs` | The month calendar of the time range popup: a 7×6 grid of days starting on Monday, with month and year arrows, on the civil-date functions of `src/timestamp.rs`. |
| `src/ui/app.rs` | `FastTailApp`, the `eframe::App`. It owns the config, the engines and the dock state. It draws the title bar, toolbar, banners, status bar and every free-floating dialog, and handles the global shortcuts, persistence, lock and screensaver. |
| `src/ui/dock.rs` | `FastTailTab`, `DockContext`, `FastTailTabViewer` (the `egui_dock::TabViewer`). Draws one stream (stream bar, filter row, rows in text / wrapped / HEX / Markdown), plus the bodies of the Filters, Color Filters and Settings panels. |
| `src/ui/find_results.rs` | The "Find results" dock tab: a search across all streams. |
| `src/ui/global_filter_bar.rs` | The global filter bar shown under the toolbar. |
| `src/ui/hit_list.rs` | Virtualized hit lists: `HitList` (a stream's results pane) and `GroupedHitList` (Find results). |
| `src/ui/overview_strip.rs` | The 10 px minimap beside the rows' scroll bar. |
| `src/ui/time_range.rs` | The time range control of the stream bar (the visible time span) and its popup: the draft, the calendar and spinner rules, the shortcuts and OK / Cancel. |
| `src/ui/timeline_strip.rs` | The 56 px timeline histogram above the rows. |
| `src/ui/zip_picker.rs` | `ArchivePicker`, the entry picker for a zip that holds several files and for any tar archive (plain or compressed). |
| `src/collapse.rs` | Not UI code, but it shapes the rows: detection of repeated entries (`CollapseMode`, `CollapseState`) and the row ↔ line mapping of a collapsed text view. |
| `src/theme.rs` | `CyberTheme`: palettes, level, label and ANSI colours, and `apply()` to egui `Visuals`. |
| `src/renderer.rs` | Backend choice (`auto`, `glow`, `wgpu`, `software`) and the description of the active backend (`ActiveRenderer`). |
| `src/screensaver.rs` | The Matrix "digital rain" screensaver, painted over the whole window. |

### 1.2 The application struct

`FastTailApp` (`src/ui/app.rs`) holds all UI state:

- `config: FastTailConfig`: the persisted preferences (see §9). Many UI toggles live here directly, such as `settings_open`, `filters_open`, `help_open`, `about_open` and the dialog positions and sizes.
- `engines: Vec<TailEngine>`: one engine per open stream. A tab refers to its engine only by path (`FastTailTab::LogStream(PathBuf)`).
- `dock_state: DockState<FastTailTab>`: the `egui_dock` tree, including floating window surfaces.
- UI-only state:
  - `screensaver`, plus the lock fields `locked`, `lock_entry`, `lock_failed`, `lock_attempts`.
  - `system` (sysinfo), `cpu_usage` and `mem_used_mb` for the telemetry meters.
  - `quick_labels`: memory only.
  - `pattern_prompt`, `archive_picker`, `open_notice`, `save_notice`, `pending_session_load`, `session_missing`: one-shot dialogs.
  - `find_all`: the cross-stream search session.
  - `global_spec`, `global_edit_at`, `global_key`: the compiled global filter and its debounce state.
  - `renderer: ActiveRenderer`, `applied_visuals`, and the frame-pacing timestamps.
  - `floating_window_rects`: rectangles of undocked dock windows, captured for persistence.
  - `session_saved`, `session_dirty`, `title_applied`: title-bar and session tracking.

### 1.3 Frame loop

`eframe::App::ui()` does frame pacing first (§1.5), then calls `render_ui()`. In order, each frame:

1. Paints the theme background over the whole canvas. `clear_color()` also returns `theme.bg_color()`.
2. Applies the always-on-top level when the config and the viewport disagree.
3. On the first frame, re-applies maximized or minimized.
4. While locked, drops every input event the workspace could act on (`allowed_while_locked`) before anything reads input.
5. Reads the input once:
   - It tracks the viewport geometry, maximized and minimized state, and user activity (for the screensaver).
   - It handles `F1`, `CTRL + SHIFT + T` and `Esc` (which closes the topmost of Settings, Filters, About and Help), dropped files and folders, and `CTRL + wheel` zoom.
6. Every 2 s, calls `save_dock_layout()`, which also saves the config.
7. At most once per second, checks whether the named session has unsaved changes, and updates the OS window title.
8. Polls the pending stdin stream, then calls `poll_updates()` on every engine, then runs the tools bound to highlight rules, then polls `find_all`.
9. Refreshes the telemetry once per second.
10. Stores the zoom factor and handles the PIN-lock triggers (`CTRL + L`, or the end of the screensaver).
11. Checks for screensaver inactivity and schedules the next repaint (§1.5).
12. Re-applies the visuals when the theme or the software-renderer flag changes.
13. Draws the panels:
    - `title_panel`, `toolbar_panel` and, on a software renderer, `software_banner` (all top panels).
    - `status_bar` (bottom panel).
    - The global filter bar, if it is open.
    - The dock, or the empty-workspace placeholder.
14. Applies requests the dock left behind: Find results jumps, `find_all_request` from a stream bar, preset events, closed tabs, changed bookmarks and bookmark notes, wrap, ANSI mode, timeline and collapse-mode flags, search-view preferences, time-delta preferences, and the OS attention request.
15. Draws the free-floating dialogs:
    - BareTail import, Settings, Filters / Color Filters, About, Help.
    - The Open pattern prompt, the session confirmation and "missing" dialogs, the archive picker (which pulls the rows its tar scan found before it draws), and the compressed-open and session-save notices.
16. Draws the screensaver, then the lock overlay on top of everything.
17. In borderless mode (not maximized, not locked), adds the resize zones and the 1 px frame with the corner grip.

### 1.4 How the UI talks to `TailEngine`

The UI is immediate mode and never blocks on file I/O it can avoid:

- **Reading.** Each frame the UI asks the engine only for what is on screen: `visible_line_count()`, `get_actual_line_idx(row)`, `get_row(line)` / `get_line(line)` (served from the engine's block cache), `get_bytes()` for HEX, `level_of()`, `row_time_delta()` and so on. Rows are virtualized with `ScrollArea::show_rows` in the plain text and HEX views, and with a custom anchored layout in the wrapped view (§3.5).
- **Writing.** Setters change the engine's view state: `set_include_filter`, `set_filter_terms`, `set_min_level`, `set_view_mode`, `set_encoding`, `set_ansi_mode`, `set_wrap_lines`, `apply_time_range_text`, `toggle_bookmark`, `set_bookmark_note`, `enter_context` / `leave_context`, `set_collapse_mode` / `toggle_collapsed_row`, `set_global_filter`, `set_highlight_rules`, `set_quick_labels` and similar. Heavy work (indexing, filtering, searching, level and timestamp scans, automatic bookmarks, and the collapse detection on a file above 16 MB) runs on worker jobs inside the engine. The UI only draws progress from `scan_progress()`.
- **Scroll requests.** The UI sets fields such as `requested_scroll_y`, `requested_scroll_x`, `scroll_to_line`, `wrap_request` or `pending_jump`. The next render consumes them.
- **Dirty flags.** The engine raises `bookmarks_dirty`, `wrap_dirty`, `ansi_dirty`, `timeline_dirty` and `collapse_mode_dirty`. The app persists the change and clears the flag after the dock is drawn.
- **Activity.** Each frame the app resets `engine.displayed` to false, and the tab viewer sets it back for the tabs on screen. Hidden tabs therefore accumulate `unseen_lines` and `unseen_severity`, which drive the tab badge and the taskbar flash.
- **Waking.** Every engine gets a `WakeFn` (`make_wake`) that calls `ctx.request_repaint()` from its filesystem watcher thread. An idle window repaints only when data actually arrives.

### 1.5 Rendering backend and repaint strategy

`src/main.rs` and `src/renderer.rs`:

- **Backend choice.** The `--renderer` command-line option wins, then the `FASTTAIL_RENDERER` environment variable, then `renderer=` in `fasttail.ini`, then `auto`.
  - `auto` starts **wgpu** first (Direct3D 12, Vulkan or Metal) and retries with **glow** (OpenGL) if eframe fails before the app was created.
  - `software` forces wgpu onto a CPU adapter (WARP or llvmpipe) through a custom adapter selector, and falls back to glow the same way.
  - `glow` and `wgpu` force that backend, with no fallback.
- **Present options.**
  - glow: vsync is **off**.
  - wgpu: `AutoVsync` on a GPU, `Immediate` on the software path. `desired_maximum_frame_latency` is 2.
- **`ActiveRenderer`.** Read from the creation context. It gives the status-bar chip (`GL`, `WGPU`, with ` fallback` appended when the fallback was used) and the details string (`Dx12 · adapter · driver`). `is_software()` matches on the adapter or driver name (WARP, llvmpipe, "Basic Render", VMware, Hyper-V, virtual, QEMU, VBox, RDP, …).
- **Software-renderer visuals** (`apply_renderer_visuals`): no feathering (anti-aliasing), no window or popup shadows, zero corner radius, no hover expansion. A warning banner is shown under the toolbar.
- **Repainting.** egui repaints on demand only. The app schedules repaints explicitly:

  | Situation | Repaint cadence |
  |---|---|
  | Any engine is monitoring | every `poll_interval_ms` (default 250 ms) on a GPU; every **2 s** safety poll on a software renderer. Real data arrives earlier through the watcher wake. |
  | Screensaver armed and window focused | every 1 s, for the inactivity check |
  | Screensaver running, lock backdrop animating | every 33 ms (`FRAME_INTERVAL`) |
  | Lock cooldown countdown | ≤ 250 ms |
  | Background scan progress (including the collapse detection and automatic bookmarks), time range pending, decompression progress, tar scan in the archive picker, timestamp timing, Find all running | 100 ms |
  | Search typing debounce | 150 ms after the last keystroke |
  | Global filter term edits | applied 300 ms (`APPLY_DELAY_MS`) after typing pauses |
  | Overview strip and timeline cache rebuilds while a stream grows | throttled to 250 ms |
  | Transient notices ("switched to", "ANSI switched") | 500 ms |
  | Find results snapshot age | 1 s under a minute, then 30 s |

- **Frame pacing** (`FastTailApp::ui`):
  - A frame caused only by pointer movement sleeps until `mouse_throttle_interval_us` has passed. That is `mouse_throttle_ms` (default 100 ms) on a GPU, and `min(200 ms, mouse_throttle_ms)` on a software renderer (`SOFTWARE_MOUSE_FPS = 5`).
  - Every other frame is capped at `max_fps` (default 60) on a GPU and at `max_fps_software` (default 30) on a software renderer.

### 1.6 Persistence of session and layout

- **Dock layout.** Serialized as RON into `[dock] layout`.
  - Floating dock windows get their current rectangle first. It is captured from egui memory under the id `"window {surf_index:?}"`.
  - The Find results tab and the `stdin` tab are removed before saving.
  - `save_dock_layout()` runs every 2 s, when a tab is closed or a file opened, on close request and in `on_exit`.
  - `FastTailConfig::save` writes the file only when its bytes changed (`write_if_changed`).
  - Stale floating-window indices are pruned (`prune_floating_window_rects`). A stale index crashed v0.1.0.
- **Workspace.** `open_files`, and per-stream state (filters, search, encoding, ANSI mode, timeline flag, collapse mode) in `[session]` / `[stream_N]` sections of `fasttail.ini`. Manual bookmarks and their notes go in `[bookmarks]` and wrap flags in `[wrapped_files]`. Automatic bookmarks are not saved: they are recomputed from the rules.
- **Named sessions.** Saved to `*.fasttail-session.ini` files from the 🗂 menu.
  - The title shows ` · name`, plus `*` when the live workspace differs from the file. The comparison uses a fingerprint that describes the dock by structure, not by geometry (`dock_signature`), and is checked at most once per second.
  - Loading a session over unsaved changes asks for confirmation.
- **Window and dialog geometry.**
  - Stored in `[window]`: x, y, width and height, recorded only when the window is neither maximized nor minimized and at least 400×300, plus the maximized and minimized flags.
  - Stored in `[dialogs]`: the open flags and the position and size of Settings, Filters, About and Help.
- **Zoom factor.** Restored before the first frame.

---

## 2. Main window

Sources: `src/main.rs`, `src/ui/app.rs`

### 2.1 Viewport setup

| Property | Value |
|---|---|
| Title | `FastTail v{CARGO_PKG_VERSION} by Matteo Baccan`. At runtime ` · {session}` and `*` are appended when a named session is active (`ViewportCommand::Title`, sent only when the text changes). |
| Initial inner size | `window_width` × `window_height` from `[window]`, else **1280 × 800** |
| Minimum inner size | **800 × 500** |
| Position | `[window] x,y` when the point (x+40, y+20) is on an attached monitor (Win32 `MonitorFromPoint`, DPI-scaled); otherwise the OS decides |
| Maximized / minimized | Restored from `[window]`. Maximize is set on the builder and again on the first frame; minimize only on the first frame, so the saved geometry is laid out first. On Windows `ShowWindow` is also called. |
| Decorations | `with_decorations(!borderless)`, switched live with `ViewportCommand::Decorations` |
| Drag and drop | Enabled (`with_drag_and_drop(true)`) |
| Icon | The title-bar badge (dark disc, cyan ring, cyan "F"), embedded as raw 128 × 128 RGBA (`assets/icon-128.rgba`, drawn from `assets/icon.png`) and set with `with_icon`. The executable file itself carries no resource icon. |
| Always on top | `ViewportCommand::WindowLevel(AlwaysOnTop)` when `always_on_top` is set |
| Subsystem | `#![windows_subsystem = "windows"]`. `--help` and `--version` attach to the parent console. |

### 2.2 Layout top to bottom

There is **no classic egui menu bar** and **no left or right side panel**. The window is a stack of `egui::Panel`s around the dock:

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ (F) FASTTAIL v0.11.0 by Matteo Baccan · proj*  ░drag░  🖥 CPU: 7% ▬ │ RAM: 5.1 GB/32 GB ▬ │
│                                                         🔍 100%  📌  🌐 │ —  🗖  ✕       │ ← title_panel (always)
├──────────────────────────────────────────────────────────────────────────────────────┤
│ [📁 Open File][🕒][📂*][🗂][⚡ Color Filters (3)][▶ Play][⏸ Pause][⚙ Settings]  [ℹ About][❓ Help] │ ← toolbar_panel
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ⚠ Software mode: rendering runs on the CPU ... A GPU is required ...                  │ ← software_banner (only on WARP/llvmpipe/VM)
├──────────────────────────────────────────────────────────────────────────────────────┤
│ 🌐 Global filter [✓]On Aa .* │ All of: [____][✖][+] │ None of: [____] │ ✖            │ ← global filter bar (CTRL + SHIFT + H / 🌐)
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ╭[#1] ▶ app.log ● [12]╮╭[#2] ■ db.log ○╮╭🔎 Find results ⏳╮                          │ ← egui_dock tab bar (26 px)
│ │ ▶ Follow │ ▶ Monitor │ 🔤 TXT 🔢 HEX 📝 MD │ # 123 Δt ↩ Wrap [UTF-8▾][ANSI: auto → render▾][× Collapse: exact▾] │ Lines: 9,812 · 9,640 rows shown │ 🕘 14:02:05 → 16:30:12 │ … 🔍[search]🔎 [3/57]▲▼☰🕒✖ │ ✏ Bookmark note, line 1233: [____] │ 💾 │ ← stream bar
│ │ ⚡ Include (Regex): [_____]✖ +2 + │ 🚫 Exclude: [_____] + │ 📊 🔍 │ Aa .* │ ≥ WARN▾ ? │ Presets ▾ 🌐 │ ← filter row (45 % opacity in context)
│ │ 🏷 Labels:  1 timeout ✕  4 req=42 ✕                                                   │ ← quick labels strip (if any)
│ │ ◆ Filters suspended: line 1234 shown in the full log  [Back to filtered view]         │ ← context banner (only in the context view)
│ │ ▁▂▅█▃▁▁▂▇▂▁  peak 812                                                                 │ ← timeline strip (📊, 56 px)
│ │ ✏   1233 │  +0.012  2026-09-27 14:02:01 INFO ...                                   ┃▌│ ← rows + overview strip (10 px)
│ │ ▶   1234 │  +1.402  ×57 2026-09-27 14:02:02 ERROR timeout ...                      ┃ │ ← a collapsed group: ×N badge
│ │ ☆   1291 │  +0.300  2026-09-27 14:02:04 WARN retry ...                             ┃ │ ← automatic bookmark (rule)
│ │ ☰ Matches for "timeout": 57                                                     ✖  │ ← search results pane (resizable)
│ │     1234│ ... timeout ...                                                          │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ▬ C:\logs\app.log                                                              WGPU  │ ← status_bar
└──────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.3 Title bar (`title_panel`)

This is a custom title bar, drawn in **both** decorated and borderless mode. In decorated mode it therefore sits under the OS title bar. Fill is `bg_color`, inner margin 8/4.

- **Left side**
  - A 22×22 painted badge: a circle of radius 10, fill `#DCEBFC` on Light and `#0A1A28` on the dark themes, 1.5 px accent stroke, and an "F" in 12 pt monospace in the accent colour.
  - The label `FASTTAIL v… by Matteo Baccan · session*`, 13 pt monospace, strong, accent. It is click-and-drag: dragging it moves the window (`ViewportCommand::StartDrag`) and the cursor becomes Move.
- **Right side** (right-to-left layout). Listed here from the right edge inward:
  1. Borderless only: **✕** Close (warn colour; saves layout and config, then `std::process::exit(0)`), **🗖/🗗** Maximize/Restore, **—** Minimize, then a separator.
  2. **🌐** Global filter bar toggle: accent while the global filter applies, dim otherwise.
  3. **📌** Always-on-top toggle: accent when on.
  4. **`🔍 {zoom}%`**, 10.5 pt: dim at 100 %, accent otherwise. A click resets the zoom to 100 %.
  5. Separator. When `telemetry_enabled` is on: RAM meter (44×6 bar, fill `secondary_accent`) with `RAM: x.x GB/y GB`, then the CPU meter (bar in the accent colour) with `🖥 CPU: n%`.
  6. The remaining width is a **drag region**. Dragging moves the window, a double-click toggles maximize, and the cursor is Move.

### 2.4 Toolbar (`toolbar_panel`)

Fill is `panel_bg`, with a 1 px stroke of accent at 25 % and inner margin 10/6. The buttons are 26 px high with corner radius 6.

| Button | Style | Action |
|---|---|---|
| `📁 {Open File}` | fill `button_bg`, 1.2 px accent stroke, strong primary text | Native multi-file dialog "Open Log Files" with the filter `Log Files (*.log, *.txt, *.*)` |
| `🕒` (30 px) | same | Menu: recent files (`📄 name (full path)`, at most 15 kept), separator, `🗑 Clear recent files history` (warn). Shows *No recent files* in italics when empty. The tooltip is the localized "Recent Files". |
| `📂*` (30 px) | same | Opens the **Open pattern** prompt, prefilled with `<dir of most recent file>\*.log` |
| `🗂` (30 px) | same | **Sessions** menu (§4.12) |
| `⚡ {Color Filters} (N)` | warn text and stroke; fill `#FEF9EB` on Light, `#14120C` on dark | Toggles the Filters / Color Filters window. N is the number of enabled rules with a non-empty pattern. |
| `▶ Play` | green `#00C864` text and stroke; fill `#EBFCF2` / `#0A1812` | Every engine: `is_watching = true`, and follow on unless the stream is compressed |
| `⏸ Pause` | red `#EB2D4B`; fill `#FEF0F2` / `#1A0C10` | Every engine: stop watching and stop following |
| `⚙ {Settings}` | dim text, 1 px dim stroke | Toggles the Settings window |
| *(right-aligned)* `ℹ About`, `❓ Help` | dim | Toggle About and Help (Help is also `F1`) |

The labels of `▶ Play`, `⏸ Pause`, `❓ Help` and `ℹ About` and their tooltips are localized.

### 2.5 Software-renderer banner

`Panel::top("software_banner")`, shown only when `renderer.is_software()`. Fill is warn at 18 % with a warn stroke at 80 %. It reads `⚠ {software_banner}` in strong warn monospace.

### 2.6 Global filter bar

Source: `src/ui/global_filter_bar.rs`

The bar is drawn inline in the central area (not as its own panel), above the dock, when `global_filter.bar_open` is set. `CTRL + SHIFT + H` or the 🌐 button opens it. It is a `horizontal_wrapped` row, followed by a separator:

- `🌐 {Global filter}`, strong. Accent when applied, dim otherwise.
- An `[On]` checkbox, and `Aa` (case) and `.*` (regex) selectable labels. Changes apply at once.
- `All of:` (accent) include term fields, each 140 px wide, with `✖` and `+`. Then `None of:` (warn) exclude fields. At most `MAX_FILTER_TERMS` per side.
- A regex that does not compile is drawn in the warn colour with a tooltip. The check is cached by a hash of the terms, so it does not recompile every frame.
- A final `✖` hides the bar. Hiding does **not** switch the filter off.
- Typing applies after a 300 ms pause.
- Streams take the compiled set before they are drawn. At most 32 MB of small-file refiltering (`SYNC_BUDGET_BYTES`) runs on the UI thread per frame; the rest continue on following frames.

### 2.7 Status bar (`status_bar`)

Fill is `panel_bg`, with a 1 px stroke of accent at 35 % and inner margin 12/5.

- **Left**
  - A 28×3 accent indicator bar.
  - The absolute path of the focused stream, or the first stream, in 11 pt primary text. For stdin it reads `standard input (spooled to {path})`; with no file it reads *No file open*.
- **Right**
  - The renderer chip (`GL`, `WGPU`, `… fallback`) in 10 pt, `secondary_accent`, or warn when the fallback was used. The tooltip is `{renderer_tip}\n{details}`.

Per-stream figures (lines, size, throughput, level counts) are in the **stream bar**, not the status bar.

### 2.8 Empty workspace

When the dock has no tab, a centred placeholder replaces it:

- 📂 at 48 pt.
- The *No file open. Drag & drop …* text.
- A large `📁 Open File` button, 160×32 with an accent stroke.
- Up to five recent-file chips (`📄 name`, `secondary_accent`, full path as tooltip).

### 2.9 Borderless mode

When `borderless` is on and the window is not maximized:

- There are 8 px resize edges. The corners are 12 px squares, except the bottom-right corner, which is 16 px. They are implemented with `ViewportCommand::BeginResize` and resize cursors, and are disabled while locked.
- The 140×40 top-right zone is excluded so the window buttons stay clickable.
- A 1 px `border_color` frame goes around the window.
- A diagonal four-line grip in accent at 80 % sits in the bottom-right corner.

---

## 3. The dock and the stream tabs

Sources: `src/ui/dock.rs`, `src/ui/app.rs`, `src/ui/find_results.rs`, `src/ui/hit_list.rs`, `src/ui/overview_strip.rs`, `src/ui/timeline_strip.rs`

### 3.1 `egui_dock` usage

- `DockArea::new(&mut dock_state).style(dock_style).show_inside(ui, &mut FastTailTabViewer)`.
- The tab type is `FastTailTab`:
  - `LogStream(PathBuf)`: one per stream.
  - `FindResults`: the cross-stream search results.
  - `Filters`, `Highlights`, `Settings`: the viewer can still render these (it calls the same content functions as the dialogs), but **no code path creates them any more**. They could only come back from an old saved layout; today these panels are floating `egui::Window` dialogs.
- **New stream tab.** `push_to_first_leaf` on the main surface, or a fresh `DockState` when the dock is empty. A second open of the same path only activates the existing tab.
- **Find results tab.** Opened by `CTRL + SHIFT + F` or 🔎. It is split **below** the first leaf at 0.62 and focused. If it already exists it is only focused.
- **Split, drag, float and close.** This is stock `egui_dock` behaviour: drag a tab to split or re-dock it, or drop it outside to make a floating window surface. FastTail does not add a custom tab context menu or an "add tab" button.
- **Closing.**
  - `on_close` on a stream removes its engine and its entry in `open_files`, then saves the layout. Dropping the engine stops its jobs and deletes a compressed spool.
  - Closing Find results cancels every search job.
- **Dock style.** See §6.4.

### 3.2 Tab title and badges

`TabViewer::title` for a stream builds:

```
[#<engine index+1>] <▶ watching | ■ paused> <name> <● new data | ○ none>[ [N] | [999+] ]
```

- `<name>` is one of:
  - the file name;
  - `pattern ▸ current-file` for a wildcard stream;
  - `archive › entry` for a compressed stream;
  - `stdin`;
  - `name (Closed)` when no engine backs the tab.
- `[N]` counts the lines appended while the tab was not displayed, capped at `[999+]`. Its colour is the maximum severity of those lines: 2 (error) → warn, 1 → accent, 0 → `secondary_accent`, all strong.
- Otherwise:
  - new data → warn, strong;
  - paused → warn;
  - normal → the dock's active or inactive text colours.
- **Tooltip** (`on_tab_button`): the archive path and `› entry` for a compressed stream, or the spool path for stdin.
- **Find results title:** `🔎 Find results`, followed by ` ⏳` while jobs run.

### 3.3 Stream bar (first row of a stream)

One `ui.horizontal` row, in this order. `|` stands for a separator.

1. **`▶ Follow` / `■ Follow`** toggle (Space, `CTRL + END`). It is disabled on compressed streams, with a tooltip saying why.
2. **`▶ Monitor` / `■ Monitor`**: disk watching on or off.
3. **View mode**: `🔤 TXT` (accent), `🔢 HEX` (`secondary_accent`), `📝 MD` (warn). MD is refused above `markdown_max_mb` with a notice.
4. Text views only:
   - **`# 123` / `# ---`**: line numbers (global).
   - **`Δt`**: time-delta column (global). The tooltip explains when timestamps are unreadable.
   - **`↩ Wrap`**: per stream, `ALT + W`.
5. Text and MD only:
   - **Encoding** combo, 90 px: UTF-8, ASCII, ANSI, Unicode, Unicode BE.
   - **ANSI** combo: `ANSI: auto → render`, `render`, `strip`, `raw`.
   - Text only (not MD): the **Collapse** combo (`render_collapse_selector`), whose closed text reads `× Collapse: off|exact|numbers` in 11 pt. Its entries are `off`, `exact` (equal text after the leading timestamp, trailing whitespace ignored) and `numbers` (as exact, with numbers, hex values and ids masked). It is per stream, `CTRL + SHIFT + D` cycles it, and the tooltip explains the modes. Picking a mode detects the groups again from scratch and keeps the line at the top of the view in place.
6. HEX only: **`Hex columns: [-8] N [+8]`**, range 8–64.
7. **`Lines: v / t`** (dim) when rows are filtered, otherwise `Lines: t`. When repeated entries are collapsed and the rows are fewer than the visible lines, ` · R rows shown` follows. In HEX it shows the hex row count.
8. **`🕘 from → to`**, text views only: the time span of the visible lines, and the **time range control** (`time_range::control`). It is a frameless button in 11 pt monospace, underlined under the pointer, with a pointing-hand cursor; a click opens or closes the time range popup (§4.15). The date is written once when both ends share it (`🕘 2026-09-18 14:02:05 → 16:30:12`); a label over 40 characters drops the seconds (`🕘 2026-09-18 14:02 → 2026-09-19 16:30`). Without a span it reads `🕘 … N%` while the stream is being timed, `🕘 no timestamps` (dim) on a timed stream without usable timestamps, and `🕘 —` when no visible line carries a time. Colours: warn while a side of the window cannot be read (`time_range_error`), accent while a window narrows the view, dim for *no timestamps*, `text_primary` otherwise; ` ⏳` follows while the window waits for the timing. The tooltip gives the full span, the window as typed, the pending or invalid state and *Click to set the time range*.
9. **`Δ +2.357 · 14 rows`**: elapsed time of a multi-row selection, in accent.
10. **Per-level counters**, most severe first, only for levels seen: `FTL n  ERR n  WRN n  INF n  DBG n  TRC n`, each in its `level_color`.
11. **`⏳ {indexing|filtering|searching|detecting levels|timing lines|finding auto-bookmarks|collapsing} N% (hits)`** in warn, while a background scan runs.
12. **`ⓘ notice`** in warn:
    - `ⓘ auto-bookmarks capped at N` while the rules match more lines than `auto_bookmark_max`. The tooltip says that only the first matches in file order are bookmarked, and where the limit is set.
    - `view_notice` (also export failures and tool-run failures) and "ANSI switched".
13. **Compressed status**: `🗜 decompressing N%` with `✖` cancel; then `🗜` (done) or `🗜 partial content (why)`; `⟳` re-extract. The tooltip is the archive path, plus `› entry` for an archive entry. When nothing was written, or the reason is a tar, the text is `🗜 why` without "partial content". The reasons (`StopReason`) are:
    - *stopped by the user*; *output cap of {size} reached*; *less than 512 MB would remain free on {volume}*;
    - *this file holds a tar archive: open it again to choose its entries* (a codec file that turned out to hold a tar);
    - *the archive no longer holds this entry*;
    - a tar entry refused when it is extracted: *link or special file, not supported*, *sparse file, not supported* (the picker's refusal texts);
    - *the decoder window exceeds the 256 MiB limit* (an xz dictionary or zstd window over `MAX_DECODER_WINDOW`);
    - *decompression failed: {error}*.
14. **Stdin status**: `⏹ input ended · N lines`, `⚠ cannot read…`, `ⓘ earlier input discarded…`.
15. **Pattern stream**: `📂* glob ▸ file` (`secondary_accent`), or `▸ waiting…` in warn, followed by `ⓘ switched to name`.
16. **`📦 size`** button, which cycles Bytes → MB → GB → Hex (`0x…`). The chosen unit applies to every stream.
17. **`Throughput: x.x KB/s`** when greater than 0.
18. **Search**:
    - A 🔍 label, then the search field, whose width is clamped to 160–360 px. Its hint is *Search buffer (F3 next, Shift+F3 prev)…*.
    - `🔎` (search every stream).
    - `[cur / total]` (accent) or `[0 / 0]` (warn), plus a "first 1,000,000 listed" note when capped.
    - `▲` / `▼`, `☰` (results pane toggle), `🕒` (history menu), `✖` (clear).
19. **Go to line** (`CTRL + G`): `⇢ Go to line: [____]`, 90 px. The hint is `line, +N, -N, 14:02`. It shows a notice (hidden or invalid) or `⏳ timing lines N%` while a time jump waits. A jump to a line hidden inside a collapsed group expands that group (`reveal_line`).
20. **Bookmark note editor**, only while it is open (row context menu → `✏ Bookmark note…`): a separator, `✏ Bookmark note, line N:` in 11 pt accent, then a 260 px single-line field with the hint *one line, Enter saves, ESC cancels*, limited to 200 characters (`MAX_NOTE_CHARS`). It takes focus on the frame after it opens and is prefilled with the current note. `Enter` saves, `Esc` cancels. Saving a note bookmarks the line (an automatic bookmark becomes a manual one); saving an empty text removes the note and keeps the bookmark. Line breaks and tabs become spaces, and the text is trimmed.
21. **`💾`** menu (§4.12).

Because it is a single row, the bar can grow wider than the panel on narrow windows. The rows area and the overview strip are clipped to the visible rectangle to compensate.

### 3.4 Filter row (second row)

Shown in the text and wrapped views. Markdown shows it only while a search is active, with a `📝 Search active: source lines` label above it. HEX does not show it.

1. `⚡ Include (Regex):` (accent), a 180 px field with the hint `ERROR|CRITICAL|Exception...`, `✖`, `+N` (accent; the extra terms are listed in the tooltip) and `+` (adds a term row and opens the Filters window on this stream).
2. `🚫 Exclude (Regex):` (warn), with the same controls and the hint `healthcheck|ping|DEBUG...`.
3. `📊` timeline toggle; while the timeline is open, a `🔍` search-lane toggle (`render_timeline_toggles`). The time range itself is set from the stream bar's time span (§3.3, §4.15); the row holds no time fields.
4. `Aa` (case) and `.*` (regex) toggles: accent and strong when on.
5. **Level selector** (110 px combo): `All levels` or `≥ LEVEL`, each entry in its level colour. `?` shows lines without a level; it is visible only when the minimum is above TRACE.
6. **`Presets ▾`**: dim when nothing matches, `name ▾` in accent when the stream equals a preset, `name * ▾` in warn when modified.
7. **`🌐` badge** when the global filter applies to this stream. The tooltip lists `+term` / `-term`.

While a stream is in the *show in context* view, the filters are suspended and the whole row is drawn dimmed, at 45 % opacity (`multiply_opacity(0.45)`). The fields stay editable. The Include label's tooltip repeats the banner text. Editing any filter ends the context view and applies the new filter.

### 3.5 Rows area

Below the filter row, in this order:

1. **Quick labels strip** (only when labels exist): `🏷 Labels:` and one chip per label, ` n text `, in the label's preset colours, each with a small `✕`.
2. **Context banner**, only while a line is shown in context: `◆ Filters suspended: line N shown in the full log` in warn, and a `[Back to filtered view]` button (tooltip: *Back to the filtered view as it was (Esc or CTRL + K)*). `Esc` (on the focused stream, when no widget has focus) or `CTRL + K` leave the context view as well. The return restores the selection, the follow state and the scroll position of the moment the view was entered.
3. **Timeline strip** (§5.6) when `📊` is on.
4. **Search results pane** (§5.5) when `☰` is on and a query is active. It is a bottom `egui::Panel` inside the tab, laid out before the rows so the rows take the remaining space.
5. **Rows**, and on the right the 10 px **overview strip** (§5.4) when it is enabled and there is something to mark.

Empty states are centred and dim: `⏳ indexing...`, `⏳ filtering...`, *Log file is empty (waiting for log output...)*, *No lines match the active filters*.

**Row anatomy (text view):**

```
[marker] [line no.] [Δt cell] [×N] [[+] JSON] text…
   ▶       1234 │   +1.402   ×57  [+] JSON   {"level":"error", ...}
```

- **Marker column.** Shown when there is a search, a bookmark (manual or automatic) or a context line. It holds one of, by priority: `◆` (the context line, warn), `▶` (current hit, accent), `●` (other hit, warn), then the bookmark glyphs in `secondary_accent`: `★` (manual bookmark), `✏` (manual bookmark with a note), `☆` (automatic bookmark only, from a rule); otherwise a figure space so the columns never shift.
  - Hovering the marker of a row whose bookmark has a note shows the note as a tooltip (`note_tooltip`).
  - On the row of a closed collapsed group, the marks also come from the lines it hides (`row_marks`, `BookmarkMark::of_row`): the row is a hit, or the current hit, when a hidden line is, and without a bookmark of its own it takes the mark of a hidden bookmarked line, a manual one first.
- **Line number.** `{:>6} │`, dim at 60 % (accent on the current hit), strong.
- **Δt cell.** Right-aligned in 11 character widths. It shows `…` while pending, `⚓` on the anchor, `+m:ss.mmm`, and switches to the accent colour at or above `time_delta_gap_ms` (previous-row mode only).
- **`×N` badge** (collapse of repeated lines, text and wrapped views). The first row of each group of two or more equal consecutive entries carries it, strong, at the log font size: in warn while the group is collapsed, dim once it is expanded. N is grouped with `,` (`×1,234`), and above 999,999 it reads `×1.2M`.
  - An *entry* is a line plus the stack-trace continuation lines that follow it, so a repeated exception collapses as a whole. Entries over 256 lines or 64 KiB are never grouped.
  - **Tooltip:** `{n} repeated entries, lines {first}–{last}`, then `🕘 first → last` timestamps once the stream is timed, then *Click to show every line* or *Click to collapse again*.
  - **Click:** expands or collapses that group (`toggle_collapsed_row`). The badge's click target is registered after the row's, so it wins the click.
  - A click on the row of a closed group selects every line of the group. `CTRL + C`, the "Copy" menu entry and the exports keep every underlying line; "Copy as shown" writes one line per row with ` ×N` after a closed group's row.
  - Groups are detected over the visible (filtered) lines; appended lines resume the detection from the last entry, and a reload starts again. The context view is never collapsed.
- **JSON toggle.** `[+] JSON` / `[-] JSON` in `secondary_accent`, at the font size minus 2 (minimum 9). When expanded, pretty JSON appears in an 11 pt frame (`panel_bg` × 1.3, border at 40 %).
- **Text colouring,** by priority:
  1. Current hit: black text on `#00FFE6`, strong.
  2. Other hits: black on `#FFE600`.
  3. Span layout, when a captures-only rule, a quick label or ANSI colours apply.
  4. The first matching highlight rule (foreground, background, bold, italic).
  5. The level palette (§6.3), when `level_colors` is on.
  6. `text_primary`.
- **Row tint** behind the whole row width: current hit is accent at α70, other hits warn at α40, selected rows `secondary_accent` at α60, bookmarked rows (manual or automatic, or a closed group hiding a bookmark) `secondary_accent` at α28.
- **Row height:** `max(font row height × 1.25, 18)`, rounded up.

**Extend (no-wrap) mode** uses `ScrollArea::both().show_rows(...)`, sticks to the bottom while following, and has a virtual horizontal extent of `MAX_LINE_BYTES × 16` px.

**Wrap mode** uses `ScrollArea::vertical().show_viewport(...)`. The viewport is anchored to a row (`WrapAnchor`). Only the rows in view are laid out as galleys. The scroll bar works from an estimated height, so its thumb is approximate. The row gutter, text, tints, the `×N` badge and JSON are painted directly with the painter; the badge's width is taken from the wrap width.

**HEX view** (`render_hex_stream`):

- A header row, `OFFSET    00 01 …  |....|`, in strong accent. It scrolls horizontally in sync with the body, with its scroll bar hidden.
- Each row: `XXXXXXXX` offset (dim at 75 %), bytes in groups of 8, then `|ASCII|` with `·` for non-printable bytes (`secondary_accent`).
- Search hits use the same yellow and cyan backgrounds.

**Markdown view:** `egui_commonmark::CommonMarkViewer` inside a `ScrollArea::both`. Embedded HTML is converted to Markdown text.

---

## 4. Secondary windows, dialogs, popups and context menus

Sources: `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/zip_picker.rs`, `src/ui/find_results.rs`, `src/ui/hit_list.rs`, `src/compressed.rs`

The shared helpers in `app.rs` are:

- `restore_dialog_geometry`: without a saved position the dialog is centred with a `CENTER_CENTER` pivot; without a saved size the default below applies.
- `capture_dialog_geometry`: writes the rendered rectangle back into the config every frame.
- `apply_dialog_chrome_cursor`: a Move cursor over the title bar's drag strip, and a pointing hand over its collapse and close buttons.

The four "big" dialogs are plain `egui::Window`s. They are **non-modal**, resizable, collapsible (egui default), closable with their ✕ and toggled by their button. **`Esc` closes the topmost open one** of Help, Settings, Filters and About (`close_topmost_dialog`, by egui layer order), not all four at once, and ends text input.

### 4.1 Settings — `⚙ Settings`

- **Opened by** the toolbar `⚙ Settings` button. Closed by ✕, `Esc` or the same button.
- **Window:** id `fasttail_settings_popup`, frame `panel_bg` with a 1.5 px `border_color` stroke, default **460 × 400**, geometry persisted in `settings_pos` and `settings_size`.
- **Contents:** a vertical `ScrollArea` that does not auto-shrink:
  1. `⚙ SETTINGS` heading.
  2. **Theme:** selectable `Tron` · `Matrix` · `Blade` · `Light` (the last one localized).
  3. **Language:** a 220 px combo. The first entry is `System language (<detected>)`, then a separator and the 16 languages by native name.
  4. **Zoom:** `[-]  🔍 N%  [+]  [100%]`, in 10 % steps over 50–300 %.
  5. **Font Size:** `[-]  N pt  [+]  [100%]`, over 8–32 pt; the default is 13.
  6. **Matrix Screensaver** checkbox; when on, a timeout `DragValue` (0–120 min, 0 = never) and a `Test screensaver` button.
  7. **🔒 PIN lock:**
     - "Lock when the screensaver ends" checkbox.
     - A PIN password field (90 px, 4–12 digits), `Save PIN` and `Clear PIN`.
     - `Lock now`, disabled without a PIN.
     - A small note that the lock is a deterrent.
  8. Checkboxes: System Telemetry · Cyber Audio SFX · Borderless Window · Show Line Numbers · Colour rows by log level · Overview strip beside the scroll bar · Time delta (Δt) column. Then `Δt gap highlight (ms, 0 = off)` as a `DragValue` (0–86 400 000).
  9. **🛠 External tools:**
     - The list of placeholders and a link to the cookbook.
     - One grouped card per tool, with Name, Program, Arguments, Shortcut (`Ctrl+Shift+F9`, with an invalid-shortcut warning), Rule (a combo of the highlight-rule patterns, with a "missing rule" warning), Match regex, and a "Run via shell" checkbox with a ⚠ warning.
     - The count of dropped runs (bound tools) and `🗑 Remove`.
     - `➕ Add tool`, and the last run error.
  10. **Renderer:** a combo (Auto, OpenGL, wgpu, `Software (CPU, wgpu) — not recommended`). Below it, `Applies at the next start · <chip> <details>`, and a ⚠ warning when Software is selected.
  11. Checkboxes: **Always on top**, **Flash the window on background alerts**.
  12. **⚡ PERFORMANCE & REFRESH:**

      | Setting | Range |
      |---|---|
      | Poll interval | 50–5000 ms |
      | Size check interval | 50–10000 ms |
      | Max UI FPS (GPU) | 15–240 |
      | Max UI FPS (Software / VM) | 10–120 |
      | Mouse throttle | 0–1000 ms |
      | Max Markdown size | 1–100 MB |
      | Max auto-bookmarks per stream (`auto_bookmark_max`) | 100–100 000, default 10 000; the open streams recompute theirs when the drag ends |
      | Max decompressed size | GB |
      | Max standard input spool | MB |

      The decompression folder is shown as a label, with `📁` (folder picker) and `↺` (reset).

### 4.2 Filters and Color Filters — `⚡ Color Filters`

- **Opened by** the toolbar `⚡` button. It is also opened on a given stream by a stream bar `+` term button or by `Presets ▾ → ⚙ Manage presets…`. That stream's header opens and scrolls into view once (`filters_focus`).
- **Window:** id `fasttail_filters_popup`. The title is `⚡ Color Filters (N active)` in warn. Default **540 × 460**, geometry persisted.
- **Contents:**
  1. **Filters section** (`render_filters_content`):
     - A `🔍 Filters (n active)` heading and a description.
     - A `🌐 Global filter active:` note with its terms, when the global filter applies.
     - **⭐ Filter presets** manager: `⬆` `⬇` reorder, name (the tooltip is a summary), `✏` rename inline (`✔` / `✖`, with a "name already used" warning), `🗑` delete with an inline confirmation (`Delete preset "x"?`, `Delete` / `Cancel`).
     - One `CollapsingHeader` per stream, `📄 file`, open by default when it has more than one term. Inside:
       - `All of:` and `None of:` numbered term rows, 260 px, each with `✖` and a `⚠ invalid regex` flag.
       - `+ Add term`, up to 8 per side.
       - `Aa` and `.*` toggles.
  2. A separator.
  3. **Color Filters section** (`render_highlights_content`):
     - A `⚡ Color Filters (a/t active)` heading, a description and an order hint.
     - One group per rule: enabled checkbox, `⬆` `⬇`, `#n:`, pattern field, `Regex`, `Captures only` (regex rules only), `Aa`, `B`, `I`, sound combo (None, Beep, Chime, Warning, Critical) with `▶` test, a **`Bookmark matching lines`** checkbox, `FG:` and `BG:` sRGB colour buttons, a live ` Preview ` chip, and `🗑`.
     - An "Add rule" button, which creates a rule with white text on `#0064C8`.
     - **Automatic bookmarks.** Every line an enabled rule with "Bookmark matching lines" matches carries an automatic bookmark (`☆`), lines appended later included, whatever the rule's colours. The tooltip says so. On a large file the matching runs as a background scan (`⏳ finding auto-bookmarks N%`). At most `auto_bookmark_max` are kept per stream, the first in file order; past that the stream bar shows `ⓘ auto-bookmarks capped at N`. `F2` / `SHIFT + F2` visit them with the manual ones, and `CTRL + F2` or "Remove bookmark" dismisses one. A dismissal lasts until the set of bookmarking rules changes (a pattern, its regex or case flag, the rule's enabled box or the option itself): colours, styles and sounds leave the automatic bookmarks alone.

### 4.3 About — `ℹ About FastTail`

- **Opened by** the toolbar `ℹ About` button.
- **Window:** id `fasttail_about_popup`, resizable, default **440 × 420**, geometry persisted.
- **Contents:**
  - A centred `⚡ FASTTAIL` title, 18 pt, strong, accent.
  - A two-column grid: Version `vX`, Build date (`BUILD_TIMESTAMP`), Renderer (`chip · details`), Author, Website (link `www.baccan.it`), Repository (link `github.com/matteobaccan/FastTail`), License `MIT`.
  - The tagline in 10.5 pt dim.

### 4.4 Help — `❓ Guide & Keyboard Shortcuts`

- **Opened by** `F1` (toggle) or the toolbar `❓ Help` button.
- **Window:** id `fasttail_help_popup`, default **580 × 500**, geometry persisted.
- **Contents:** a `⚡ FASTTAIL` header, then three `ui.group`s with warn-coloured titles:
  1. **🔍 ZOOM & FONT SIZE**: `CTRL +  /  CTRL =`, `CTRL -`, `CTRL 0`, `CTRL + Wheel`.
  2. **🧭 NAVIGATION & LOG STREAMING**: Spacebar, `CTRL F`, `CTRL + SHIFT + F`, `CTRL + K`, `CTRL + SHIFT + H`, `CTRL + SHIFT + D`, `F3  /  SHIFT + F3`, `Click / SHIFT + Click / CTRL + Click`, `CTRL + A  /  CTRL + C`, `CTRL + G`, `ALT + W`, `ALT + 1..9`, `☰ ↑ ↓ PgUp PgDn Enter Esc`, `CTRL + SHIFT + 1..9`, Right click / tool shortcut, `CTRL + SHIFT + T`, `CTRL + L`, `CTRL + F2  /  F2  /  SHIFT + F2`, `F1`, `Esc`, Drag & Drop.
  3. **⚡ COLOR FILTERS & VISIBILITY**: six bullet paragraphs (evaluation order, reordering, bold and italic, visibility filters, log levels, recent files).
- Keys are written in capitals joined with ` + ` (house style). The exception is `CTRL F`, which lacks the `+`.

### 4.5 Open pattern prompt — `📂* Open pattern`

- **Opened by** the `📂*` toolbar button (prefilled with `<recent dir>\*.log`) or by **dropping a folder** on the window (prefilled with `<folder>\*.log`).
- **Window:** id `fasttail_pattern_prompt`, not resizable, not collapsible, anchored at the centre. The text field is 420 px, takes focus when opened, and has a hint.
- **Behaviour:** the text is validated live. The directory part must exist; otherwise `ⓘ invalid` shows in warn. `Open` is disabled while the text is invalid. `Enter` submits and `Esc` cancels.

### 4.6 Session dialogs

All three are anchored at the centre, not resizable and not collapsible.

- **`🗂 Unsaved session changes`** (`fasttail_session_confirm`): *The session "name" has unsaved changes…*, with `Load anyway` and `Cancel` (also `Esc`).
- **`🗂 Streams not opened`** (`fasttail_session_missing`): lists the missing paths as bullets, with `OK` (also `Esc`).
- **`💾 Session saved`** (`fasttail_save_notice`): explains that the stdin stream was not saved, with `OK`.

### 4.7 Compressed-open notice — `🗜 Cannot open the compressed file`

`fasttail_open_notice`, anchored at the centre. It shows the path and the reason (empty zip, no space, refused entry, *the archive no longer holds this entry*, I/O error) and an `OK` button.

### 4.8 Archive entry picker — `🗜 Choose entries — archive.zip`

Source: `src/ui/zip_picker.rs` (`ArchivePicker`), with the archive detection in `src/compressed.rs`

Files are recognised by their content, not their name: gzip, bzip2, xz and zstd files are single compressed logs, zip and tar are archives. A gzip, bzip2, xz or zstd file whose decompressed content starts with a tar header is a compressed tar (see §10.3 for how that is decided).

- **Opened**
  - for a zip that holds more than one file entry; a zip with a single openable entry opens directly, and an empty zip shows the compressed-open notice (*the zip archive holds no file*);
  - at once for **any tar**, plain or compressed (`ArchivePicker::scanning`). A tar has no central directory, so a background `TarScan` walks its headers and the picker pulls the rows it found every frame (`sync`). When the scan ends with exactly one openable entry, and the user neither opened nor checked anything, that entry opens and the picker closes by itself.
- **Window:** id `fasttail_zip_picker`, resizable, not collapsible, default width **520**. The list scroll area is at most 320 px high. The title is `🗜 Choose entries — <archive file name>`.
- **Contents:**
  - **Scan line** (tar only), above the filter:
    - while the scan runs, `🗜 scanning N%` in warn and a `✖` button (tooltip: *Stop scanning (the entries found so far stay listed)*); the window repaints every 100 ms;
    - when it ended early, a `⚠` line in warn: *The list is partial: the scan stopped at 100,000 entries or 1024 GB of data* (a `ScanLimits` bound), *Scan stopped: the list is partial* (the ✖), *Damaged archive, the list ends at the unreadable header: {error}*, or *decompression failed: {error}*;
    - once the scan is over and no entry can be opened (or there is none), *The archive holds no file that can be opened* in warn.
  - A `🔍` filter field (260 px), `Select all` (openable visible rows only) and `Select none`.
  - Sortable headers `Name ▲/▼` and `Size ▲/▼`.
  - A checkbox per entry with its human-readable size (dim). Refused entries are shown with a disabled checkbox, `size · reason` in warn and the reason as tooltip. The reasons are: *encrypted entry, not supported*, *compression method {method} not supported*, *unsafe entry name*, *another entry has the same name* (zip), and *link or special file, not supported*, *sparse file, not supported* (tar).
  - `📂 Open (n)`, disabled when n = 0 (with a tooltip saying why), and `Cancel`. Closing the window with ✕ cancels.
  - Each chosen entry opens as its own stream. While a tar scan still runs, `Open` keeps the picker open and clears the selection, so more entries can be picked as they are found; otherwise it closes the picker.
- An entry that is itself compressed (for example a `.gz` inside a tar) is decoded once more when it is extracted.

### 4.9 BareTail import — `⚡ BARETAIL CONFIGURATION DETECTED`

- **Shown** once at startup when `baretail_import` is true, the prompt was not shown before and a BareTail configuration is found in the registry (Windows).
- **Window:** not collapsible, not resizable, centred. Fill `bg_color` with a 2 px `border_color` stroke.
- **Contents:** a description and `• Discovered recent files: n` and `• Highlight rules: n` in `secondary_accent`. These two labels are hard-coded English.
- **Buttons:** `IMPORT FROM BARETAIL` (merges rules not already present and opens the files) and `SKIP`. Either one sets `baretail_prompt_shown`.

### 4.10 Save filter preset — `💾 Save filter preset`

Source: `src/ui/dock.rs`, `render_preset_save_dialog`

- **Opened by** `Presets ▾ → 💾 Save current as preset…`.
- **Window:** a true **`egui::Modal`** (it dims and blocks the UI), 320 px wide. Its draft state is kept in egui temp memory per stream.
- **Contents:**
  - A `Name:` field (200 px), which keeps focus.
  - An `Include the time range` checkbox.
  - `⚠ A preset with this name exists: saving replaces it`.
  - `Save` or `Overwrite`, and `Cancel`.
- **Keys:** `Enter` saves unless the name is taken. `Esc` cancels.

### 4.11 PIN lock overlay

Source: `src/ui/app.rs`, `render_lock_overlay`

- **Triggered by:**
  - `CTRL + L` or `Lock now` in Settings, when a PIN is set;
  - the end of the screensaver when "lock" is enabled.
- **Backdrop:** painted on the `Middle` layer and fully opaque in `bg_color`, so the logs are hidden. It animates:
  - a 34 px accent grid at 7 % alpha that drifts one cell every 4 s;
  - a soft 14-band glow sweep every 6 s;
  - repainted every 33 ms.
- **Prompt:** an `egui::Modal` with a transparent backdrop and a `panel_bg` frame with a 2 px `border_color` stroke. Its width is **measured from the translated strings** (monospace 16 and 14 pt), clamped between 300 px and 90 % of the window. It contains:
  - `🔒 FASTTAIL LOCKED` (16 pt, accent);
  - *Enter the PIN to continue* (dim);
  - a centred 180 px password field that grabs focus;
  - an `Unlock` button (accent);
  - `⚠ Wrong PIN` in warn after a failure. An error beep plays if sound is on.
- **Cooldown:** after 3 wrong PINs the field is replaced for 60 s by `⏳ Too many attempts: try again in N s`.
- **Input filtering:** while locked, only text, paste, pointer events and Enter, Backspace, Delete, the arrow keys, Home, End and Tab without modifiers reach egui. Escape, F-keys, Space, copy and cut, and every shortcut with CTRL or ALT are dropped. The borderless resize zones are disabled.
- **Backdoor:** the passphrase `joshua` always unlocks (`LOCK_BACKDOOR`). It is documented in code as a deterrent, not a security boundary.

### 4.12 Menus and drop-downs

| Menu | Opened from | Entries |
|---|---|---|
| Recent files | toolbar `🕒` | `📄 name (path)` × up to 15, separator, `🗑 Clear recent files history` |
| Sessions | toolbar `🗂` | `Save session as…` (native save dialog, filter `FastTail session` *.ini), `Save session` (disabled without a current session), `Load session…`, `Recent sessions ▸` submenu (`🗂 name`, path as tooltip; `Clear recent sessions`), separator, `Save current as default workspace` |
| Search history | stream bar `🕒` (max width 280) | the last queries; picking one runs it and centres the first hit |
| Export / stream actions | stream bar `💾` (max width 260) | `Export visible lines...`, `Export search matches...` (with a query), separator + `Clear bookmarks` (with manual or automatic bookmarks; it removes the manual ones with their notes and dismisses the current automatic ones), separator + *Run tool* + `▶ tool` × n (tools run on the current row) |
| Presets | filter row `Presets ▾` (min width 240) | each preset (a selectable label, summary as tooltip) + a small `all` button (apply to all streams), separator, `💾 Save current as preset…`, `⟳ Update "name" from this stream` (when modified), `⚙ Manage presets…` |
| Combos | — | encoding, ANSI mode, collapse mode, minimum level, language, renderer, sound alert, tool rule binding |

### 4.13 Context menus (right click)

- **Row context menu** (`row_context_menu`, text and wrapped views). It is **always available** on a row. Minimum width 160. The picks are applied after the rows are drawn (`RowMenuPicks`). Groups are separated by separators:
  1. `Copy  (CTRL + C)`: the selection when the clicked row is part of it, else the clicked row alone (it becomes the selection); every underlying line, those a collapsed group hides included.
  2. `Copy as shown`: the same, one line per row as the view shows it, with ` ×N` after a closed group's row.
  3. `✏ Bookmark note…`: opens the note editor in the stream bar (§3.3) on that line.
  4. `☆ Remove bookmark  (CTRL + F2)`, only when the row's own line is bookmarked: what `CTRL + F2` does there (removes a manual bookmark and its note, or dismisses an automatic one).
  5. `◆ Show in context  (CTRL + K)`, on a filtered stream.
  6. `⚓ Set time anchor here` (not on the current anchor) and `Clear time anchor` (when one is set), while Δt is shown.
  7. `▶ tool name` for each external tool.
- **Find results hit context menu:** `◆ Show in context  (CTRL + K)`. It jumps to the hit with the stream's filters suspended.
- **Dock tabs:** `egui_dock` defaults only. FastTail does not customize them.

### 4.14 Native dialogs (rfd)

- "Open Log Files": multi-select, filter `*.log, *.txt, *.*`.
- Session save and load: `FastTail session` (*.ini). The default name is `session.fasttail-session.ini`.
- Export save: `Text (*.txt, *.log)`, named `<stem>-export.txt` or `<stem>-matches.txt`.
- The spool folder picker.

An export failure is shown as an `ⓘ` notice in the stream bar (`view_notice`).

### 4.15 Time range popup

Opened by a click on the stream bar's time span (§3.3); a second click on the span closes it. An `egui::Popup` anchored under the span (`PopupCloseBehavior::CloseOnClickOutside`, 4 px gap), per stream (`time_range::popup_id`). Opening it asks for the stream to be timed (`request_timeline`), in the background above 16 MB.

- **Two sides**, "from" and "to", side by side with an 18 px gap. Each has:
  - its label (`from` / `to`) in 11 pt accent;
  - a 180 px single-line field (hint `2026-09-18 14:02:05` / `2026-09-18 16:30`) that takes every format the time range accepts; editing it moves the calendar to the month typed;
  - `⚠ invalid time` in warn under it, when it holds text that cannot be read and does not have the focus;
  - the **calendar** (`calendar::show`): `«` `‹` *Month YYYY* `›` `»` (small buttons with *Previous / Next month / year* tooltips, the title in 11.5 pt accent, 104 px), a row of weekday abbreviations (Monday first, dim), and a 7×6 grid of painted 26×18 px day cells. Days of other months are dim; the days between the log's first and last timestamp have an accent fill at 18 %; the side's day an accent fill at 55 %; today an accent outline; the hovered day a dim outline. A click picks the day (a day of another month moves the calendar there);
  - `🕘 HH : MM : SS` spinners (`DragValue`, two digits, 0–23 / 0–59, tooltip *Hour, minute and second: drag or type*).
- **Shortcuts:** `Whole log` (tooltip *Clear the time range*), `First day`, `Last day`, `Last hour`. The last three are disabled until the stream is timed and when it has no usable timestamps.
- **Hints:** `⏳ applies when timing finishes` (warn) while a window is held, `⏳ timing lines N%` (dim) while the stream is being timed, `ⓘ no timestamps` (dim, with the explanation in the tooltip).
- **`OK`** (disabled while a side cannot be read, tooltip *invalid time*) and **`Cancel`**, reusing the session dialogs' texts.

Everything edits a **draft** (`TimeRangeDraft`: the two texts and the month of each calendar) kept in egui temp memory while the popup is open, and prefilled with the stream's `time_from_text` / `time_to_text` when it opens. The calendars open on the month of their side, else of the log's first timestamp, else the current month. Picking a day keeps the side's time (`2026-09-19 14:02:00`), otherwise writes the bare date (from midnight, or through 23:59:59.999 on the "to" side); a spinner writes `YYYY-MM-DD HH:MM:SS` on the side's day, or on the log's day (or today) when it lies in the month shown, else on the 1st of that month. The spinners of an empty or bare-date "to" side show `23:59:59`. `First day` / `Last day` put the date of the log's first / last timestamp on both sides; `Last hour` writes the hour ending at the last timestamp; `Whole log` empties both sides.

`OK`, or `Enter` in a field while both sides can be read, writes the draft into the stream through `apply_time_range_text` (the window is applied, or held while the stream is timed, exactly as typed text) and closes the popup. `Cancel`, `Esc` (consumed by the popup) or a click outside drop the draft and leave the window as it was. Nothing of the popup is saved.

### 4.16 Things that do not exist

There is no separate goto-line dialog (it is inline in the stream bar), no bookmarks list window and no note dialog (the note editor is inline in the stream bar). Bookmarks are the `★` / `✏` / `☆` markers, the note tooltips, the overview strip marks, `F2` navigation and "Clear bookmarks". There are no toast notifications. Transient information is shown as `ⓘ` labels in the stream bar or in the small centred notice windows above.

---

## 5. Graphic elements inventory

Sources: `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/hit_list.rs`, `src/ui/overview_strip.rs`, `src/ui/timeline_strip.rs`, `src/theme.rs`, `src/screensaver.rs`

### 5.1 Widgets used

- **Standard egui widgets:** `Button`, `small_button`, `selectable_label` and `selectable_value`, `checkbox`, `TextEdit::singleline` (some with `password(true)`), `ComboBox`, `DragValue`, `color_edit_button_srgb`, `CollapsingHeader`, `Grid`, `group`, `hyperlink_to`, `menu_button` and `MenuButton::from_button`, `context_menu`, `ScrollArea` (`show_rows`, `show_viewport`), `Panel` (top, bottom, and a resizable bottom panel inside a tab), `Window`, `Modal`.
- **Custom painting** (`Painter`): the title-bar badge, the telemetry meters, the status-bar indicator, the wrapped rows, the hit lists, the overview strip, the timeline, the lock backdrop, the borderless frame and grip, and the screensaver.
- **Custom composite:** `toggle_button` (`src/ui/dock.rs`). When active, the label is strong in the accent colour on a fill of accent at 20 % with an accent stroke at 85 %; when inactive, the label is dim. It exists because label colour alone was unreadable on Light.

### 5.2 Emoji and glyphs used as icons

| Glyph | Meaning / where |
|---|---|
| `F` (painted badge) | app logo, title bar |
| `✕` `🗖` `🗗` `—` | borderless window buttons |
| `🌐` | global filter (title bar button, stream badge, bar title, Filters window) |
| `📌` | always on top |
| `🔍` | zoom level; search field label; timeline search-lane toggle; Filters heading; archive picker filter |
| `🖥` | CPU meter |
| `📁` | Open File; spool folder picker |
| `🕒` | recent files; search history |
| `📂*` / `📂` | open pattern / pattern stream label; empty-state icon; archive picker "Open" |
| `🗂` | sessions |
| `⚡` | Color Filters; Include field; FASTTAIL headers; Performance section |
| `▶` / `■` | watching / paused (tab title, Follow and Monitor toggles); `▶` also marks the current hit and runs tools |
| `⏸` | Pause |
| `⚙` / `⚙️` | Settings (the `⚙️` variant is used only by the legacy Settings tab); Manage presets |
| `❓` `ℹ` | Help, About |
| `⚠` | warnings (software banner, invalid values, wrong PIN) |
| `●` / `○` | new data / none (tab title); `●` is also the "other hit" marker |
| `★` | manual bookmark marker |
| `✏` | marker of a manual bookmark with a note; "Bookmark note…" menu entry and note editor label; `✏ line: note` in the overview strip tooltip; also rename (presets) |
| `☆` | marker of an automatic bookmark (a rule's "Bookmark matching lines"); "Remove bookmark" menu entry |
| `◆` | line shown in context (marker, banner, "Show in context" menu entries) |
| `×`, `×N` | collapse selector (`× Collapse: …`); badge of a collapsed group (`×57`, `×1.2M`) |
| `🔤 TXT` `🔢 HEX` `📝 MD` | view modes |
| `# 123` / `# ---` | line numbers on / off |
| `Δt`, `⚓`, `…` | time-delta column toggle, anchor, pending |
| `↩` | Wrap |
| `🕘` | the visible time span, which opens the time range popup; the popup's spinners; first → last time in a collapse badge tooltip |
| `«` `‹` `›` `»` | calendar of the time range popup: previous / next year and month |
| `📊` | timeline toggle |
| `⏳` | background work / progress / cooldown |
| `ⓘ` | informational notice |
| `🗜` | compressed stream status, archive picker (title and `scanning N%`), compressed notice |
| `⏹` | stdin ended |
| `📦` | file size |
| `🔎` | search all streams / Find results tab |
| `☰` | search results pane |
| `▲` `▼` | previous / next hit; sort direction; group collapsed / expanded (`▶` / `▼`) in Find results |
| `✖` / `✕` | clear / remove / close / cancel (also stops decompression and the tar scan) |
| `+`, `+N` | add a filter term / extra terms |
| `🚫` | Exclude field |
| `Aa`, `.*`, `?`, `≥` | case sensitive, regex, show unknown levels, minimum level |
| `🏷` | quick labels |
| `💾` | export menu; save preset; session saved |
| `⟳` | re-extract archive; refresh Find results; update preset |
| `↺` | reset spool folder |
| `⬆` `⬇` `🗑` `✔` `➕` `⭐` `🛠` `🔒` `⇢` `📄` | reorder, delete, confirm, add tool, presets heading, tools heading, lock, go-to, file entry |
| `[+] JSON` / `[-] JSON` | JSON expander |
| `␛` | ESC byte in ANSI raw mode |
| `·` | non-printable byte in the HEX ASCII column |

On Windows the CJK fallback fonts are appended (§6.6). Emoji rendering relies on egui's default fonts.

### 5.3 Markers, highlights and colour-coded levels

- **Marker column** (`◆` `▶` `●` `★` `✏` `☆`), **`×N` badge**, **row tints**, **search colours** (`#00FFE6` current, `#FFE600` other, black text): see §3.5. The results lists use the same `#FFE600` for the query tint (`QUERY_BG`).
- **Level colouring:** see the palette in §6.3. It applies only when no user highlight rule matches the row and `level_colors` is on. INFO and unknown lines keep the primary text colour. FATAL rows get a filled background and bold text.
- **Highlight spans** (`span_layout_job`), non-overlapping and sorted:
  - `Rule` (the rule's foreground and background, italic);
  - `Label(n)` (the theme's preset colour, §6.5);
  - `Ansi` (the theme's ANSI palette with bold→bright, dim, inverse and underline).
  - Search hits skip spans and keep the search colours.
- **Level counters and the level combo:** `level_color` per level.

### 5.4 Overview strip (minimap)

Source: `src/ui/overview_strip.rs`

- 10 px wide, on the right edge of the rows area, when `overview_strip` is on and there is at least one mark or a current hit.
- **Background:** `panel_bg` at 85 %, with a 1 px left line in `border_color` at 40 %.
- **Marks per pixel row,** painted in this order:

  | Mark | x-extent | Colour |
  |---|---|---|
  | ERROR / FATAL | 1 px to 45 % | `level_color(Error)` |
  | search hits | 40 % to 100 % | warn |
  | automatic bookmarks (not dismissed, not also manual) | 1 px to 35 % | `secondary_accent` at half alpha (`gamma_multiply(0.5)`) |
  | manual bookmarks | 1 px to 35 % | `secondary_accent` |

  The automatic marks are drawn at half strength and under the manual ones, so the manual bookmarks stay visible among a rule's many marks.
- The current hit is a 2 px accent line across the strip. The viewport is a box outline in `text_primary` at 70 %.
- **Collapsed rows.** While repeated entries are collapsed, the strip maps rows, not lines: a line a closed group hides is marked at the group's row (hit, bookmark, automatic bookmark or error). Error marks are exact up to 4 000 000 visible lines and sampled above (at most 256 sampled rows per pixel).
- **Tooltip:** `Line N`, then the notes of the bookmarks drawn within 2 px of the pointer, as `✏ line: note` (at most five, visible lines only), plus "Error marks are sampled…" when sampled. A click or drag centres the main view on that row.
- **Cache:** kept in egui memory per stream. It is rebuilt at once on a height or bookmark change, and otherwise at most every 250 ms. Without collapse, error marks are sampled only on filtered views larger than 4 000 000 rows.

### 5.5 Search results pane and Find results list

Source: `src/ui/hit_list.rs`

- **Search results pane.**
  - A bottom panel, default height 180 px, minimum 60, and it always leaves at least 80 px to the rows. The user can resize it; the height is shared by all streams and persisted.
  - The header is `☰ Matches for "q": N` (accent) with the capped note and `⏳ searching N%`, and a `✖` on the right that closes the pane.
  - Rows show an optional `▶`, `{:>7}│` and the text coloured by level, with the query on yellow.
  - The current hit gets an accent fill at α60. The keyboard selection gets a 1 px `secondary_accent` outline. When the list has focus, a 1 px accent outline goes around it.
  - In HEX view it only shows a notice.
- **Find results tab.**
  - A query box, `Find`, `■ Stop` and `⟳ Refresh`.
  - A summary: `N matches in s of t streams`, `⏳ r searching, q queued`, `snapshot 42s ago`.
  - One virtualized list grouped by stream. Header rows have a fill of accent at α28 and show `▶/▼ name — N matches` with coloured notes: queued, `⏳ %`, stopped, cannot read, HEX skipped, stale (drawn dimmed), capped.

### 5.6 Timeline histogram

Source: `src/ui/timeline_strip.rs`

- The full width of the tab, **56 px** high. Background `panel_bg` at 85 % with a border.
- A 5 px **search lane** at the top: warn marks in the columns that hold hits.
- **Stacked bars**, bottom-up:

  | Level group | Colour |
  |---|---|
  | ERROR + FATAL | `level_color(Error)` |
  | WARN | warn |
  | INFO | accent |
  | DEBUG + TRACE | dim |
  | unknown | dim at 45 % |

  The scale is linear up to the peak. The label `peak N` sits top-left in 10 pt dim.
- The **current time window** is shaded with accent at 18 %, outlined with accent at 70 %.
- **While dragging,** the prospective span is outlined in 1.5 px `text_primary`.
- **Status text** at the top right: `⏳ timing lines N%` or *No timed lines yet*.
- **Tooltip:** the span (`2024-03-05 14:02:00 – 14:02:59`), the per-group counts, the untimed count, and a lane note.
- A click selects one column and a drag selects a range. Either writes the from/to fields.

### 5.7 Tooltips

Nearly every control has a localized tooltip through `on_hover_text`. Disabled controls use `on_disabled_hover_text`. The strips use `on_hover_text_at_pointer`.

### 5.8 Notices, banners and "toasts"

There is no toast system. The feedback channels are:

- `ⓘ` / `⚠` / `⏳` labels in the stream bar (`view_notice`, auto-bookmarks capped, ANSI switched, pattern switched, scans, compressed stop reasons);
- the goto notice;
- the software banner;
- the context banner;
- the `⚠` scan notices and "no openable file" line of the archive picker (§4.8);
- the small centred notice windows (§4.6, §4.7);
- sound effects when `sound_enabled` is on (attach blip, error beep, rule alerts);
- `RequestUserAttention(Informational)`, a taskbar flash for a background error when `flash_on_alert` is on and the window is unfocused.

### 5.9 Status and telemetry fields

- **Title bar:** CPU % (bar and text), RAM used / total GB (bar and text), zoom %, pin state, global filter state.
- **Status bar:** the stream path and the renderer chip.
- **Stream bar:** follow and monitor state, mode, collapse mode, lines (visible / total, and rows shown when collapsed), visible time span, selection elapsed time, level counts, scan progress, size, throughput, match counter.

### 5.10 Matrix screensaver

Source: `src/screensaver.rs`

- Starts after `screensaver_timeout_mins` without input, only while the window is **focused**. It is drawn on the `Foreground` layer.
- A black backdrop at α245, with falling glyph columns 18 px apart in 14 pt monospace. The column head is `#DCFFE6` and the trail `#00FF41` fading out.
- Animates at about 30 fps (33 ms). Any input dismisses it.

---

## 6. Themes

Source: `src/theme.rs`, with the dock style and hard-coded colours in `src/ui/app.rs`

### 6.1 The four themes

`CyberTheme` (serialized in `[general] theme`):

| Enum | Settings label | `name()` |
|---|---|---|
| `Tron` (default) | Tron | Tron (Neon Cyan / Electric Blue) |
| `Matrix` | Matrix | Matrix (Phosphor Green / Black) |
| `Blade` | Blade | Blade (Amber Noir / Neon Magenta) |
| `Light` | localized "Light" | Light (Clean Solar / Crisp Slate) |

### 6.2 Base palette

| Token (method) | Tron | Matrix | Blade | Light |
|---|---|---|---|---|
| `bg_color` (canvas, title bar, tab bar) | `#0A0F18` | `#030603` | `#121014` | `#F3F5F9` |
| `panel_bg` (panels, tab body, dialogs) | `#0D1420` | `#060C06` | `#1A161C` | `#FFFFFF` |
| `tab_active_bg` | `#122032` | `#0A1A0C` | `#261C2A` | `#FFFFFF` |
| `tab_inactive_bg` | `#080C12` | `#040804` | `#0E0C10` | `#EAEEF4` |
| `button_bg` | `#0C131E` | `#08120A` | `#1C141A` | `#F0F4FA` |
| `code_block_bg` | `#121C2C` | `#081008` | `#221C24` | `#EEF2F8` |
| `border_color` | `#00E5FF` | `#00FF41` | `#FF8C00` | `#0078D7` |
| `accent_color` (= `info_color`) | `#00E5FF` | `#00FF41` | `#FF8C00` | `#0072CE` |
| `secondary_accent` | `#00B4D8` | `#32CD32` | `#FF0055` | `#009688` |
| `text_primary` | `#E0F0FF` | `#DCFFDC` | `#FFF3E0` | `#181C24` |
| `text_dim` | `#648CAA` | `#82C382` | `#AF9187` | `#64748B` |
| `error_color` | `#FF3344` | `#FF3344` | `#FF3344` | `#FF3344` |
| `warn_color` | `#FFBB00` | `#FFBB00` | `#FFBB00` | `#C36900` |
| selection fill | accent × 0.35 | accent × 0.35 | accent × 0.35 | accent × 0.35 |

`×` means egui `gamma_multiply`: every channel, alpha included, is scaled, so the colour becomes translucent.

### 6.3 Log-level palette

`level_color` is used for tags, counters and the level combo. `level_style` is used to colour rows when no rule matches.

| Level | `level_color` dark themes | `level_color` Light | Row style (`level_style`) |
|---|---|---|---|
| FATAL | `#FF3344` | `#C81E2D` | **bold**. Dark: text `#FFEBEE` on `#78101C`. Light: white on `#C81E2D`. |
| ERROR | `#FF3344` | `#C81E2D` | foreground = `level_color`, no background |
| WARN | `#FFBB00` | `#C36900` | foreground = warn |
| INFO | accent | accent | *none* (primary text) |
| DEBUG | `text_dim` | `text_dim` | foreground = `text_dim` |
| TRACE | `text_dim` × 0.7 | `#94A0B0` | foreground = `level_color(Trace)` |
| unknown | `text_dim` | `text_dim` | *none* |

### 6.4 Colours outside `theme.rs` (dock and chrome)

These are set in `src/ui/app.rs`.

| Element | Colour |
|---|---|
| Tab bar | fill `bg_color`, h-line accent × 0.35, height 26 |
| Active / focused tab | fill `tab_active_bg`, outline accent, text accent, top corners radius 6 |
| Inactive tab | fill `tab_inactive_bg`, outline `border_color` × 0.2, text `text_dim` |
| Tab body | fill `panel_bg`, 1.5 px stroke accent × 0.7, radius 6 |
| Dock separator | 3 px; idle accent × 0.25; hovered and dragged accent |
| Tab close button | `text_dim`; active warn |
| Current search hit bg | `#00FFE6` (constant) |
| Other search hit bg / query tint | `#FFE600` (constant) |
| Play button | `#00C864`; fill `#EBFCF2` (Light) / `#0A1812` |
| Pause button | `#EB2D4B`; fill `#FEF0F2` (Light) / `#1A0C10` |
| Color Filters button fill | `#FEF9EB` (Light) / `#14120C`, text and stroke warn |
| Title badge fill | `#DCEBFC` (Light) / `#0A1A28` |
| Telemetry meter track | `#DCE4EE` (Light) / `#081623`; RAM fill `secondary_accent`, CPU fill accent |
| Screensaver | black α245, `#00FF41` trail, `#DCFFE6` head |

### 6.5 Quick-label presets (`label_style(1..=9)`)

The nine presets run red, orange, yellow, green, cyan, blue, violet, magenta, grey.

| n | Dark themes background | Dark text | Light background | Light text |
|---|---|---|---|---|
| 1 | `#DC3232` | `#0A0A0A` | `#FFCDCD` | `#181C24` |
| 2 | `#E6781E` | `#0A0A0A` | `#FFDEB4` | `#181C24` |
| 3 | `#E6C81E` | `#0A0A0A` | `#FFF5A0` | `#181C24` |
| 4 | `#28BE50` | `#0A0A0A` | `#C8F0C8` | `#181C24` |
| 5 | `#00C8DC` | `#0A0A0A` | `#BEF0F5` | `#181C24` |
| 6 | `#3C78F0` | white | `#C8D7FF` | `#181C24` |
| 7 | `#965AF0` | white | `#E1CDFF` | `#181C24` |
| 8 | `#E63CB4` | `#0A0A0A` | `#FFCDEB` | `#181C24` |
| 9 | `#96A0AA` | `#0A0A0A` | `#DCE1E6` | `#181C24` |

**ANSI 16-colour palette** (`ansi_palette`). Indices 16–255 use the xterm table, and 24-bit colours are used as given.

- **Dark themes:** `#606470 #F05252 #50C869 #E6C850 #5C8EFA #D270E6 #3CCDDC #CDD2DA` / bright `#878C98 #FF7373 #78F091 #FFEB7D #87AFFF #F096FF #78F0FA #FFFFFF`.
- **Light:** `#181C24 #B91C2A #167830 #875A00 #1950C8 #91289B #007382 #585E69` / bright `#555E6E #CD2837 #1A7C36 #7D5F00 #2D5FD7 #A537AF #007886 #3C414C`.

A unit test checks contrast against `bg_color`: WCAG AA (4.5) everywhere, except 3.0 for the two dark greys on the dark themes. A background with no foreground gets black or white text, whichever has the higher contrast.

### 6.6 Applying the theme to egui

`CyberTheme::apply(ctx)` starts from `Visuals::light()` or `Visuals::dark()` and sets:

- `panel_fill` = `panel_bg`. `window_fill` = `panel_bg` on Light, `bg_color` on the dark themes. `extreme_bg_color` = `bg_color`. `faint_bg_color` = gray 240 on Light, black α180 on the dark themes.
- `noninteractive`: bg `panel_bg`, fg `text_primary`, stroke `border` × 0.4, radius 6.
- `inactive`: bg `button_bg`, fg `text_primary`, stroke `border` × 0.35, radius 6.
- `hovered`: bg `#E4ECF8` on Light, `panel_bg` × 1.25 (linear) on the dark themes; fg accent; 1.5 px accent stroke; radius 6; expansion 0.
- `active`: bg accent × 0.25, fg accent, 2 px accent stroke, radius 6, expansion 0. `open.expansion` 0.
- `selection`: bg accent × 0.35, stroke accent.
- `window_stroke`: 1.5 px `border_color`. `window_corner_radius` 8, `menu_corner_radius` 6.
- `ctx.set_theme(Light or Dark)`, and the **same** `Style` for both egui themes. The OS dark or light preference therefore never overrides the chosen FastTail theme.

On a software renderer, `apply_renderer_visuals` then removes shadows and rounding and disables feathering.

**Dark and light handling.** Light is a first-class theme with its own warn colour, level colours, label pastels and ANSI table. Several toolbar fills and the logo and meter colours switch on `theme == Light`. The theme is saved at once when it changes.

**Switching the theme.** Only through the Settings theme row. There is no shortcut.

### 6.7 Fonts and sizes

- **Default fonts.** egui's default fonts (eframe `default_fonts` feature).
- **CJK fallback on Windows.** Each of these fonts found in `C:\Windows\Fonts` is appended as a fallback to **both** the Proportional and the Monospace family: msyh, simsun, msjh, mingliu, YuGothR, meiryo, msgothic, malgun, gulim. There is one face per script, because no single CJK face covers the others.
- **Monospace everywhere.** Almost every label uses `RichText::monospace()`.
- **Log font.** `font_size`: default **13 pt**, range 8–32. It applies to rows, the hex view and the hit lists, and is independent of the zoom.
- **Zoom.** egui `zoom_factor` 0.5–3.0 in 0.1 steps. It scales the whole UI. `CTRL +`, `CTRL -` and `CTRL 0` are handled by egui itself; `CTRL + wheel` and the Settings row are handled by FastTail.
- **Fixed sizes in use:**

  | Size | Where |
  |---|---|
  | 10 pt | renderer chip, timeline labels |
  | 10.5 pt | telemetry, zoom label, hints |
  | 11 pt | stream bar and filter row labels, pretty JSON, status path |
  | 11.5 pt | notices |
  | 12 pt | badge "F", Help subtitle |
  | 13 pt | title |
  | 13.5 pt | empty-state text |
  | 14 pt | lock body, screensaver |
  | 16 pt | lock title, Help header |
  | 18 pt | About header |
  | 48 pt | empty-state 📂 |

---

## 7. Keyboard and mouse interaction model

Sources: `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/find_results.rs`, `src/ui/global_filter_bar.rs`, `src/ui/hit_list.rs`

### 7.1 Focus rules

- **Focused stream.**
  - It is the `LogStream` of the dock's focused leaf (`find_active_focused`), or else the active tab of the main surface.
  - **Only** it handles `F3`, `CTRL + F`, `CTRL + G`, `CTRL + K`, `CTRL + SHIFT + D`, `ALT + W`, `CTRL + A`, `CTRL + C`, the `F2` family, the navigation keys, external-tool shortcuts and quick labels. It also provides the prefill for `CTRL + SHIFT + F`.
  - When Find results is the focused tab, no stream is focused.
- **Keyboard ownership inside a stream.**
  - The navigation keys (arrows, Page Up / Page Down, Home / End, `CTRL + HOME` / `CTRL + END`) act only when **no widget wants keyboard input** (`egui_wants_keyboard_input()`).
  - `CTRL + A`, `CTRL + C`, `CTRL + SHIFT + D` and `F2` also work while the stream's results pane has focus.
  - Inside the search box, `↑`, `↓`, Page Up, Page Down, `Enter` and `SHIFT + ENTER` are consumed by the box for hit navigation. `Esc` in the box gives up focus.
- **Hit lists.** When one has focus it locks the arrows and Escape (`set_focus_lock_filter`) so egui's focus navigation does not steal them. `Esc` gives the keyboard back to the stream.
- **Order of consumption.** App-level shortcuts that could collide with stream shortcuts are **consumed before the dock is drawn**: tool shortcuts, `CTRL + SHIFT + 1..9`, `CTRL + SHIFT + F`, `CTRL + SHIFT + H` and `CTRL + L`. egui matches shortcuts logically, so otherwise `CTRL + F` would also fire on `CTRL + SHIFT + F`.
- **Lock.** While locked, events are filtered before any handler runs (§4.11).
- **Auto-focus.** The pattern prompt, the lock PIN field, the preset name field, the Find results query (after `CTRL + SHIFT + F`, with its text selected) and the go-to box take focus when they open.

### 7.2 Keyboard shortcuts

| Shortcut | Scope | Action |
|---|---|---|
| `F1` | global | Toggle the Help window |
| `Esc` | global | Close the topmost of Help, Settings, Filters and About, and end text input. Also closes the pattern prompt, session dialogs, notices and the preset modal; leaves the search box, the go-to box and hit lists; cancels the bookmark note editor; leaves the context view when no widget has the focus |
| `Space` | focused stream | Toggle Follow (not on a compressed stream, not while a text field has the keyboard) |
| `CTRL + SHIFT + T` | global | Toggle always-on-top |
| `CTRL + L` | global | Lock behind the PIN (needs a PIN) |
| `CTRL + SHIFT + H` | global | Show or hide the global filter bar |
| `CTRL + SHIFT + F` | global (prefill from the focused stream) | Open or focus Find results with the stream's query; the box takes focus, and `Enter` runs it |
| `ALT + 1..9` | global | Activate the tab of engine #1..9 (the `[#N]` in the tab title; engine order, not dock order) |
| `CTRL +` / `CTRL =` / `CTRL -` / `CTRL 0` | global (egui) | Zoom in, out or reset (10 % steps, 50–300 %) |
| `CTRL + wheel` | global | Smooth zoom |
| `CTRL + SHIFT + 1..9` | focused stream | Create, recolour or remove quick label n for the current search text. Without a current hit it shows the notice "Search for the text first" |
| *Tool shortcut* (e.g. `CTRL + SHIFT + F9`) | focused stream | Run that external tool on the current row. A modifier is required |
| `CTRL + F` | focused stream | Focus the search box with its text selected |
| `F3` / `SHIFT + F3` | focused stream | Next / previous hit (centred; wrap-around beep when sound is on) |
| `Enter` / `SHIFT + ENTER` | search box | Next / previous hit, using the text as typed (bypasses the debounce) |
| `↑` / `↓` | search box | Next / previous hit; scrolls one line when there are no hits |
| `PgUp` / `PgDn` | search box | Scroll one page |
| `CTRL + G` | focused stream | Open the inline go-to box. It accepts `N`, `+N`, `-N` or a time such as `14:02`. `Enter` jumps and selects; `Esc` closes |
| `CTRL + K` | focused stream | Show the selection anchor (or the first selected line) in context with the filters suspended; pressed again, go back |
| `CTRL + K` | Find results list | Show the selected hit in context |
| `CTRL + SHIFT + D` | focused stream, text views | Cycle the collapse of repeated lines: off → exact → numbers → off (per stream, persisted) |
| `ALT + W` | focused stream | Toggle line wrap (per file, persisted) |
| `CTRL + A` | focused stream | Select every visible row |
| `CTRL + C` | focused stream | Copy the selected rows (or the current hit) as plain text; every line a selected collapsed group hides is included |
| `CTRL + F2` | focused stream | On the current row (the selection, else the current hit, else the top row): remove a manual bookmark with its note, dismiss an automatic bookmark, or else add a manual bookmark |
| `F2` / `SHIFT + F2` | focused stream | Next / previous bookmark, manual or automatic, among the visible lines (wraps); select it and centre it |
| `Enter` / `Esc` | bookmark note editor | Save / cancel the note |
| `Enter` / `Esc` | time range popup | `Enter` in a field applies the draft (when both sides can be read) and closes; `Esc` closes and keeps the window |
| `↑` `↓` | rows | Scroll one line (pauses follow) |
| `←` `→` | rows | Scroll 40 px horizontally; `CTRL` gives 200 px |
| `PgUp` / `PgDn` | rows | Scroll one page |
| `Home` / `End` | rows | Horizontal start / end |
| `CTRL + HOME` / `CTRL + END` | rows | Top (pauses follow) / bottom (resumes follow) |
| `↑` `↓` `PgUp` `PgDn` `Home` `End` (`CTRL + HOME` / `CTRL + END`) | results pane or Find results with focus | Move the selection. In Find results, reaching a hit shows it in its stream at once |
| `Enter` | results pane | Make the selected hit current and centre it (pauses follow) |
| `Enter` | Find results | Toggle the group on a header, or show the hit |
| `Enter` | Find results query box | Run the search and move focus to the results |
| `Enter` / `Esc` | pattern prompt, preset modal, lock | Submit / cancel (the lock accepts only `Enter`) |

The Help window lists a subset of these. It does not list `CTRL + HOME` / `CTRL + END`, the arrow keys, Page Up / Page Down and Home / End navigation, or the keys of the note editor.

### 7.3 Mouse interaction

- **Rows.**
  - Click selects a row. `SHIFT + click` extends the selection over the visible rows. `CTRL + click` toggles a row. On a closed collapsed group, the row stands for every line of the group.
  - Right-click opens the row context menu (§4.13), on any row.
  - Clicking `[+] JSON` expands or collapses the pretty-printed JSON.
  - Clicking a `×N` badge expands or collapses that group; hovering it shows the repetitions, the line span and the times.
  - Hovering the `✏` marker shows the bookmark note.
  - The wheel scrolls; in wrap mode, a far scroll-bar drag jumps by estimate.
- **Overview strip:** click or drag to centre the view there; hover shows the line and the notes of nearby bookmarks.
- **Timeline:** click a column or drag a span to set the time range; hover shows the counts.
- **Time span** (stream bar): click to open or close the time range popup; in the popup, click a day, drag or type in the spinners, a click outside cancels.
- **Results pane:** drag its top edge to resize (persisted, shared). Click a hit to commit it and focus the list.
- **Find results:** click a header to collapse or expand it. Click a hit to show it. Right-click a hit for "Show in context".
- **Dock:** drag a tab to split, re-dock or float it. Drag the separators to resize. Tab ✕ closes.
- **Title bar:** drag the title label or the free area to move the window; double-click the free area to maximize or restore; click the zoom % to reset.
- **Borderless:** edge and corner resize zones with resize cursors.
- **Dialogs:** a Move cursor on the title strip, a pointing hand on the collapse and close buttons.
- **Stream bar:** click `📦 size` to cycle units; click the toggles.
- **Drag and drop:**
  - Files open as streams. gzip, bzip2, xz, zstd, zip and tar files are recognised by content; a multi-entry zip or any tar opens the archive picker (§4.8).
  - A **folder** opens the pattern prompt with `folder\*.log`.
- **Any input** (key, pointer move, click, wheel, text) resets the screensaver timer. While the screensaver runs, any raw event dismisses it.

---

## 8. Internationalization

Source: `src/i18n.rs`, with call sites across `src/ui/*.rs`

### 8.1 Languages

There are 16 languages. `Language::ALL`, in the order of the settings picker:

| # | Enum | Code | Native name |
|---|---|---|---|
| 1 | `En` | `en` | English |
| 2 | `De` | `de` | Deutsch |
| 3 | `Es` | `es` | Español |
| 4 | `Fr` | `fr` | Français |
| 5 | `It` | `it` | Italiano |
| 6 | `Nl` | `nl` | Nederlands |
| 7 | `Pl` | `pl` | Polski |
| 8 | `PtBr` | `pt-BR` | Português (Brasil) |
| 9 | `Tr` | `tr` | Türkçe |
| 10 | `Ru` | `ru` | Русский |
| 11 | `Uk` | `uk` | Українська |
| 12 | `Ja` | `ja` | 日本語 |
| 13 | `Ko` | `ko` | 한국어 |
| 14 | `Zh` | `zh` | 中文 (简体) |
| 15 | `ZhTw` | `zh-TW` | 中文 (繁體) |
| 16 | `Fur` | `fur` | Furlan (Friulian) |

### 8.2 Detection

`Language::detect()` reads `LC_ALL`, `LC_MESSAGES` and `LANG` first, stripping codeset suffixes. On Windows it then uses `GetUserDefaultUILanguage()`; the sub-language distinguishes zh-TW, zh-HK and zh-MO (Traditional) from Simplified Chinese.

`from_code` matches prefixes: `zh-tw`, `zh-hk`, `zh-mo` and `zh-hant` map to Traditional, and any `pt` maps to Brazilian Portuguese. Unknown codes fall back to English.

`language_auto = true` (the default on a fresh install) keeps following the OS. Picking a language in Settings turns it off; the "System language" entry turns it on again.

### 8.3 Lookup: `t(lang, key)`

- `pub fn t(lang: Language, key: &str) -> &'static str` is one large `match (lang, key)`.
- Localized arms come first, `(Language::It, "open_file") => "Apri File"`. Each of the 15 languages other than English has 443 arms, one per English key.
- The English arms come last as wildcards: `(_, "open_file") => "Open File"`. They serve English and act as the fallback for any key a language lacks.
- An unknown key returns the literal `"Unknown"`.
- The UI calls `t()` every frame with the current `config.language`, so a language switch applies on the next frame with no restart. The config is saved when the theme or language changes.

### 8.4 Placeholders

There is no formatting engine. Strings carry `{name}` tokens that the call site fills with `.replace(...)`:

- `{line}`, `{n}`, `{secs}`, `{limit}`, `{size}`, `{volume}`, `{path}`, `{lines}`, `{error}`, `{name}`, `{delta}`, `{hits}`, `{streams}`, `{total}`, `{running}`, `{queued}`, `{age}`, `{method}`, `{first}`, `{last}`, `{rows}`.
- Examples: `context_banner` → "Filters suspended: line {line} shown in the full log"; `lock_cooldown` → "Too many attempts: try again in {secs} s"; `find_all_summary` → "{hits} matches in {streams} of {total} streams"; `collapse_badge_tip` → "{n} repeated entries, lines {first}–{last}"; `collapse_rows` → "{rows} rows shown"; `auto_bookmarks_capped` → "auto-bookmarks capped at {n}".

Numbers are grouped with `,` by `group_thousands` in every language. Times come from `crate::timestamp::format_*`.

### 8.5 Layout robustness

- The lock prompt measures its translated lines to size itself. A unit test checks that Russian gets a wider dialog than English.
- The language picker is a combo rather than a row of buttons, so it stays on one line whatever the number of languages.
- CJK scripts rely on the Windows font fallbacks (§6.7).
- Some strings stay hard-coded English:
  - the BareTail dialog counts;
  - `FG:`, `BG:`, `Regex`, `B`, `I`;
  - the theme names Tron, Matrix and Blade;
  - the native file dialog titles and filters ("Open Log Files", "FastTail session", …);
  - the filter hints (`ERROR|CRITICAL|Exception...`).

---

## 9. Configuration that affects the UI

Source: `src/config.rs`, plus `src/global_filter.rs`, `src/filter_preset.rs`, `src/session.rs`, `src/renderer.rs` and `src/cli.rs`

**Location of `fasttail.ini`,** in priority order:

1. the `FASTTAIL_CONFIG` environment variable, or `--config`;
2. `./fasttail.ini`;
3. next to the executable;
4. `%APPDATA%\FastTail\fasttail.ini` (or `$XDG_CONFIG_HOME` / `~/.config`);
5. otherwise the executable's directory. If that directory is read-only, saving falls back to the per-user path.

A legacy `fasttail.toml` is migrated. The file is written only when its content changes.

### 9.1 `[general]`

| Key | Default | UI effect |
|---|---|---|
| `theme` | `Tron` | Tron, Matrix, Blade or Light (§6) |
| `language`, `language_auto` | detected, `true` | UI language (§8) |
| `renderer` | `auto` | `auto`, `glow`, `wgpu` or `software`; applies at the next start |
| `always_on_top` | false | window level; 📌 |
| `flash_on_alert` | false | taskbar attention for background errors |
| `level_colors` | true | colour rows by level |
| `search_pane`, `search_pane_height` | false, 180 | results pane on or off, and its height (accepted 40–4000) |
| `overview_strip` | true | minimap beside the scroll bar |
| `timeline_search_lane` | true | hit lane in the timeline |
| `screensaver_enabled`, `screensaver_timeout_mins` | true, 10 | Matrix screensaver |
| `lock_enabled`, `lock_pin` | false, empty | PIN lock (the PIN is stored scrambled with FNV-1a) |
| `telemetry_enabled` | true | CPU and RAM meters in the title bar |
| `sound_enabled` | false | UI sound effects |
| `borderless` | false | custom chrome, no OS decorations |
| `show_line_numbers` | true | `# 123` |
| `show_time_delta`, `time_delta_gap_ms` | false, 1000 | Δt column and its gap tint |
| `font_size` | 13 | log font size (8–32) |
| `zoom_factor` | 1.00 | interface zoom (0.5–3.0) |
| `poll_interval_ms` | 250 | repaint and poll cadence on a GPU (50–5000) |
| `size_check_interval_ms` | 500 | fallback size check (50–10000) |
| `max_fps` | 60 | frame cap on a GPU renderer |
| `max_fps_software` | 30 | frame cap on software rendering |
| `mouse_throttle_ms` | 100 | pointer-move frame throttle (0–1000) |
| `markdown_max_mb` | 1 | MD view size limit |
| `auto_bookmark_max` | 10000 | automatic bookmarks kept per stream (clamped to 100–100 000); the `ⓘ auto-bookmarks capped at N` notice |
| `spool_dir`, `compressed_max_gb`, `stdin_spool_max_mb` | temp, 20, 2048 | compressed and stdin spools |
| `size_unit` | `Bytes` | Bytes, MB, GB or Hex for the 📦 size |
| `baretail_import`, `baretail_prompt_shown` | false, false | BareTail dialog |
| `session_file` | — | current named session (title suffix) |

Environment overrides for support: `FASTTAIL_POLL_INTERVAL_MS`, `FASTTAIL_SIZE_CHECK_INTERVAL_MS`, `FASTTAIL_MAX_FPS`, `FASTTAIL_MAX_FPS_SOFTWARE`, `FASTTAIL_MOUSE_THROTTLE_MS`, `FASTTAIL_MARKDOWN_MAX_MB` and `FASTTAIL_RENDERER`.

### 9.2 Other sections

| Section | Keys | UI effect |
|---|---|---|
| `[window]` | `x`, `y`, `width`, `height`, `maximized`, `minimized` | window geometry (§2.1) |
| `[dialogs]` | `settings_open`, `filters_open`, `about_open`, `help_open`; `*_pos` / `*_size` as `x,y` / `w,h` | dialogs reopen where and as they were |
| `[dock]` | `layout` (RON) | dock tree and floating windows |
| `[open_files]` / `[recent_files]` / `[recent_sessions]` | `file_N` | restored streams, 🕒 menu (15), 🗂 recent menu |
| `[search_history]` | `query_N` | search 🕒 menu |
| `[bookmarks]` | `file_N`, `lines_N`, `note_<N>_<line>` | manual bookmarks (`★`, `✏`) and their notes, one key per note so a note that does not read back loses only itself (at most 50 files × 1000 lines; a note of a line not kept is dropped). Automatic bookmarks are not stored |
| `[wrapped_files]` | `file_N` | ↩ Wrap per file (at most 50) |
| `[session]` / `[stream_N]` | `path`, `rel`, `entry`, `include[.n]`, `exclude[.n]`, `search`, `wrap`, `encoding`, `ansi`, `timeline`, `collapse`, … | per-stream state of the default workspace. `collapse=exact` or `collapse=numbers` is written only when the collapse is on; a missing or unknown value reads as off. In a named session file the same sections also carry `bookmarks` and `bookmark_note.<line>` |
| `[highlight_N]` | `pattern`, `is_regex`, `case_sensitive`, `fg`, `bg` (`r,g,b`), `bold`, `italic`, `sound_alert`, `enabled`, `captures_only`, `bookmark` | Color Filters; `bookmark=true` is "Bookmark matching lines" (default false) |
| `[tool.N]` | `name`, `program`, `args`, `shortcut`, `rule`, `shell`, `match` | external tools, menus, shortcuts |
| `[filter_preset.N]` | `name`, `include.n`, `exclude.n`, `case_sensitive`, `regex`, `min_level`, `show_unknown_levels`, `time_from`, `time_to` | Presets ▾ |
| `[global_filter]` | `enabled`, `bar_open`, `case_sensitive`, `regex`, `include.n`, `exclude.n` | global filter bar |

Command-line options that shape the UI:

- `--fresh`: empty workspace, no layout.
- `--renderer <name>`.
- `--session <file>`.
- `--config <file>`.
- `--filter`, `--exclude`, `--follow`, `--no-follow`: applied to the files named on the command line.
- `-`: stdin tab.

---

## 10. Design conventions, principles and known limitations

### 10.1 Principles observable in the code

1. **Never block the UI thread on the whole file.**
   - Every draw path reads only the visible rows through the engine's block cache (`show_rows`, the anchored wrap layout, virtualized hit lists).
   - Large-file indexing, filtering, searching, level detection, timestamp timing, automatic bookmarks and the collapse detection run as background scan jobs with progress in the stream bar.
   - Find all runs at most four jobs, and others are queued.
   - Decompression, the tar header scan and stdin spooling run on threads. The tar peek of a codec file, done on the UI thread when it is opened, is capped at an 8 MiB decoder window and cached per path, size and modification time.
   - The global filter refilters small files synchronously within a 32 MB-per-frame budget.
2. **Repaint only when needed.**
   - egui repaints on demand only.
   - Watcher threads wake the window.
   - Every periodic repaint is explicit and throttled (§1.5).
   - The screensaver does not start in an unfocused window.
3. **Frame budgets for weak hardware.**
   - Software-rasterizer detection strips feathering, shadows and rounding.
   - Frames are capped, pointer-move frames are throttled, and the idle poll drops to 2 s.
   - glow vsync is off, to avoid busy-waiting drivers.
4. **Caches keyed by generations.**
   - The overview strip and timeline caches live in egui temp memory and are keyed by `search_generation`, `filter_generation`, `buffer_generation`, `bookmarks_generation`, `histogram_generation` and the size.
   - They are rebuilt at most every 250 ms while a stream grows.
   - The global filter's regex validity check is cached by a hash of the terms.
   - The bookmark a closed collapsed group hides is cached per row, keyed by the filter and bookmark generations (`hidden_bookmark`).
5. **Layout and state persistence everywhere.**
   - Dock layout, floating windows, window geometry, dialog geometry, zoom, pane height and toggles are persisted.
   - Writes are debounced (2 s layout, save on change) and skipped when nothing changed.
   - Results (Find results) and stdin are deliberately not persisted.
6. **Deferred mutation.**
   - The dock viewer never mutates the dock while it draws. It leaves requests (`find_all.jump`, `find_all_request`, `preset_events`, `tab_closed`, `labels_changed`, dirty flags) that the app applies after `DockArea::show_inside`.
   - Menus record an action (`SessionAction`, `RowMenuPicks`, the badge toggle) and run it after they close or after the rows are drawn.
7. **One source of truth per preference.**
   - Zoom is egui's `zoom_factor`, which `CTRL +/-/0`, `CTRL + wheel` and Settings all move. An earlier bug zoomed twice.
   - Line numbers, the Δt column, the overview strip, the results pane and the size unit are global.
   - Wrap, encoding, ANSI mode, collapse mode, timeline, filters and bookmarks are per stream. `auto_bookmark_max` is global.
8. **Keyboard scoping.** Shortcuts act on the focused stream only. Conflicting shortcuts are consumed before the dock. Text boxes and hit lists own the keys while focused.
9. **Visible state.**
   - Hidden conditions are surfaced: `🌐` badge, `+N` term badge, `name *` preset, "partial content", "sampled" tooltip, capped-results note, "auto-bookmarks capped", "rows shown", the partial-list notice of the archive picker, `(Closed)` tab, `⚠ invalid`.
   - Toggles use a filled, stroked style so "on" is readable on Light.
10. **House style.**
    - Monospace text, emoji as icons.
    - The accent colour means active or current, warn means attention or pending, dim means inactive.
    - Shortcuts are written as `CTRL + K` in the help.
    - Comments explain *why* a behaviour exists, often with the bug that motivated it.

### 10.2 "Show in context"

Merged in #122. Sources: `src/tail_engine.rs` (`enter_context`, `leave_context`, `context_line`, `rows_filtered`), `src/ui/dock.rs`, `src/ui/find_results.rs`, `src/ui/hit_list.rs`.

- **Entering.** `CTRL + K` on the focused stream (the selection anchor, or the first selected line), `◆ Show in context  (CTRL + K)` in the row context menu of a filtered stream, or the same entry (and `CTRL + K`) on a Find results hit. It needs an active filter (the stream's own or the global one) and a finished line index.
- **The view.** The stream's filters and the global filter are suspended and every line is shown, with the line centred and selected and follow paused. Nothing is recomputed: `filtered_lines` stays exact and keeps growing meanwhile. The line carries the `◆` marker, the banner shows under the filter row, and the filter row is dimmed to 45 % opacity with the banner text as the Include label's tooltip. The collapse of repeated lines does not apply in this view: it is the full log. From the context view, the menu entry on another line (or a Find results hit of the same stream) only moves to that line; `CTRL + K` on the stream goes back.
- **Leaving.** `[Back to filtered view]`, `Esc` (no widget focused) or `CTRL + K` restore the selection, the follow state and the scroll position of the moment the view was entered (in wrap mode, the line is centred). Editing a filter also ends it: the new filter applies, and the line stays centred when it is still visible. Switching to HEX or MD, or a reload of the file (truncated, rewritten, re-decoded), ends it too.
- **i18n and Help.** Its strings exist in all 16 languages, and the Help window lists `CTRL + K`.

### 10.3 Known limitations and oddities (from the code)

- **Double title bar in decorated mode.** The in-app title bar is always drawn, so with OS decorations on there are two title strips.
- **`ALT + 1..9`** follows engine order (the `[#N]` numbers), not the visual order of tabs in the dock.
- **Legacy tab kinds.** `FastTailTab::Filters`, `Highlights` and `Settings` are still rendered by the viewer but are never created.
- **Wide stream bar.** It is a single non-wrapping row and can overflow narrow windows.
- **Approximate scroll thumb in wrap mode.** The height is estimated from the average row height.
- **Borderless close** calls `std::process::exit(0)` right after saving and deleting its spools, bypassing eframe's normal shutdown.
- **Failures.** A failed export shows in the stream bar and a failed session save in the notice window; a failed session load shows as a "missing" entry.
- **Hard-coded English strings** remain (§8.5).
- **Help omissions.** The Help window does not list the plain navigation keys or the note editor keys (§7.2).
- **Compressed tar with a large window and no tar name.** Whether a gzip, bzip2, xz or zstd file holds a tar is decided on the UI thread when it is opened (`codec_holds_tar`): a tar-like name (`.tar.*`, `.tgz`, `.tbz`, `.tbz2`, `.txz`, `.tzst`) is believed, and any other file is peeked at with the decoder window capped at 8 MiB. An xz (or zstd) tar whose dictionary (window) is larger than 8 MiB and whose name does not say tar is therefore first taken for a single compressed log: its extraction finds the tar header and stops with `🗜 this file holds a tar archive: open it again to choose its entries`, and records the answer, so opening it again shows the entry picker. It costs one extra open.
- **Tar scan bounds.** The picker lists at most 100 000 entries or 1024 GB of decoded data; past that the list is partial and says so.
- **Decoder windows.** An xz dictionary or zstd window over 256 MiB is refused (`the decoder window exceeds the 256 MiB limit`).
- **Automatic bookmarks and dismissals are not saved.** They are recomputed from the rules when a stream opens, so a dismissed automatic bookmark comes back after a restart. A note turns a line into a manual bookmark, which is saved.
- **Group rows and bookmarks.** A closed collapsed group's row shows the marker of a bookmark it hides, but the note tooltip and the "Remove bookmark" menu entry only look at the row's own (first) line.
- **The lock is a deterrent.** The FNV-scrambled PIN and the `joshua` backdoor are documented as such.
