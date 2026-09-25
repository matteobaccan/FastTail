# Changelog

All notable changes to FastTail are listed here. The section matching a
release tag is used as the body of the GitHub Release; GitHub appends the
list of merged pull requests and the compare link below it.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Fixed

- **The About dialog and the status bar are translated everywhere.** "Website" in About
  and "No file open" in the footer were English in every language; both now follow the
  interface language like the rest of the window.

## [0.9.0] - 2026-09-25

### Added

- **Time range filter and go-to-time.** A `from / to` window above the buffer keeps only
  the lines stamped inside it, combined with the include/exclude and level filters; both
  sides are optional. Timestamps are read from the line itself — ISO 8601, syslog,
  Apache/nginx, epoch seconds or millis, within the first 64 bytes, no format to
  configure — and a line without one inherits the entry above it, so a stack trace stays
  with its error. `Ctrl+G` now accepts `14:02` as well as a line number, jumping to the
  first line at or after it, and the stream status bar shows the span of the visible
  lines. The fields take `14:02`, `14:02:05`, `YYYY-MM-DD HH:MM[:SS]` or a timestamp
  pasted from a line, and times are compared on the clock the log printed: a zone suffix
  such as `+0200` is not applied. A log whose lines FastTail cannot time disables the
  controls with a hint instead of hiding everything.

### Changed

- **Empty streams say why they are empty.** An open stream with no rows used to show
  "No file open"; it now says the file is empty, that no line matches the active
  filters, or which background scan (indexing, filtering) is still running.
- **The Windows archive no longer carries the debug symbols.** `fasttail-windows-x86_64.zip`
  holds the executable alone — about 8 MB instead of 24 — and `fasttail.pdb` ships as
  `fasttail-windows-x86_64-symbols.zip` for whoever needs to read a crash dump.

### Fixed

- **Config and session saves refuse a path that is not a regular file.** Saving
  `fasttail.ini` or a `.fasttail-session.ini` onto a directory, a FIFO or a device now
  fails with an error instead of blocking on the open or truncating it.

### Performance

- **Highlighting the first span of a row allocates nothing.** `claim_span` pushes the
  first match directly and reuses two buffers for the rest, instead of a new vector per
  existing span.

### Known issues

- **The first use of the time controls reads the whole file on the UI thread.** On a
  multi-GB log the window pauses until every line has been timed, once per stream; after
  that the cache is kept up to date as the file grows. Moving this scan to a background
  job is planned for 0.9.1.

### Documentation

- **Specs and cookbook checked against the code.** The OpenSpec specs now describe the
  Windows symbols archive, the 1 MB Markdown default, the vsync rule per backend, the JSON
  toggle, the About dialog, the zoom keys, the empty-stream messages and the time range;
  the external tools cookbook's JSON and clipboard recipes are scripts that work with the
  argument quoting FastTail applies.
- **Logo.** The banner pills read RUST · GPU, ZERO-LAG ENGINE, REGEX FILTERS and TIME
  RANGE instead of "MMAP ENGINE" (the engine never memory-maps the file) and "MULTI-TAB",
  and no longer run under the signature.
- **FAQ.** The README answers the questions people search for — BareTail alternative,
  `tail -f` GUI on Windows / Linux / macOS, huge files, filters, highlighting, rotated
  logs, time ranges and timestamp formats, privacy and licence — in plain
  question-and-answer form.
- **Social preview.** `assets/social-preview.png` (1280×640), the README banner over a
  crop of the main view, is the card shown when the repository link is shared; it is
  uploaded in Settings → General → Social preview.
- **macOS Gatekeeper.** The README explains the "Apple cannot verify fasttail is free of
  malware" dialog: the build is unsigned, and `xattr -d com.apple.quarantine ./fasttail`
  (or Privacy & Security → Open anyway) runs it. Signing and notarizing needs a paid
  Apple Developer account.
- **Distribution channels.** `docs/distribution-channels.md` records what publishing to
  crates.io, Scoop, winget or Chocolatey would take — the prepared work, the one-off
  manual steps and the secrets — so the decision can be taken later from the facts. None
  of them is wired up: releases stay GitHub archives.

## [0.8.0] - 2026-09-24

### Added

- **Follow the system language.** The language picker's first entry, "System language
  (…)", keeps the interface on the operating system language, so it changes by itself
  when Windows does; picking a language turns it off. It is how a fresh install starts,
  while a configuration written before this setting keeps the language it had.
- **The window reopens minimized.** Closing FastTail while it is minimized now reopens
  it minimized, the way closing it maximized already reopened it maximized; the saved
  position and size are no longer overwritten by the placeholder geometry Windows
  reports for a minimized window.

- **Visible, persisted interface zoom.** `Ctrl +`, `Ctrl -`, `Ctrl 0`, `Ctrl + wheel`
  and the new Settings → Zoom row all move the same value, which scales the whole
  interface and is saved as `zoom_factor` in `fasttail.ini`. The title bar shows it as a
  percentage next to the always-on-top pin (click to reset to 100%), so a stray
  `Ctrl + wheel` no longer resizes the app with nothing on screen to explain it. The log
  font size in points stays a separate setting.
- **PIN lock.** A 4 to 12 digit PIN can be set in Settings → PIN lock. With the lock
  armed, leaving the screensaver asks for the PIN, and `Ctrl+L` (or the "Lock now"
  button) locks the window on demand. While locked, an opaque animated backdrop hides
  the workspace, every keyboard shortcut is ignored (`Esc` included) and the streams
  keep tailing behind it; `Enter` confirms the PIN, a wrong one beeps, and three wrong
  ones in a row replace the entry field with a one-minute countdown. The PIN is
  scrambled before it reaches `fasttail.ini`. The lock is a deterrent against a
  passer-by, not a security boundary: the log files stay readable on disk and a
  maintenance unlock phrase always opens it.
- **Eleven more interface languages.** German, Portuguese (Brazil), Russian, Ukrainian,
  Japanese, Korean, Turkish, Polish, Dutch, Chinese (traditional) and Friulian join
  English, Italian, French, Spanish and Chinese (simplified), each with the full set of
  235 interface strings. The language is detected from the system locale (`LC_ALL`,
  `LC_MESSAGES`, `LANG` or the Windows UI language, with `zh-TW`/`zh-HK` telling
  traditional Chinese apart from simplified) and can be changed in Settings.
- **Per-script CJK font fallback.** Fonts are now loaded for each script that Windows
  provides (simplified and traditional Chinese, Japanese, Korean) instead of only the
  first one found, so Japanese and Korean no longer render as empty boxes.

### Fixed

- **Dialog sizes were not kept.** The Color filters, About and Help windows reopened at
  the size of their content instead of the size they were left at: their scroll areas
  auto-shrank, the window hugged them, and what was saved to `fasttail.ini` was that
  content height rather than the window. The scroll areas now claim the whole window,
  About is resizable like the others (it was fixed-size, so a stored size could never be
  applied), and the geometry that round-trips is the one the window really has.

- **`Ctrl + wheel` zoom did nothing.** egui turns a wheel event carrying `Ctrl` into a
  zoom delta and empties the scroll delta, so the handler that read the scroll delta
  never ran.
- **The zoom shortcuts and the settings disagreed.** `Ctrl +`, `Ctrl -` and `Ctrl 0`
  scaled the whole interface (egui applies them itself) *and* changed the log font by a
  point, while the settings buttons only changed the font. All of them now move one
  value, the interface zoom, which is persisted as `zoom_factor` and restored at
  startup; the log font size in points stays a separate setting. Settings gained a Zoom
  row next to it.

### Documentation

- **External tools cookbook.** New `docs/external-tools-cookbook.md` with ten worked
  recipes (open the row in an editor, jump to a stack frame, SSH to the host named in
  the line, open a URL or ticket, pretty-print the row's JSON, grep the file on disk,
  fire a webhook from a rule-bound tool) plus the habits that keep them safe. Linked
  from the README and from the External tools section of the settings.
- **Specs realigned with the code.** `localization-i18n` describes the sixteen
  languages, BCP-47 detection and per-script CJK fonts instead of five languages; a new
  `window-lock` capability covers the PIN lock; `cyber-ui-docking` gains the visible zoom
  level and the stream toolbar affordances (active-toggle styling, fixed position of the
  TXT/HEX/MD switcher) and its dialog-chrome cursors; `screensaver-matrix` notes that
  dismissal can hand over to the PIN prompt; `rendering-backend` records that the
  settings label the software renderer as not recommended; `external-tools` points at
  the cookbook. `Ctrl+L` added to the README shortcut table.

## [0.7.1] - 2026-09-21

### Added

- **Software (CPU) renderer fallback.** Added `software` (alias `cpu`) as a
  `renderer` value in the CLI, `FASTTAIL_RENDERER`, the `renderer` ini key and the
  Settings dialog. It forces the wgpu CPU rasterizer (WARP on Windows, llvmpipe on
  Linux) for machines without a usable GPU, retrying with OpenGL if no CPU adapter
  can be created. Because WARP spreads rasterization across every logical core and
  keeps them busy even when the log is idle, a persistent banner warns that a GPU is
  required for optimal performance; use `auto`, `wgpu` or `glow` whenever a GPU is
  available.
- **Unbounded horizontal scrolling.** The horizontal scroll canvas is now sized from
  the widest measured row (or a fixed extent far beyond the longest line the renderer
  can produce) instead of the rendered content size, so long lines can be scrolled
  past their end without the view snapping back.

### Performance

- **Event-driven idle on software rasterizers.** With the software renderer each
  stream's filesystem watcher wakes the event loop directly instead of pumping at the
  poll cadence, keeping only a 2 s safety poll, so a static file no longer repaints
  continuously. The residual idle cost is WARP's own multi-core spin, which is inherent
  to WARP and only stops when the window is minimized.

### Changed

- **Single GUI application.** Removed the experimental terminal (TUI) frontend and its
  configuration; FastTail now ships only the desktop UI.

## [0.7.0] - 2026-09-21

### Added

- **Configurable Markdown file size limit.** Added `markdown_max_mb` (default 1 MB,
  range 1..=100 MB) in preferences under Performance & Refresh with INI persistence
  and `FASTTAIL_MARKDOWN_MAX_MB` environment variable override. Files exceeding
  the threshold remain in lightweight text streaming mode to prevent high memory
  consumption and UI freezes, with a dynamic localized tooltip.
- **Configurable mouse pointer move throttling.** Added `mouse_throttle_ms`
  (default 100 ms, range 0..=1000 ms) in preferences with INI persistence and
  `FASTTAIL_MOUSE_THROTTLE_MS` environment variable override.

### Performance

- **Mouse pointer move event coalescing.** Frames driven purely by mouse pointer
  movement coalesce and pace at 100 ms intervals, eliminating CPU spikes (previously
  reaching 100% CPU on software rasterizers like WARP/llvmpipe or virtual machines / RDP)
  without adding any latency to clicks, key presses, or mouse wheel scrolling.
- **Software-rasterizer mouse pacing.** On software rasterizers (WARP / llvmpipe /
  virtual machines / RDP) pure pointer-move frames are additionally paced to at most
  5 FPS — or the configured `mouse_throttle_ms`, whichever is lower — because every
  frame is rasterized on the CPU; hardware rendering keeps the user's own cadence.
- **Reduced per-frame cost on software rasterizers.** When the active renderer reports
  a software rasterizer, feathering (anti-aliasing), window/popup shadows, hover
  expansion and rounded corners are disabled to cut the CPU cost of every frame;
  hardware rendering keeps the full styling.
- **Fast Markdown threshold rejection.** Files larger than the Markdown size cap
  bypass full commonmark parsing and buffer duplication entirely upon opening.

### Compatibility

- **Windows 7 and Windows Server 2008 R2 support.** Completely eliminated startup
  loader crashes (`0xc0000005`, `combase.dll is missing`, `GetSystemTimePreciseAsFileTime`,
  `GetDpiForSystem`) by introducing dynamic IAT compatibility thunks:
  - Routed `WaitOnAddress`, `WakeByAddressSingle`, and `WakeByAddressAll` to
    `KernelBase.dll` on Windows 8+ or native `ntdll.dll` keyed events
    (`NtWaitForKeyedEvent` / `NtReleaseKeyedEvent`) on Windows 7 / 2008 R2, purging
    `api-ms-win-core-synch-l1-2-0.dll` from the PE import table.
  - Redirected `ProcessPrng` to `advapi32.dll!SystemFunction036` (`RtlGenRandom`),
    purging `bcryptprimitives.dll` from imports.
  - Redirected `CoTaskMemFree` to `ole32.dll`, removing `combase.dll` from imports.
  - Hooked `GetSystemTimePreciseAsFileTime` (falling back to `GetSystemTimeAsFileTime`)
    and dynamically resolved `GetDpiForSystem` (falling back to 96 DPI).

### Changed

- **Clean HEX view toolbar.** The `# 123` line numbers toggle button and `↩ Wrap`
  button are now hidden when viewing files in HEX mode, keeping the toolbar clean
  and uncluttered for byte inspection.

## [0.6.0] - 2026-09-20

### Added

- **Toolbar action tooltip localization.** Hover tooltips on toolbar buttons
  (`Color Filters`, `Play`, `Pause`, `Help`, `About`) are now localized across
  all five supported languages (English, Italian, French, Spanish, and Chinese).
- **Configurable refresh cadence.** Added preferences options for tail stream
  polling interval (`poll_interval_ms`, default 250ms) and size check interval
  (`size_check_interval_ms`, default 500ms) with INI persistence and live UI
  sliders.
- **Configurable rendering frame pacing.** Added preferences sliders to configure
  target maximum FPS (`max_fps`, default 60 FPS) and software rasterizer FPS cap
  (`max_fps_software`, default 30 FPS).

### Performance

- **SIMD-accelerated case-insensitive search.** Accelerated `find_case_insensitive`
  in `tail_engine` using `memchr::memchr2` on the first character's lowercase
  and uppercase variants, speeding up ASCII highlight rule and label evaluation
  by ~18%.
- **Adaptive frame pacing for software rendering.** Paces frame rendering based
  on renderer detection (`renderer.is_software()`), capping software rasterizers
  (WARP, llvmpipe, VMs) to 30 FPS by default to dramatically reduce CPU usage
  during mouse movements.
- **No-VSync default for wgpu.** Switched wgpu presentation to `AutoNoVsync` with
  latency 1 to avoid swapchain backpressure and improve throughput.

### Security

- **Shell command injection prevention in external tools.** Quoted expanded
  placeholder values (`{line}`, `{selection}`, `{file}`) with target-shell
  specific escaping (`quote_sh_arg` for POSIX, `quote_cmd_arg` for Windows CMD)
  when executing external tools in shell mode (`use_shell = true`), preventing
  arbitrary command execution from log content.
- **Safe export file creation.** Validates regular file metadata on the opened
  file handle before truncating export targets (`set_len(0)`), preventing UI
  thread deadlocks and blocking DoS when targeting non-regular files (such as
  FIFOs/named pipes or device nodes).

## [0.5.0] - 2026-09-19

### Added

- **Background stream monitoring.** Active log streams continue to poll and
  update in real time even when FastTail is idle or running in the background.

### Changed

- **Smart configuration persistence.** The configuration file (`fasttail.ini`)
  is now only written to disk if its serialized content has actually changed,
  eliminating redundant periodic disk writes.

### Fixed

- **FileSource slice-bounds crash fix.** Prevented slice out-of-bounds panic in
  `FileSource::read_with` when an underlying file shrinks while cached in memory.
- **Atomic save / file replacement detection.** Detects when files are replaced
  or saved atomically by external editors (such as Notepad or VS Code) using
  filesystem identity and handle tracking, reopening the handle and indexing
  newly appended lines.

### Performance

- **UI frame pacing.** Added 8ms frame pacing (~125 FPS cap) to prevent
  excessive repaints and high CPU usage during high-frequency mouse movements.
- **Throttled fallback size checks.** Throttled filesystem metadata checks in
  the tail engine to 500ms intervals during live UI frames.
- **Theme visual styling cache.** Cached theme visuals to prevent costly
  re-evaluation of egui context styles on every rendered frame.

## [0.4.0] - 2026-09-18

### Added

- **Named sessions.** The 🗂 menu saves the workspace (open files and patterns,
  dock layout, per-stream filters, search query, wrap, encoding and bookmarks)
  to a `*.fasttail-session.ini` file and loads it back, replacing the current
  streams; global preferences stay in `fasttail.ini`. Paths are stored absolute
  and, when the file lies under the session's folder, also relative, so a
  session saved next to a log bundle still opens after the bundle moves.
  Recent sessions menu, "save as default workspace", `*` in the title bar when
  the workspace differs from the saved session (with a confirmation before a
  load discards it), missing files listed and skipped, `--session <file>` on
  the command line. Filters, search query and encoding of every stream are now
  restored at the next start too.
- **Empty state when no file is open.** The workspace shows an icon, a short
  hint, an Open File button and quick buttons for the five most recent files
  instead of a blank area.
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

- **Faster case-insensitive search.** ASCII searches scan candidate positions
  with `memchr2` before comparing, about 30% faster on highlight rule scans.
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

- Non-regular files (directories, pipes, devices) are rejected on the opened
  handle in `FileSource::open` and `reopen`, closing a time-of-check race that
  could hang a scan thread.
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

[0.9.0]: https://github.com/matteobaccan/FastTail/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/matteobaccan/FastTail/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/matteobaccan/FastTail/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/matteobaccan/FastTail/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/matteobaccan/FastTail/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/matteobaccan/FastTail/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/matteobaccan/FastTail/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/matteobaccan/FastTail/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/matteobaccan/FastTail/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/matteobaccan/FastTail/commits/v0.1.0
