# Changelog

All notable changes to FastTail are listed here. The section matching a
release tag is used as the body of the GitHub Release; GitHub appends the
list of merged pull requests and the compare link below it.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Capture-group highlighting.** A regex highlight rule can tick "Captures
  only" to paint just its capture groups (the whole match when the pattern has
  no group) instead of the row; `req=(\d+)` colours the request id alone. Rules
  keep their top-down priority per byte, whole-row rules still colour the rest
  of the row, and at most 64 spans are painted per row. Works in extend and
  wrap mode.
- **Quick colour labels.** `Ctrl+Shift+1..9` turns the current search text of
  the focused stream into a label painted with preset colour 1..9 in every
  stream; the same key again removes it, another digit recolours it. Labels are
  listed in a strip above the rows with a remove button, rank below the
  highlight rules and live in memory only.
- **Log level detection.** The level of every line (`FATAL`, `ERROR`, `WARN`,
  `INFO`, `DEBUG`, `TRACE`, plus syslog `<n>` priorities) is detected from the
  common layouts without configuration: the first level word in the line
  header, bracketed, bare, `level=error` or `"level":"debug"` alike. Levels
  are cached per line (1 byte per line) and detected on a worker thread for
  large files, with progress in the stream bar.
- **Rows coloured by level.** `FATAL`, `ERROR`, `WARN`, `DEBUG` and `TRACE`
  rows get the theme's level palette when no highlight rule matches them, so
  existing colour rules keep priority. A Settings checkbox turns it off.
- **Minimum-level filter.** A `≥ level` selector above the buffer hides the
  lines below the chosen level, combined with the include / exclude filters
  (exclude, then include, then level). Stack-trace continuation lines follow
  their parent; a `?` toggle shows or hides lines without a detectable level.
- **Per-level counters** in the stream bar, updated live as the file grows.
- **Line wrap.** A per-stream `↩ Wrap` toggle (`Alt+W`) soft-wraps long lines at
  the window width instead of scrolling horizontally. Wrapped rows keep their
  line number, marker, colours and JSON expander; search jumps, bookmarks,
  go-to, arrows and paging keep navigating by line. Only the rows in view are
  laid out (each capped to 64 KB), so wrapping costs the same on a 10 GB file;
  the scroll bar thumb is approximate in wrap mode. The toggle is saved per
  file in `fasttail.ini`.

- **External tools.** Settings gain a list of user-defined commands with an
  argument list using the placeholders `{line}`, `{file}`, `{dir}`, `{lineno}`,
  `{selection}` and `{match}` (first capture of the tool's regex). Tools run
  on a row from the right-click menu, the stream menu or a shortcut such as
  `Ctrl+Shift+F9`, and can be bound to a highlight rule to run when it matches
  an appended line (once per second per tool, at most 10 children, dropped
  runs counted in Settings). Arguments are passed as separate argv entries
  without a shell; the "run via shell" flag (`cmd /c`, `sh -c`) is off by
  default because it lets the row text reach a shell parser. Persisted as
  `[tool.N]` sections of `fasttail.ini`.
- **Directory wildcard tail.** A stream can be opened from a pattern such as
  `C:\logspp-*.log` (the `📂*` prompt, a folder dropped on the window, or
  the command line): it tails the newest matching file and switches by itself
  when a newer one appears, checking the folder every 2 seconds. Filters,
  highlight rules, search, wrap and encoding survive the switch; buffer,
  bookmarks and selection start over, and the stream bar shows the pattern,
  the current file and a 5-second "switched to" notice. The pattern is what
  the workspace and the recent list remember; while nothing matches the
  stream waits and picks up the first file that appears.

### Changed

- **Recent Files is an icon button (🕒) next to Open File.** The localized
  name is its tooltip; the menu content (recent list, clear entry) is
  unchanged. The title bar is narrower and the two ways of opening a log sit
  together.
- **wgpu is now the first choice of the `auto` renderer**, OpenGL the fallback.
  Measured on an NVIDIA Windows machine with the pointer moving over the
  window: a continuous repaint costs about 12% of one core on wgpu against a
  full core on OpenGL, whose driver busy-waits for the vertical blank. The
  OpenGL path now runs without vsync (100% -> about 35% of a core) for the
  machines that fall back to it or pin it. The status chip reads `GL fallback`
  when the retry happened.
- **Screensaver.** It no longer starts, and stops, when the window does not
  have the focus, so it cannot animate unseen behind other windows; the
  animation is capped at 30 fps.

### Fixed

- The screensaver timeout field accepts 0 (= never); it used to clamp 0 to 1
  minute, so the timeout could not disable the screensaver.
- The About dialog links (`www.baccan.it`, the GitHub repository) open the
  default browser again: the `links` feature of eframe had been dropped with
  the explicit feature list, so clicks only produced a log warning. The links
  now also show the full URL as a tooltip, and a test guards the feature.

## [0.3.0] - 2026-09-18

### Added

- **Row selection, copy and export.** Click, Shift+click and Ctrl+click select
  rows (ranges follow the visible order under the active filters), Ctrl+A
  selects every visible row, Ctrl+C copies the selection or the current search
  hit as plain text. A save menu in the stream bar exports the visible lines
  or the search matches to a file.
- **Go to line (Ctrl+G).** An inline box in the focused stream accepts an
  absolute line number or `+N` / `-N` relative to the current line. Numbers
  past the end clamp, a line hidden by the filters resolves to the next
  visible one. The target row is centred and selected, follow mode pauses.
- **Always on top.** Title-bar pin, Settings checkbox and Ctrl+Shift+T keep the
  window above the others; the choice is persisted in `fasttail.ini`.
- **Line bookmarks.** Ctrl+F2 toggles a bookmark on the current line, F2 and
  Shift+F2 jump to the next and previous one with wrap-around, respecting the
  filters. Bookmarks show a star in the marker column, are persisted per file
  and restored on reopen; the stream menu clears them.
- **Background-tab activity badge.** Tabs that are not displayed show the
  number of lines appended since they were last shown (capped at 999+),
  coloured by the most severe highlight rule among them. An optional setting
  requests OS attention when a sound-alert rule matches in a hidden tab while
  the window is unfocused.
- **Scan progress in the stream bar.** While a background job runs, the bar
  shows `indexing / filtering / searching NN%` with the hit count so far.

### Changed

- **Files are no longer held in memory.** The engine reads on demand through
  a small block cache (16 x 256 KB); indexing, filters and search stream the
  file in 1 MB chunks. Resident memory per stream is the line index (8 bytes
  per line) plus at most 4 MB of cache, whatever the file size. Truncation
  drops cache and index and returns their memory.
- **Background scans on large files.** Above 16 MB, include/exclude filtering
  and search run on a worker thread; above 256 MB the line index is built
  there too. The UI keeps repainting, a newer filter or search cancels the
  job it replaces, and appended lines are picked up when the job ends.
- **Incremental line index on append.** Growing files no longer trigger a
  full rescan on every poll: only the tail from the last (possibly partial)
  line is indexed, using `memchr` for the byte encodings. On a 100 MB file
  20 append polls went from 1570 ms to 27 ms, opening from 131 ms to 55 ms.
- **Stronger rewrite detection.** A file reset and regrown past its old size
  with the same header is detected by also comparing the 64 bytes where the
  old data ended, so old and new content are never spliced.
- Lines longer than 1 MB are shown truncated with a marker. Rendered Markdown
  is refused above 32 MB with a notice; the text view stays available.
- README documents the memory model. All new UI strings are localized in the
  five supported languages.

### Removed

- The `memmap2` dependency: the block cache replaced the memory map.

## [0.2.0] - 2026-09-18

### Added

- **Command line arguments.** `fasttail [OPTIONS] [PATH...]` with `--fresh`,
  `--filter`, `--exclude`, `--follow` / `--no-follow`, `--renderer`,
  `--config`, `--version`, `--help` and `--`. Files are opened after the
  restored workspace, missing ones are reported, usage errors exit with 2.
  On Windows `--help` and `--version` attach to the parent console.
- **wgpu renderer fallback.** Machines without a usable OpenGL driver (Remote
  Desktop, VMs, basic adapters) could not start FastTail at all. Startup now
  tries OpenGL first and, with `renderer=auto`, retries with wgpu. The
  `FASTTAIL_RENDERER` variable, the `renderer` config key and Settings force
  `auto`, `glow` or `wgpu`. The status bar shows the active backend with the
  adapter in its tooltip and in About.

### Fixed

- Crash `index out of bounds` after closing a floating dock window: its
  surface index stayed in the saved window rectangles and was dereferenced
  without a bounds check. Stale entries are now pruned before rendering and
  before saving the layout.

### Changed

- The filter benchmark moved from `examples/` to `benches/` and generates its
  own deterministic log (`FASTTAIL_BENCH_BYTES`, default 200 MB) unless
  `FASTTAIL_BENCH_LOG` points to an existing file.
- The Windows ARM64 release target was dropped: Windows on ARM runs the
  x86_64 executable under emulation.

## [0.1.0] - 2026-09-18

First public release: multi-stream tail with docking tabs, include/exclude
filters, highlight rules with sound alerts, search, HEX and Markdown views,
encoding detection, localized UI and a CI pipeline that publishes Windows,
Linux and macOS builds on every `v*` tag.

[Unreleased]: https://github.com/matteobaccan/FastTail/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/matteobaccan/FastTail/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/matteobaccan/FastTail/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/matteobaccan/FastTail/commits/v0.1.0
