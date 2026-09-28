# A terminal FastTail — feasibility spike

Could FastTail also run in a terminal? This page reports on a prototype built to find
out: `fasttail-tui`, a second binary over the same engine, drawn with
[ratatui](https://ratatui.rs) and [crossterm](https://github.com/crossterm-rs/crossterm).
It is a spike and not a port. It shows what the engine gives for free, what a real
terminal mode would still need, and what it costs at run time.

**Nothing changes for the GUI.** The binary sits behind a new Cargo feature, `tui`, that
is off by default. `cargo build`, `cargo test`, the GUI binary and the release workflow
are unchanged. The only visible trace is three new optional dependencies in
`Cargo.lock`.

```sh
cargo build --release --features tui --bin fasttail-tui
target/release/fasttail-tui                        # the GUI's workspace from fasttail.ini
target/release/fasttail-tui app.log other.log.gz   # just these files
target/release/fasttail-tui --session incident.fasttail-session.ini
target/release/fasttail-tui --config D:\other\fasttail.ini
cargo test --features tui --bin fasttail-tui       # the prototype's 28 unit tests
```

## What works

- **The GUI's settings and workspace, read-only.** The TUI finds `fasttail.ini` as the
  GUI does (`FastTailConfig::config_path()`): `FASTTAIL_CONFIG`, then the current
  directory, next to the executable, then the per-user folder. `--config FILE` sets
  `FASTTAIL_CONFIG` exactly as the GUI's command line does. From the ini it takes:
  - **the workspace.** With no FILE arguments it opens `open_files` in the GUI's tab
    order, with each stream's saved state from `[session]` / `[stream_N]`: include and
    exclude terms, the search query, encoding, ANSI mode, collapse mode, and bookmarks
    with their notes. The order and the calls are those of the GUI's
    `apply_stream_state` and `restore_bookmarks`. Pattern entries (`logs\app-*.log`)
    open on their newest match. Files that no longer exist are skipped, and the status
    bar names them ("Skipped 1 missing file: old.log").
  - **FILE arguments.** Only those files open, but each one still gets the state the
    ini keeps for its path (filters, search, bookmarks), as the GUI does for a file it
    reopens.
  - **`--session FILE`.** A named `*.fasttail-session.ini` replaces the workspace,
    through `Session::load_from`, as in the GUI. Missing entries are reported the same
    way.
  - **global settings.** The theme (`--theme` still overrides it) and `level_colors`.
    The highlight rules go through the engine's `set_highlight_rules`: the first
    enabled rule that matches colours the row with its fg/bg/bold/italic, mapped like
    the level colours, and the level palette only applies to rows no rule matched.
    `bookmark=` rules create the engine's automatic bookmarks (the `*` in the gutter).
    The global filter (`[global_filter]`, when enabled) goes to every stream.
    `poll_interval_ms` becomes the idle wait of the event loop (50 ms while a job
    runs). `size_check_interval_ms`, `auto_bookmark_max`, the spool folder, and the
    compressed and stdin size caps apply as well.
  - **not used.** The language: the TUI's few texts (status hints, help, dialog titles)
    are English and not yet in `i18n`. Wrap, the dock layout, the window geometry, the
    timeline, quick labels, filter presets and external tools are not used either.
    The minimum level and the time range are not in the ini: the GUI does not persist
    them per stream, so there is nothing to restore.
- **It never writes `fasttail.ini`.** `FastTailConfig::load()` is bypassed on purpose,
  because it saves the ini when it migrates an old `fasttail.toml`. The TUI parses the
  file with `FastTailConfig::from_ini` and keeps every change in memory, so it cannot
  fight a running GUI over the file. A test checks that the file is byte-for-byte
  unchanged. Saving TUI state later would need a merge-on-save of the TUI's own streams
  into the file (or a separate `[tui]` section or file). The GUI rewrites the whole ini
  when it saves, so two writers would overwrite each other's changes.

- **Windows with borders.** Each stream is a bordered window: double lines when it has
  the focus (in the theme's accent colour), single lines otherwise. The top border
  holds `[#N] name` and FOLLOW or PAUSED. The bottom border holds the counts and any
  background job on the left, and the filters, level, collapse and search position on
  the right. The status line is a bordered bar. Prompts and help open as centred
  dialogs that clear what is under them, with `[ OK ]` / `[ Cancel ]` buttons.
- **Several files.** One engine per stream, plus standard input (`-`, or piped input,
  which opens next to the workspace and takes the focus, as in the GUI). A title strip appears once there are two files. `Tab` / `Shift+Tab`
  and `Alt+1..9` switch files.
- **Split.** `s` cycles through one window, two side by side and two stacked. In a
  split, `Tab` moves the focus between the two windows. `--split` starts that way.
- **Follow.** Auto-scroll to the end. `Space` toggles it. `End` / `G` jumps to the bottom
  and follows. Scrolling up, or the mouse wheel, pauses it.
- **Scrolling.** Arrows, `j` / `k`, `PgUp` / `PgDn`, `Ctrl+B` / `Ctrl+F`, `Home` / `End`, and
  `Left` / `Right` / `0` to scroll sideways.
- **Level colours.** The theme's level palette (`CyberTheme::level_style`) is mapped from
  `Color32` to 24-bit RGB. If the terminal lacks truecolor, each colour falls back to
  the nearest of the 16 basic colours. The palette is the ini's theme, or
  `--theme tron|matrix|blade|light`.
- **Search.** `/` opens the search dialog. `n` / `N` go to the next and previous hit. Hit
  rows carry a `>` in the gutter, the hits inside the text are painted, and the current
  hit's line number is reversed. `Esc` clears the search and the selection.
  `--search TEXT` searches at start.
- **Filters.** `i` / `x` edit the include / exclude filter in a dialog. `l` cycles the
  minimum level: off, DEBUG, INFO, WARN, ERROR.
- **Collapse.** `c` cycles off, exact and numbers. Collapsed groups show a `[12x]` badge.
- **Mouse** (see [Mouse](#mouse)). The wheel scrolls the window under the pointer. A
  click focuses a window and selects a row. `Shift`+click or a drag selects a range. A
  double click toggles a bookmark (shown as `*` in the gutter). Clicks work on the file
  titles and on the dialog buttons, and a click outside a dialog closes it.
- **Copy.** `y` (or `Ctrl+C` when rows are selected) copies the selection. It goes to the
  system clipboard through `arboard`, which egui already pulls in, and falls back to
  OSC 52. With nothing selected, `Ctrl+C` quits.
- **Compressed files and archives.** gzip, bzip2, xz and zstd files, a zip with a single
  entry, and any zip or tar entry named as `archive.zip/entry.log`. They go through the
  engine's own decompress-to-spool path (`compressed::classify` +
  `compressed::open_engine`), with a "decompressing N%" note in the bottom border. There
  is no entry picker, so a tar or a zip with several entries prints the path form to
  use. All four codecs, a single-entry zip and a `.tar.gz` entry were checked.
- **Quitting cleanly.** `q` quits. A panic hook first restores the terminal (mouse
  capture off, raw mode off, main screen, cursor on) and then prints the panic. The
  same restore runs on a normal exit.
- **Resize.** Resize events trigger a redraw at the new size.
- **ASCII fallback.** `--ascii`, or `FASTTAIL_TUI_ASCII`, draws `+-|` borders (the focused
  window uses `=`). This is picked automatically on a Windows console without
  virtual-terminal support (see [Windows console notes](#windows-console-notes)).
- **A calm event loop.** Crossterm events are polled with the ini's `poll_interval_ms` as
  the timeout (250 ms by default, 50 ms while a background job runs). Input ends the
  wait at once, so keys are never delayed. Each tick calls `poll_updates()` on every engine, so hidden
  files keep tailing and their title gets a `+`. The screen is redrawn only when a
  signature of what it shows changes (line counts, generations, job progress, follow,
  focus) or on input. Only the visible rows are read from the engine.

### Captures

These are real frames, printed by `fasttail-tui --capture 100x14 ...`, which draws one
frame into ratatui's `TestBackend`. The colours are lost in text.

One file, searching `put` (`--no-follow --search put`). The bottom border says the
current hit is 1 of 4:

```text
╔ [#1] demo-a.log  PAUSED  ════════════════════════════════════════════════════════════════════════╗
║   1 2026-09-28 00:06:10.370 [DEBUG] DELETE /api/items/98078 status=200 took=470ms user=u43409 ses║
║   2 2026-09-28 00:06:11.371 [INFO] GET /api/items/72561 status=200 took=140ms user=u14975 session║
║   3 2026-09-28 00:06:12.372 [WARN] DELETE /api/items/32747 status=200 took=325ms user=u45948 sess║
║   4 2026-09-28 00:06:13.373 [INFO] DELETE /api/items/21067 status=200 took=745ms user=u30079 sess║
║   5 2026-09-28 00:06:14.374 [INFO] DELETE /api/items/37652 status=200 took=339ms user=u29572 sess║
║   6>2026-09-28 00:06:15.375 [TRACE] PUT /api/items/13016 status=200 took=780ms user=u08722 sessio║
║   7 2026-09-28 00:06:16.376 [DEBUG] DELETE /api/items/74042 status=200 took=106ms user=u45471 ses║
║   8>2026-09-28 00:06:17.377 [INFO] PUT /api/items/62534 status=200 took=274ms user=u00818 session║
║   9 2026-09-28 00:06:18.378 [INFO] DELETE /api/items/20119 status=200 took=810ms user=u24868 sess║
╚ 30/30 lines ═══════════════════════════════════════════════════════════════════════════ /put 1/4 ╝
┌ FastTail TUI ────────────────────────────────────────────────────────────────────────────────────┐
│? help  q quit  Space follow  / search  n/N next  i/x filter  l level  c collapse  s split  y copy│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Two files side by side (`--split`), the left one focused (the middle rows are trimmed
here):

```text
 1:demo-a.log | 2:demo-b.log
╔ [#1] demo-a.log  FOLLOW  ══════════════════════╗┌ [#2] demo-b.log  FOLLOW  ──────────────────────┐
║  19 2026-09-28 00:06:28.388 [INFO] POST /api/it║│  24 2026-09-28 01:23:42.022 [INFO] GET /api/ite│
║  20 2026-09-28 00:06:29.389 [TRACE] DELETE /api║│  25 2026-09-28 01:23:43.023 [WARN] DELETE /api/│
║  ...                                           ║│  ...                                           │
║  29 2026-09-28 00:06:38.398 [INFO] GET /api/ite║│  34 2026-09-28 10:00:00 [WARN] heartbeat missed│
║  30 2026-09-28 00:06:39.399 [INFO] GET /api/ite║│  35 2026-09-28 10:00:00 [WARN] heartbeat missed│
╚ 30/30 lines ════════════════════════ no filter ╝└ 35/35 lines ──────────────────────── no filter ┘
┌ FastTail TUI ────────────────────────────────────────────────────────────────────────────────────┐
│? help  q quit  Space follow  / search  n/N next  i/x filter  l level  c collapse  s split  y copy│
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

The search dialog in ASCII mode (`--ascii`):

```text
+ [#1] demo-a.log  FOLLOW  ========================================================================+
|  22 2026-09-28 00:06:31.391 [INFO] DELETE /api/items/50688 status=200 took=276ms user=u30825 sess|
|  23 2026-09-28 00:06:32.392 [INFO] POST /api/items/21010 status=200 took=595ms user=u03039 sessio|
|  24 2026-09-28 0+ Search ------------------------------------------------------+ser=u01831 sessio|
|  25 2026-09-28 0|>                                                             |er=u33099 session|
|  26 2026-09-28 0|Enter confirm   Esc cancel   Ctrl+U clear                     |ser=u41349 sessio|
|  27 2026-09-28 0|                                           [ OK ]  [ Cancel ] |user=u19196 sessi|
|  28 2026-09-28 0+--------------------------------------------------------------+er=u30881 session|
|  29 2026-09-28 00:06:38.398 [INFO] GET /api/items/9965 status=200 took=304ms user=u49625 session=|
|  30 2026-09-28 00:06:39.399 [INFO] GET /api/items/19371 status=200 took=720ms user=u34051 session|
+ 30/30 lines ========================================================================== no filter +
+ FastTail TUI ------------------------------------------------------------------------------------+
|? help  q quit  Space follow  / search  n/N next  i/x filter  l level  c collapse  s split  y copy|
+--------------------------------------------------------------------------------------------------+
```

### Not done in the prototype

- A cursor row for the keyboard. Selection is mouse-only for now, so the features that
  act on "the current row" have no key yet: show in context (`enter_context` /
  `leave_context`), bookmark navigation, notes and go-to-line. The engine calls exist.
- Captures-only highlight rules (regex groups painted inside the row) are not drawn:
  only whole-row rules colour rows. `match_highlight_spans` has the spans; merging them
  with the search-hit segments is the missing piece.
- ANSI colours in the log (`RowText::ansi`) are stripped, not rendered.
- Pattern streams (`dir/*.log`) open through `TailEngine::open_pattern` but were not
  exercised.
- Wide characters (CJK, emoji) are not measured for horizontal scrolling. Ratatui clips
  them correctly, but `Left` / `Right` step by characters, not by cells.
- Standard input was wired but not tried with a real pipe on Windows. Crossterm reads
  keys from `CONIN$`, so it should work.

## What the engine gave for free

Almost everything that matters. The prototype is about 3,200 lines, a fifth of them
tests, and not one of them touches the engine:

- the line index, built in the background on large files, with progress;
- tailing (notify watcher plus the size-check fallback), truncation and rewrite
  handling;
- the include / exclude / level filters and their background jobs;
- search with the capped hit list, next / previous, the current hit, and wrap-around
  that knows about collapsed groups;
- collapse of repeated lines, including the row mapping (`visible_line_count`,
  `get_actual_line_idx`, `collapsed_row`, `row_marks`). The view only ever asks "what is
  row N";
- selection (`select_row`, `extend_selection_to`, `copy_selection_text`) and bookmarks;
- decompression to a spool, archive entries, the standard-input spool;
- encoding detection and ANSI stripping (`get_row`).

`scroll_to_line` is the whole contract between search and the view, and the TUI
consumes it exactly as the GUI does.

## What is GUI-only and would need work

| GUI feature | In a terminal | Effort |
|---|---|---|
| Markdown view | `egui_commonmark` cannot be reused. Either show the raw text (as the TUI does now), or render with a Markdown-to-ratatui crate. | M |
| HEX view | The engine already serves the bytes (`get_bytes`, `total_hex_rows`, `search_byte_matches`), so it only needs a hex-dump widget. | S |
| Timeline histogram | The data (`time_histogram`, `TimeHistogram`) is UI-agnostic. A ratatui `BarChart` or `Sparkline` gives a coarser but usable strip. Brushing a time range with the mouse is extra. | M |
| Overview strip | `error_block_counts` feeds a one-column coloured strip next to a ratatui `Scrollbar`. | S |
| Dock layout | `egui_dock` drag-and-drop tabs have no equivalent. Fixed splits (done: 2-way) are easy; free tiling with resizable panes is real work. | M–L |
| Dialogs (settings, highlight-rule editor, presets, time range, go-to, zip / tar picker, note editor, external tools editor) | Ratatui has no form widgets. Each dialog needs text inputs, lists and checkboxes (`tui-input` / `tui-textarea` help). This is the biggest share of a port. | L |
| Mouse | Done for the basics (see below). Drag-to-resize splits, the context menus and hover tooltips are not. | S–M |
| Global filter bar | `set_global_filter(Arc<FilterSpec>)` is engine-side. It needs a bar and a dialog. | S |
| Find results (`find_all`) | The search runs in the engine. It needs a results pane with jump-to. | M |
| External tools | Launching is UI-agnostic, but `external_tools::Shortcut` stores `egui::Key` / `egui::Modifiers`. | S |
| Themes | Level colours map already. Backgrounds, panels and the "cyberpunk" chrome follow the terminal's own colours, which is what terminal users expect. | S |
| i18n | `i18n::t(lang, key)` is plain strings and reusable as is. The TUI texts are English-only now. | S |

## Mouse

Mouse capture is on by default (`EnableMouseCapture`) and is turned off by
`--no-mouse`, by the exit path and by the panic hook.

- **Wheel:** scrolls the window under the pointer, focused or not, by 3 rows, and pauses
  its follow, as the GUI does.
- **Left click:** on a window, focuses it. On a row, also selects it. On a title in the
  file strip or on a window's top border, activates that stream. On `[ OK ]` /
  `[ Cancel ]`, presses the button. Outside an open dialog, closes it like `Esc`.
- **Shift+click / drag:** selects a range of rows. A drag re-derives the range from the
  anchor, so dragging back shrinks it.
- **Double click:** toggles a bookmark (the engine's `toggle_bookmark`).
- **Copy:** `y` or `Ctrl+C` with a selection. `arboard` is already in the dependency tree
  through `egui-winit`. The TUI names it as an optional dependency with the same
  version, so no new code is compiled. When no system clipboard is reachable (SSH, a
  headless box), the text goes out as an OSC 52 escape. Windows Terminal, xterm, kitty,
  WezTerm and tmux (with `set-clipboard on`) honour it; conhost ignores it.

Hit testing is pure. Every frame records the rectangles it drew (title strip, windows
with their first row and row count, the dialog and its buttons) in a `HitMap`, and
`mouse::hit_test(map, column, row)` maps a cell to a `Target`. The mapping has its own
unit tests, and a `TestBackend` test clicks the second window of a split and checks
that its border becomes the focused one.

**Selecting text natively.** While the app captures the mouse, the terminal does not
select text. In most terminals `Shift`+drag bypasses the capture and selects natively.
This works in Windows Terminal, xterm, GNOME Terminal, kitty and WezTerm; iTerm2 and
macOS Terminal use `Option` / `Fn` instead. `--no-mouse` leaves the mouse to the terminal
entirely. The help dialog (`?`) says so.

## Measurements

The machine is Windows 11 with a shared, loaded workstation disk. The test file was a
synthetic 1.04 GB log with 10 million lines, mixed levels and runs of repeated lines
for the collapse test. It was generated under `target/` and deleted afterwards. The
build is release (thin LTO, one codegen unit).

**Engine work** was measured headless with `--bench`:

| Step | Time |
|---|---|
| Open, index and level scan (cold OS cache) | 20.0 s |
| Same, warm cache | 6.1 s |
| Search `error` (996k hits) | 2.8 s |
| Include `GET` (2.49M rows) | 2.0 s |
| Collapse "numbers" (9.16M rows) | 5.3 s |

**Render time per frame** was measured headless: a 200×60 `TestBackend`, 300 frames
spread over the whole file. This covers the engine row reads, the widget layout and
ratatui's buffer diff, but not the terminal:

| View | avg | p50 | p95 | max |
|---|---|---|---|---|
| plain | 1.30 ms | 1.36 ms | 1.70 ms | 7.5 ms |
| search active | 0.97 ms | 0.84 ms | 1.54 ms | 2.0 ms |
| include filter | 1.03 ms | 0.85 ms | 1.73 ms | 2.2 ms |
| collapsed | 1.05 ms | 0.90 ms | 1.58 ms | 2.0 ms |

**Frame time in a real console** was measured with `--scroll-test 300 --stats FILE`. The
app ran in its own 120×30 console window and paged down once per tick after indexing,
so every frame rewrites the whole screen. "draw" is the full `terminal.draw` (widgets,
diff, escape sequences, write and flush). "widgets" is our part only.

| Host | draw p50 | draw p95 | widgets p50 |
|---|---|---|---|
| Windows Terminal (default-terminal handoff, OpenConsole) | 0.64–0.83 ms | 2.2–18 ms | 0.29–0.40 ms |
| conhost (`conhost.exe fasttail-tui ...`) | 0.60 ms | 2.4 ms | 0.26 ms |

The ranges are two runs on a busy machine. The long tails are the host catching up,
not our code.

One finding matters for a real port: **buffer stdout**. The first run wrote through
plain `io::Stdout`, whose line buffer flushes every 1 KiB, and paging cost 160 ms per
frame (p50). Wrapping it in a 256 KiB `BufWriter`, so each frame reaches the console in
one write, brought it under 1 ms. That first run's cache state was not controlled, so
take the factor as indicative.

**Idle CPU:** with the 1 GB file open, following, and mouse capture on, the process used
31 ms of CPU in 12 s, which is **0.26 % of one core** (the 100 ms poll plus
`poll_updates()` on each tick). That was measured before the ini was read. The idle
wait is now `poll_interval_ms`, 250 ms by default, so idle CPU can only go down. Memory was about 97 MB working set, almost all of it the
engine's line index (10M × 8 B offsets) and level cache, which the GUI would hold too.

**Binary size:** `fasttail-tui.exe` is **3.5 MB**, against 21.7 MB for the GUI
`fasttail.exe` (same profile, debug info in the separate PDB). The linker drops egui,
wgpu and winit, because the TUI never reaches them. The *build*, though, still compiles
all of them, because the library crate is monolithic. A clean release build took about
15 minutes on this machine, and an incremental one about 3.

## Windows console notes

- **Windows Terminal vs conhost.** Both worked with the same binary. Windows Terminal
  has truecolor, box drawing, mouse input and OSC 52. The Windows 10/11 conhost also
  renders 24-bit colour and box drawing once virtual-terminal processing is on
  (crossterm turns it on), but it ignores OSC 52. The automated runs above covered both
  hosts. Keys and mouse were only exercised through the unit tests, not by hand.
- **Truecolor detection.** `COLORTERM=truecolor|24bit`, `WT_SESSION` (Windows Terminal),
  or a Windows console that accepts VT sequences (`crossterm::ansi_support`) mean RGB.
  Otherwise the 16 basic colours are used. `FASTTAIL_TUI_COLORS=16|truecolor` overrides
  the detection. `xterm-256color` without `COLORTERM` is treated as 16 colours, which is
  conservative; a real port would add the 256-colour cube.
- **Legacy console.** Without VT support (the pre-2017 conhost, or "Use legacy console"
  ticked), crossterm falls back to the WinAPI with 16 colours, and raster fonts may not
  have box characters. The TUI then switches to the 16-colour palette and ASCII
  borders by itself. Rust writes to a real console through `WriteConsoleW` (UTF-16),
  so the OEM code page (437/850) does not garble the box characters; the font is the
  risk.
- **Key events.** The Windows console reports key *releases* as well as presses.
  Without filtering on `KeyEventKind::Press`, every key acts twice. Windows Terminal's
  default key bindings take `Ctrl+Alt+1..9` for its own tabs, not `Alt+digit`, so
  `Alt+1..9` should reach the app. That was not verified by hand. `Shift`+letter arrives
  as the capital letter.
- **Mouse and QuickEdit.** In conhost with QuickEdit mode on (the default), a click
  starts a console text selection and the app never sees the mouse. Crossterm's
  `EnableMouseCapture` sets the console input mode *without* the QuickEdit flag while it
  runs, and `DisableMouseCapture` restores the previous mode. So the mouse works while
  the TUI captures it, and `--no-mouse` gives QuickEdit back. If the app were killed
  without the restore, the console would keep QuickEdit off until it is closed.
  Windows Terminal delivers mouse events to console apps that ask for them, and
  `Shift`+drag should still select natively there. Neither the mouse nor this was
  checked by hand in either host; the mouse logic is covered by the unit tests only.
- **Git Bash (mintty)** is not a console. Crossterm cannot put a pipe into raw mode, so
  run the TUI from Windows Terminal, cmd or PowerShell, or through `winpty` in mintty.
  The measurements above were taken in real console windows.
- **The GUI binary cannot host it.** `fasttail.exe` is built with
  `#![windows_subsystem = "windows"]`, so a shell does not wait for it and it has no
  console of its own. `fasttail --tui` from a prompt would need
  `AttachConsole(ATTACH_PARENT_PROCESS)` and would then share the console with the
  shell that already returned its prompt. That is a known source of garbled input.

## What a real port would need to decouple

The prototype maps `Color32` at the edge and changes nothing in the engine. A real
terminal mode should first:

1. **Colours without egui.** `HighlightStyle { fg, bg: Color32 }` and `CyberTheme`'s
   colour functions return `egui::Color32`. Use a plain RGBA type in the engine and
   theme, and keep `CyberTheme::apply(ctx)` (egui `Visuals`) in the GUI.
2. **Move `markdown_cache: egui_commonmark::CommonMarkCache` out of `TailEngine`** into
   the GUI's per-tab state. It is view state.
3. **Use a UI-neutral key type in `external_tools::Shortcut`** instead of `egui::Key` /
   `egui::Modifiers`.
4. **Split the crate.** `lib.rs` compiles `ui`, `renderer` and so eframe, wgpu, winit,
   `egui_dock`, `rfd` and `egui_commonmark` for everyone. Either add a `gui` feature,
   on by default, that gates them, or move the engine into a `fasttail-core` workspace
   crate. The TUI build would then skip wgpu and most of its 15-minute clean build.
5. **Share the "open anything" logic.** `open_log_file` / `open_engine_for` /
   `open_archive` in `ui/app.rs` (compressed dispatch, archive pickers, pending stdin,
   recent files, workspace and session restore) are GUI methods today. The TUI
   re-implemented a subset (`src/bin/fasttail-tui/workspace.rs` copies the GUI's
   `apply_stream_state` and `restore_bookmarks`). A shared, UI-agnostic workspace layer
   would serve both front ends and keep them from drifting apart. The same goes for a
   read-only `FastTailConfig::read_from(path)`: `load()` can write.
6. **Audio:** the search wrap-around beep is played from the engine through
   `crate::audio`. That is fine behind its feature, but it should be a callback or an
   event the front end decides on.

## Recommendation

**A terminal mode is feasible, and cheap to keep honest.** The engine was already built
for virtualised views and background work. The prototype reached a usable log viewer
(tabs, split windows, follow, search, filters, collapse, mouse, compressed files)
without touching it, and it redraws a full screen in about a millisecond on a
10-million-line file while idling at a quarter of a percent of one core.

**Ship it as a separate binary, not as `fasttail --tui`.** The Windows GUI subsystem
makes a console mode in the same executable awkward, and a separate binary is
one-sixth the size. A separate *crate* is the cleaner end state. A second binary in
this package, behind the `tui` feature as now, is the cheap first step. The CI would
add one more `cargo build --features tui --bin fasttail-tui` per target and one more
archive per release.

Rough effort for a real `fasttail-tui`, one developer:

| Step | Effort |
|---|---|
| Decoupling steps 1–5 above (colours, markdown cache, shortcut key type, `gui` feature or core crate, shared open/workspace layer) | 3–5 days |
| Keyboard cursor row, context view, bookmarks and notes navigation, go-to line and time, captures-only rules, quick labels and ANSI colours, saving TUI state | 4–6 days |
| HEX view, overview strip, global filter, find-results pane, archive entry picker | 4–6 days |
| The form dialogs (settings, rules, presets, time range, external tools) | 1–2 weeks |
| Timeline histogram, resizable tiling, i18n of the TUI strings, packaging in the release workflow, docs | 1 week |

That is **about 4–6 weeks** for a terminal FastTail at feature parity for everyday use.
A "viewer" subset is **1.5–2 weeks** on top of this spike: the first two rows, plus
packaging. It is what most SSH-and-tail users want, and it could ship first.
