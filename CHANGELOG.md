# Changelog

All notable changes to FastTail are listed here. The section matching a
release tag is used as the body of the GitHub Release; GitHub appends the
list of merged pull requests and the compare link below it.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Highlight-rule editor in the terminal interface (`r`).** It lists the rules in their
  order, the first that matches painting the line, each pattern drawn in its own
  colours. `Enter` edits a rule in a form with every option:
  - regex, groups only and match case;
  - bold and italic;
  - text and background colour, typed as `#RRGGBB` or picked with `[` / `]`, with a preview;
  - sound and automatic bookmarks;
  - the tool it runs, and on / off.

  `a` adds a rule, `d` deletes one, `Space` switches it on or off, and `Alt+↑` / `Alt+↓`
  (or `K` / `J`) move it. Every change reaches all streams and `fasttail.ini` at once, as
  in the GUI's Highlights window. A renamed rule keeps its tool.
- **Filter presets in the terminal interface (`p`).** These are the presets of the GUI's
  drop-down: each name comes with its filters, and the one equal to the focused stream
  is marked `=` (`~` when it was applied and edited since).
  - `Enter` applies a preset to the stream, `A` to every stream.
  - `s` saves the stream's filters as a preset, optionally with the time range. A taken
    name replaces that preset.
  - `r` renames a preset, `d` deletes it after a confirmation, and `Alt+↑` / `Alt+↓`
    move it.

  Every change is saved to `fasttail.ini`, so the GUI lists it.
- **Global filter in the terminal interface (`F`, `f`).** `F` edits the GUI's global
  filter: the on switch, up to 8 include and 8 exclude terms, match case and regular
  expressions. A regex that does not compile is marked. Edits reach every stream 300 ms
  after the last key, and streams opened later too; they are saved in
  `[global_filter]`. `f` switches it on or off at once.

### Changed

- **Disabled controls say why.** Play / Pause are disabled with no stream open; the
  Compare view's previous / next change buttons are disabled when the two sides are
  identical; the column header menu's Hide, Move left / right and column checkboxes,
  the scratchpad's find-next and the rule set export / import buttons show a tooltip
  when disabled; Find All's Refresh tells how to start a search (#216, #218, #222, #224).
- **Field and level scanning.** A line without `=` is rejected at once by the logfmt
  scanner; unescaping copies the text between backslashes in bulk; a field lookup by
  name compares keys without building each field; level words are matched by length
  first (#219, #223, #225).

### Fixed

- **Scratchpad and terminal statistics files.** Saving the scratchpad and writing the
  `--stats` report refuse a directory or another non-regular file, as the other exports
  do (#220).

## [0.16.0] - 2026-09-30

Terminal interface windows: floating windows that move, resize and overlap, `[x]` to
close, an empty workspace, and a help that shows every key. The terminal interface is
still a preview (editors, lock, hand-off and translations come with 0.20.0).

### Added

- **Floating windows in the terminal interface.** `Alt+F` takes the focused window out of
  the dock (or docks it back); a window's title dragged anywhere but onto another window's
  edge or centre floats it too. A floating window lies over the others with a shadow:
  drag its title to move it, drag its bottom-right corner (◢) to resize it, click it to
  bring it to the front; `Alt+arrows` move it. They are saved as the GUI's floating
  windows in `[dock] layout`, so each interface shows the other's.
- **`[x]` on every terminal window.** Top right of each window, docked or floating: a
  click closes the stream it shows, as `Ctrl+W` does (the window goes when it is empty).
- **Empty terminal workspace.** The last stream can be closed too, and `fasttail-tui` with
  nothing to open starts empty instead of exiting: `o` opens a file, `Ctrl+O` a session.
  The last docked window can float as well, leaving the dock empty; `Alt+F` docks a
  floating window back into it.

### Changed

- **Archive picker.** "All" is disabled when no listed entry can be opened, "None" when
  nothing is selected, each with a tooltip saying why (#200).
- **Highlighting.** A highlight span that overlaps no span already claimed on the row is
  added directly, without the subtraction pass (#203).
- **Release page.** Each release starts with a downloads grid (architecture by system,
  one link per file), generated from the files the release publishes.

### Fixed

- **Terminal help (`?`, `F1`).** It now covers the whole screen and uses up to three
  columns, so every key shows at once on a 120 x 30 terminal; a smaller one scrolls and
  says how many entries are below without covering one. The mouse gestures are listed as
  entries of their own.
- **Bookmark report export.** Saving the report refuses a directory or another
  non-regular file instead of writing through it, as the other exports do (#201).

## [0.15.0] - 2026-09-30

### Added

- **Terminal interface (preview).** `fasttail-tui` ships next to `fasttail` in every
  archive: the same engine and `fasttail.ini` in a terminal window, for SSH sessions and
  servers without a desktop. It reads the GUI's workspace and writes back only what it
  changes, so both can be used on the same configuration. This preview has:
  - windows arranged as the GUI's dock, moved and resized with the mouse (drag a
    divider, drag a title onto a window's edge or centre) or the keys (`s` `|` `_`
    split, `<` `>` move, `Alt+arrows` resize, `Alt+X` close a window), saved in the
    same `[dock] layout` and in sessions;
  - follow, search (`/`, `n` `N`, `F3`), include / exclude filters, level, collapse,
    context lines, time range with a calendar, bookmarks and notes, go to line or time,
    HEX view, ANSI colours, copy, compressed files and archive entries, standard input;
  - an Open dialog that browses folders like a file dialog, sessions (open, save as),
    the Settings dialog, the five themes (`Shift+T`) at 256 colours or truecolor,
    ASCII borders (`--ascii`) and the mouse;
  - `e` `E` / `w` `W` jumps to the next ERROR / WARN line, counts before moves (`10j`,
    `3e`), `Ctrl+W` to close a stream, and the help (`?`, `F1`) as a menu of commands.

  Still missing, planned for 0.20.0 (the terminal interface at parity with the window):
  the rule editor, filter presets, the global filter, external tools, the PIN lock,
  the command palette, switching between the two interfaces, and translations (the
  terminal speaks English for now).

- **Commander theme.** A fifth theme, the classic blue and white of the DOS file managers:
  blue screen and panels, near-white text, light cyan borders and yellow accents. Listed
  in Settings and in the command palette; stored as `theme=Commander` (older builds read
  it as Tron).

## [0.14.0] - 2026-09-29

### Added

- **Field parser for structured logs.** A stream bar chip shows how the lines' fields are
  read: JSON and logfmt are detected from the first 200 lines, and the menu forces JSON,
  logfmt, Apache / NGINX combined, syslog (RFC 3164), a regular expression with named
  groups, or off. Saved per stream as `fields_parser=` and `fields_regex=` (older builds
  ignore them). Scanners borrow the line and allocate nothing per line (~500 MB/s on
  JSON, ~700 MB/s on logfmt; `cargo bench --bench fields`).
- **Column view of the fields.** `▦ Columns` in the stream bar shows the parsed fields as
  columns with a header: the first 8 keys by default, then the message (the fields not
  shown follow it as `key=value`). Drag to move or resize a column; the header menu hides,
  moves and picks fields, or resets. Level fields take the level colour, unparsed lines
  run across the columns. Only the drawn rows are parsed (a cache of 1,024 rows). Saved
  as `fields_view`, `fields_columns`, `fields_width.<key>`. The view keeps one row per
  line, also with wrap on.

- **Relative, live time windows.** The time range fields take `-15m`, `-90s`, `-3h`, `-2d`,
  `-1w`, `-1h30m` and `now`, counted back from now on the stream's display clock. A window
  with a relative side is re-read every 5 seconds and slides: on a log whose timestamps
  never go back only the lines that left or entered it are touched, otherwise it is
  refiltered at most once a minute. The popup has a "Relative to now" row (5m, 15m, 1h, 6h,
  24h, 7d); the stream bar shows `⟳` and the tooltip the bounds in force, or since when
  there is no line. `--since` / `--until` without `--print` now keep a relative value as
  typed, so the window opened from the command line slides too.

- **Select part of a row.** Pressing on a row's text and dragging selects characters of
  that row (the row is selected too); a click leaves a caret, `SHIFT + ←/→`,
  `CTRL + SHIFT + ←/→` and `SHIFT + Home/End` move the end of the selection, a
  double-click selects the word and a triple-click the whole row, in the normal and the
  wrapped layout. `CTRL + C` then copies just those characters (as shown: ANSI handled,
  timestamps as displayed), the row menu offers "Copy selected text", `CTRL + F` puts
  them in the search box and `Esc` clears them; `SHIFT` / `CTRL` clicks keep selecting
  rows. The selection is cleared by a reload and when a filter hides its row.

- **Tray icon (Windows).** Settings → Tray icon (off by default): click to show or hide the
  window, a menu to show / hide, follow or pause every stream, mute, pick a stream or
  quit; minimise to tray and close to tray options; `CTRL + Q` quits. While hidden the
  streams, rules and sounds keep running, and a red dot on the icon (with a count in the
  tooltip) marks the alerts sounded meanwhile. New `[general]` keys `tray_icon`,
  `minimize_to_tray`, `close_to_tray`. Linux and macOS later.
- **Compare lines.** The row menu's "Compare" compares the two selected lines, or a marked
  selection with another one (any streams, up to 20,000 lines a side), in a Compare tab:
  side by side, differing tokens highlighted, rows aligned as equal / changed / removed /
  added, `F7` / `SHIFT + F7` between changes, double-click back to the line. Ignore
  options (leading timestamp by default, numbers, hex ids and UUIDs, whitespace, case),
  a JSON mode with sorted keys, and "Copy as unified diff". Uses the `similar` crate.
- **Filter as a new tab.** "Open filter as new tab" (stream menu, row menu) opens a tab
  `⧉ app.log ▸ ERROR` with the lines passing the stream's filter as it is now (frozen),
  following the source (appends added, truncation or rotation rebuilds it). It is a normal
  stream with its own search and filters; the gutter and go to line use the source's line
  numbers, and Show in context / `CTRL + K` shows the line in the source. It stops
  following when the source is closed or its spool (bounded by `stdin_spool_max_mb`) is
  full, and is not saved with the workspace, like standard input.
- **Scratchpad.** A plain-text tab per session (🗂 menu, palette) to collect lines and
  notes: "Send to scratchpad" in the row menu and in Find results, or `CTRL + SHIFT + N`,
  appends the selected rows under a `── app.log:1204 ──` reference line (or without it);
  `CTRL + click` / `ALT + Enter` on a reference shows the line in its stream, reopening
  the file if needed. Find box, Save as…, Clear; 4 MB cap; saved beside the session
  (`….scratch.txt`, `scratchpad.txt` for the default workspace) a second after each
  change and on exit. A dock layout saved while the Scratchpad tab is open is not read
  by older versions (they start with the default layout).

### Changed

- The "Last hour" shortcut of the time range popup is now "Last hour of the log" (same
  behaviour), next to the new relative shortcuts.

### Fixed

- **Crash on opening a file with every stream in floating windows** ("There did not exist a
  tree at surface index 0"). With all the tabs dragged out of the main window, the layout
  was saved with an empty main surface and the next open of any file panicked; such
  layouts now load, and the main surface is kept when saving.
- **`fasttail -V`, `--help` and `--print` from `cmd` or PowerShell (Windows)** no longer seem to
  wait for a key: the shell does not wait for the GUI program and had already printed its
  prompt above the output; FastTail now sends one Enter to the console when it is done, so a
  fresh prompt appears below.
- **The command palette no longer stays a couple of rows high.** After a search with few
  or no matches, clearing it left the list as short as it had become; it now grows back
  to the rows it shows (up to its usual height).

## [0.13.0] - 2026-09-29

### Added

- **Bookmark report and tags.** "Bookmark report…" in the stream menu (this stream) and
  "Bookmark report (all streams)…" in the 🗂 menu and the palette write a Markdown
  incident report of the bookmarks: summary, then each bookmark with line number,
  timestamp, note and tags and its line with 0 to 20 context lines in a fenced block
  (overlaps merged, fence longer than any backtick run), by stream or by time across
  streams, optionally with automatic bookmarks and a tag filter; copied to the clipboard
  or saved as `.md`, built in small steps with progress and Cancel. Every `#word` in a
  note is a tag: shown as chips in the tooltip, and `CTRL + G` with `#tag` jumps to the
  next bookmark carrying it. New `[general]` keys `report_context`, `report_auto`,
  `report_order`.

- **Search in a range.** The `⌖` chip beside a stream's search box limits hits, the
  counter (`[3 / 17 in range]`), `F3` / `SHIFT + F3`, the results pane and the timeline's
  search lane to part of the view, which stays visible: the selection, a typed line range
  (`1200-5000`, `1200-`, `-5000`) or a time span typed as in the time range popup. The row
  menu offers "Search in selection", "Search from here" and "Search up to here"; the
  overview strip shades a line scope; a reload resets it, with a note. A small line scope
  of a huge file is searched on the UI thread. The Find results tab gets optional from /
  to time fields applied to every stream, on each stream's own clock. Not saved.

- **Selection highlight.** Double-clicking a word of a row (letters, digits and
  `_ . : / @ - %`, trailing `.` / `:` dropped, 2 to 256 bytes: an id, an IP with its port,
  a UUID, a path) outlines every exact, case-sensitive occurrence of it in the rows of that
  stream, in the normal and wrap layouts, with a 1 px box that leaves rule colours, the
  search and the filters alone and does not use the 64-span budget. The row menu offers
  "Highlight "…"" on the word under the pointer; `Esc` on the rows, a double-click on
  empty space or on the same word, or a reload clears it. Not saved.

- **Next line of a rule (`F4` / `SHIFT + F4`).** The row menu's "Next line of rule" lists
  the enabled highlight rules matching the row (whatever their priority); picking one goes
  to its next shown line, and `F4` / `SHIFT + F4` then walk to the next / previous shown
  line that rule matches, from the selected row (or the top row), wrapping once with the
  search beep. Without a picked rule `F4` uses the first rule matching the selected row, or
  says that none does. The walk runs on the UI thread at most 4 ms per frame, showing
  `⏳ seeking rule "…"` while it takes longer, `Esc` stops it, and a whole walk without a
  match says so. The jump selects and centres the line, pauses follow and expands a
  collapsed group hiding it. The rule is forgotten when the rules change. In the command
  palette too.

- **Highlight rule sets.** "Export rules…" / "Import rules…" in the Highlights dialog write
  and read a `*.fasttail-rules.ini` file: `[fasttail_rules] version=1`, then
  `[highlight_N]` sections with the keys of `fasttail.ini` (the two share one reader and
  writer). External tool bindings are not exported. Import previews the rules, then
  **Append** (rules with the same pattern, regex and case options as an existing one are
  skipped and counted) or **Replace** (after a confirmation); a file without the header or
  with a newer version is refused and nothing changes.

- **Time display.** A `🌐` menu per text stream shows the leading timestamp of each row as
  written (default), in UTC, in local time (the offset in force at each instant, daylight
  saving included) or at a fixed offset from `-14:00` to `+14:00`, with a source zone
  (local, UTC or an offset) for timestamps that carry none; epoch seconds and milliseconds
  become dates (`1790604125123` → `2026-09-28 14:02:05.123Z`). The fraction keeps the
  digits printed (at most 3), `Z` or the offset is appended for UTC and fixed offsets, and
  the tooltip shows the original. Only the display changes: filters, search, rules, copy,
  export and external tools see the line as written, and the timestamp cache is not
  recomputed. The time span, the time range popup (shown and typed times), the timeline
  histogram, go to time and the collapse tooltip use the display zone. Saved per stream in
  the workspace and sessions as `time_display` / `time_source_zone` when not the default.
  In the command palette: show timestamps as written, in UTC or in local time. On Windows
  the local offset of an instant now follows that date's daylight saving rules.

- **Automatic token highlighting.** Settings → "Automatic token highlighting" (off by
  default, `[general] auto_highlight`) paints IPv4 / IPv6 addresses (with ports), UUIDs,
  URLs (`http`, `https`, `ftp`, `ws`, `wss`, `file`), durations (`250ms`, `1.5s`, `2m30s`)
  and file paths with a colour per kind from each theme (4.5:1 contrast or better, checked
  by a test), each kind switchable (`auto_highlight_kinds`). A hand-written single-pass
  scanner (no regex, under 2 µs for a 200-byte row in release); the tokens rank after
  rules, quick labels and ANSI colours, above the level colouring, within the 64-span
  budget. `--print --color` applies them too. Also a palette toggle. Four-part versions
  ending in `.0.0` (`Chrome 118.0.0.0`) and hex words around `::` (`dead::beef`) are left
  as text; long runs of hex digits and colons are skipped in linear time.

- **7z archives.** A `.7z` (recognised by its signature, whatever its name) opens like a
  zip: one file opens directly, several show the entry picker, and each chosen entry opens
  as its own stream titled `logs.7z › server.log`. Copy, LZMA, LZMA2, BZip2, Deflate and
  PPMd entries are read, with the BCJ / BCJ2 / ARM (and other branch) and delta filters.
  Solid archives work: the block is decoded from its start, the entries before the chosen
  one are dropped without being written, and the progress runs over the block (the
  picker shows its size when hovering the entry). Encrypted entries, other coders
  (zstd, brotli, lz4...), unsafe or duplicate names and a dictionary above 256 MiB are
  listed disabled with the reason; a 7z whose header is encrypted, larger than 64 MB or
  declaring more than 250,000 files, blocks or streams is refused with a message, and no
  password is asked; a 7z whose start header fails its CRC is reported as damaged. The
  picker lists at most 100,000
  entries and says when the list is partial. Space guard, output cap, re-extract,
  nested `.gz` entries, workspace, sessions (`entry=`), recent files and bookmarks work
  as for zip entries. Pure-Rust decoder (`sevenz-rust2`); all 16 languages updated.

- **Print mode.** `fasttail --print [OPTIONS] PATH...` writes the lines that pass the
  filters to standard output and exits, without a window and without touching the
  workspace or `fasttail.ini`. The matching is the window's: `--filter` and `--exclude`
  (each up to 8 times), `--regex`, `--case-sensitive`, `--level`, `--since` / `--until`
  (everything the time range popup reads, plus `now`, `-15m`, `-1h30m`, `-2d`, `-1w`), with stack-trace
  lines following their entry; `--context N` adds lines around each match with `--`
  between groups, `--line-numbers` and the `file:` prefix (several inputs, `--no-prefix`
  to drop it) label them, `--color auto|always|never` paints the theme's level colours,
  the highlight rules and the log's own ANSI colours. Inputs are files, patterns,
  single compressed files and standard input, read as streams in constant memory;
  `--follow` keeps printing appended lines, handling truncation and newer pattern
  files. Exit codes 0 (printed), 1 (no match), 2 (usage), 3 (unreadable input). On
  Windows, redirected output is used as it is and the parent console is attached only
  when there is none. Command-line output stays in English, like `--help`.

- **`--since` / `--until` in the window.** Without `--print` they set the time range of
  the streams opened from the command line (a relative time is fixed at start, to the
  millisecond); repeated `--filter` / `--exclude` still mean the last one.

- **Context lines around filter matches.** A `± N` drag value in the stream bar (0 to 100,
  per stream, next to the collapse selector) shows every line the filters keep together
  with the `N` file lines before and after it, like `grep -C N`. Context lines ignore every
  filter (stream terms, level, time range, global filter) and are drawn dimmed; a thin rule
  marks where lines are hidden between two groups, with the count in its tooltip. Changing
  `N` never refilters: merged line ranges are derived from the matches in one pass
  (24 bytes per group), and on files above 16 MB the context appears with the matches as
  the background scan finds them. Search, `F3`, bookmarks, go to line, selection, copy,
  export, the Δt column, the overview strip and collapse work on the rows shown, context
  rows included; the Find results tab keeps listing matches only. Saved per stream in the
  workspace and in sessions as `context_lines=N` (only when above 0). Help dialog and all
  16 languages updated. `fasttail --print --context N` gives the same
  context in print mode.

- **Command palette.** `CTRL + SHIFT + P`, or `⌨` in the title bar, opens a box over the
  window listing every command: the window, stream, search, bookmark, session and view
  actions (follow, wrap, collapse, go to line, export, next match, show in context, open
  file, sessions...), the items of the stream menus and of the row menu (bookmark note,
  copy as shown, time anchor…), and every on / off or
  multiple-choice setting of the Settings page ("Toggle …" with its state; theme,
  language, renderer and size unit open a second step listing the values, the current one
  marked). Typing filters with a fuzzy, case- and accent-insensitive match on the name in
  the interface language and in English; each row shows its category and shortcut. `↑` /
  `↓` / `PgUp` / `PgDown` move, `Enter` runs, `Esc` closes, and while it is open no key
  reaches the streams. Stream commands act on the stream focused when the palette opened
  and run the same code as their button, menu item or key; one that cannot run now (no
  stream, no search, HEX view...) is greyed with the reason. The last 8 commands come first
  and are saved as `[general] palette_recent` in `fasttail.ini`. The actions live in a new
  registry (`src/actions.rs`) that later changes extend. Help dialog and all 16 languages
  updated.

### Changed

- **Development.** Windows builds link with `rust-lld` and development builds keep line
  tables only (an incremental test build went from about two minutes to ten seconds); CI
  fails on clippy warnings; every Rust file carries the project, copyright and SPDX
  header.
- **Building from source needs Rust 1.93.** `sevenz-rust2` 0.23, used to read `.7z`
  logs, requires it; the release binaries are unaffected.
- `--filter` and `--exclude` can be repeated with `--print` (the window keeps using the last one, as before);
  `--level`, `--context`, `--color`, `--line-numbers`, `--no-prefix`, `--regex` and
  `--case-sensitive` without `--print` are usage errors. `--help`, `--version` and usage
  errors now go to a redirected standard output or error as they are.

### Fixed

- **Two windows no longer overwrite each other's settings.** With two FastTail windows on
  the same `fasttail.ini`, the idle one rewrote the file every couple of seconds with the
  state it had loaded, undoing what the other had just saved (a new rule, a theme, a
  preset). Each window now writes the file only when its own settings or workspace
  change.

## [0.12.0] - 2026-09-28

### Added

- **Global filter.** `CTRL + SHIFT + H`, or `🌐` in the title bar, shows a bar with up to 8
  include and 8 exclude terms (and their own `Aa` / `.*` toggles) that every stream, open
  now or later, applies on top of its own filters: a line is shown when it passes the
  stream's terms, level and time range and contains every global include term and no
  global exclude term; a stack-trace line follows its entry unless an exclude term
  matches it. Each stream bar shows `🌐` with the global terms in its tooltip, and the
  Filters window lists them. Edits reach the streams 300 ms after the last key; large
  files refilter in the background; the Find results tab honours it. An **On** switch
  suspends it without losing the terms, and hiding the bar keeps it on. Saved in
  `fasttail.ini` as `[global_filter]`; not in sessions or presets. Help dialog and all 16
  languages updated.

- **Show in context.** With a filter active, the row menu item **Show in context** or
  `CTRL + K` shows the selected line in the full, unfiltered log (centred, marked `◆`,
  follow paused) under a banner; **Back to filtered view**, `Esc` or `CTRL + K` returns to
  the filtered view with the same top row, selection and follow state, without
  recomputing anything. The filter fields are dimmed meanwhile, and editing one applies the
  new filter. The Find results tab offers it too, also for lines the stream's filter hides.
  Help dialog and all 16 languages updated.

- **Bookmark notes.** Right-click a row and pick **Bookmark note…** to attach a one-line
  note (at most 200 characters) in a field that opens in the stream bar: the row is
  bookmarked, its marker becomes `✏`, and the note shows when hovering the marker or the
  bookmark's mark in the overview strip. An empty note keeps the bookmark; **Remove
  bookmark** drops both. Notes are saved with the bookmarks, per file in `fasttail.ini`
  (`note_<i>_<line>` beside `lines_<i>`) and in sessions (`bookmark_note.<line>`). Older
  versions still read the bookmarks, and drop the notes when they save the settings.

- **Rules that bookmark matching lines.** A highlight rule's new **Bookmark matching
  lines** option bookmarks every line it matches, whatever the filters and the colours a
  higher rule paints: the lines in the file and every line appended later. These automatic
  bookmarks show `☆`, `F2` / `SHIFT + F2` visit them, the overview strip draws them dimmer,
  and `CTRL + F2` dismisses one until the next reload or rules change. They are recomputed
  from the rules, never saved, and capped per stream by the new `auto_bookmark_max`
  setting (Settings → Performance & refresh, default 10,000, 100 to 100,000; the stream
  bar says when it is reached). Files above 16 MB are matched in the background with
  progress in the stream bar. Saved as `bookmark=true` in the rule's section. All 16
  languages updated.

- **bzip2, xz, zstd and tar archives.** Before, only gzip and zip opened: a `.tar.gz` was
  stopped with "tar archives are not supported", a bare `.tar` opened as one binary blob
  and `.bz2` / `.xz` / `.zst` files opened as they were, in HEX view. Now bzip2, xz and
  zstd files are recognised by their content and decompressed in the background exactly
  like gzip (progress, cancel, re-extract, output cap, free-space guard; concatenated
  streams and frames included), and a tar, plain or inside any of the four codecs
  (`.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`), opens the entry picker at once: it fills in
  while a background scan reads the headers (`scanning N%`, ✖ to stop), entries open
  before the scan ends, and a tarball holding a single file opens it directly. Tar links,
  devices, FIFOs, sparse files and unsafe names are listed disabled with the reason; an
  entry that is itself compressed (`bundle.tgz › logs/app.log.1.gz`) is decompressed once
  more; sessions keep `entry=` for tar entries too. xz and zstd windows over 256 MiB are
  refused. Pure-Rust decoders only (`tar`, `bzip2` on `libbz2-rs-sys`, `lzma-rust2`,
  `ruzstd`). All 16 languages updated.

- **Collapse repeated lines.** The `× Collapse` selector in the stream bar, or
  `CTRL + SHIFT + D`, shows runs of consecutive equal entries (a line plus its stack-trace
  frames) as one row with a `×N` badge: **exact** compares the text after the leading
  timestamp, **numbers** also masks numbers, `0x` values, hex ids and UUIDs. Groups are
  formed over the lines the filters leave visible; a click on the badge expands or
  collapses a group, and its tooltip gives the line range and time span. Copy and export
  still write every line, and the row menu gains **Copy** and **Copy as shown**
  (`retry 1 ×3`); search counts every hit, `F3` steps over a group once, and going to a
  line, a bookmark or a hit of the results pane expands the group that hides it. The time
  delta after a group starts from its last line, the overview strip follows the collapsed
  rows, and Show in context stays the full, uncollapsed log. A repetition appended while
  following raises the last count instead of adding a row; files above 16 MB are grouped in
  the background with progress in the stream bar. Only the groups are kept (24 bytes each).
  Saved per stream in the workspace and in sessions as `collapse=exact|numbers`. Help
  dialog and all 16 languages updated.

### Changed

- **Line numbers and the time delta column are set per stream; the ini values are the
  defaults for new streams.** The `# 123` and `Δt` buttons of a stream bar now change
  that stream only, so `Δt` can be on in a timed service log and off in the access log
  beside it. A new stream (file, pattern, standard input, compressed file, archive entry)
  starts from `show_line_numbers` and `show_time_delta` in `[general]`, which Settings
  now labels "… in new streams"; changing them leaves open streams alone. Each stream's
  switches are saved with it in the workspace and in session files as `line_numbers=` /
  `time_delta=`, always written, so a saved stream does not follow a later change of the
  defaults; files from older versions, without the keys, take the defaults. The Δt gap threshold stays one setting. All 16 languages updated.

- **The time range moved to the stream bar's time span.** `🕘 2026-09-18 14:02:05 →
  16:30:12`, the span of the visible lines (the date written once when both ends share
  it), is now the time range control: it turns to the accent colour while a window
  narrows the view, to the warning colour while a side cannot be read, ends with `⏳`
  while the window waits for the timing, and reads `🕘 no timestamps` on a stream without
  usable ones. A click opens a popup with, for each side, a field wide enough for a full
  timestamp (every format accepted before still works), a calendar of the month (Monday
  first; the days the log spans tinted, today outlined, arrows for month and year) and
  hour / minute / second spinners, plus the shortcuts **Whole log**, **First day**,
  **Last day** and **Last hour**. Edits are a draft: **OK** or `Enter` applies the window
  and closes the popup; **Cancel**, `Esc` or a click outside leave it as it was, and OK is
  disabled while a side cannot be read. Picking a day keeps the side's time, or gives the
  whole day. The inline time fields and their `✖` left the filter row, where `📊` and
  `🔍` stay. Windows are saved per stream and in sessions as before; nothing new in
  `fasttail.ini`. All 16 languages updated.

- Faster ANSI handling and one-byte searches: CSI sequences are parsed in one pass and
  a one-byte query uses `memchr`.

### Fixed

- **`Space` no longer toggles Follow while you type.** A space typed in a filter, search or
  settings field toggled Follow on every open stream; `Space` now acts only on the focused
  stream, and only when no text field has the keyboard.
- The **Max UI FPS (GPU)** setting is now applied: frames are capped at it on a GPU as the
  software setting already did on a CPU renderer.
- The window and taskbar now show the FastTail icon instead of the platform default.
- The **Auto** renderer label said "OpenGL, then wgpu"; it tries wgpu first, and says so.
- The toolbar's **Play**, **Pause**, **Help** and **About** are translated, and the Help
  dialog lists `CTRL + L` (PIN lock) and `ALT + 1..9` (switch tab).
- `Esc` closes only the dialog on top (Settings, Filters, About or Help) instead of all of
  them at once.
- A failed export or session save is shown (in the stream bar, or in a notice window)
  instead of going only to a console the GUI build does not have.
- Closing the borderless window with its `✕` deletes this run's decompression spools, as
  closing it any other way already did.
- The rule and preset reorder buttons (⬆ / ⬇) say in their tooltip why they are disabled
  at the ends of the list.

## [0.11.0] - 2026-09-27

### Added

- **Time delta column.** The `Δt` button next to the line-number toggle shows, for each
  row with a timestamp of its own, the time since the previous visible row (`+0.125`,
  `+4:05.120`, `+2:03:04`, `+3d 04:05`), so it follows the filters; stack trace frames
  stay blank and deltas of at least `time_delta_gap_ms` (default 1000, 0 = off, in
  Settings) are drawn in the accent colour. The switch is global (`show_time_delta` in
  `fasttail.ini`). "Set time anchor here" in the row context menu measures every row
  from one line instead, signed (`-0.500` above it), until "Clear time anchor"; the
  anchor survives filter changes, is dropped on truncation or rewrite and is not saved.
  A selection of two or more rows shows its elapsed time in the stream status bar
  (`Δ +2.357 · 14 rows`). Times are those of the timestamp cache, on the clock the log
  printed; turning the column on never blocks (a file above 16 MB is timed in the
  background and its rows show `…` until then), and a stream without usable timestamps
  keeps the column hidden with a tooltip saying why.
- **Search all streams.** `Ctrl+Shift+F`, or the `🔎` button next to a stream's search
  box, opens a Find results tab (split below the streams) with the focused stream's query;
  `Enter` or Find runs it over every open stream with the per-stream search's
  case-insensitive match, over the lines each stream shows under its own include/exclude,
  level and time filters, with ANSI codes stripped where the stream strips them. Streams
  in HEX view are skipped and listed as skipped. Each stream is searched by a background
  job of its own, never cancelling or cancelled by the stream's own index, filter, search,
  level or timestamp scans; at most four run at once (fewer on machines with fewer cores),
  the others wait in the order of the streams, and a stream still being indexed waits its
  turn. Up to 100,000 hits are listed per stream and the true total is still counted. The
  results are grouped by stream in one virtualized list: a header per stream with its
  match count, progress and notes (queued, stopped, capped, skipped, stale), collapsible
  with a click or `Enter`, then its lines with line number, level colour and the query
  tinted. A click or `Enter` on a result brings that stream's tab to the front, centres
  the line (or the next visible one when the stream's filters now hide it), selects it
  and pauses follow; the stream's own search query and matches are untouched. The list
  keeps the keyboard: the arrows, `Page Up` / `Page Down` and `Home` / `End` walk the
  results and the stream follows each one. Results are a
  snapshot (its age is shown): Refresh runs the query again, Stop keeps what was found,
  a new query, closing the tab or closing a stream cancels its jobs, and a stream reloaded
  since (truncated, rotated, rewritten, switched to another file) is marked stale, its rows
  dimmed and no longer jumping. The tab is not saved in the dock layout. Help dialog and
  all 16 languages updated.
- **Several include and exclude terms per stream, and filter presets.** The `+` beside
  the stream bar's include or exclude field adds a term row, edited in the Filters
  window under "All of" / "None of": a line is visible when it contains every include
  term and none of the exclude terms (up to 8 per side), then the minimum level and the
  time range apply as before and stack-trace lines still follow their entry. Terms are
  matched as typed with the stream's case and regex toggles — no operator syntax; OR
  inside a term stays the regex `a|b` — and a regex term that does not compile is
  flagged in its row. The bar keeps the first term of each side with a `+N` badge.
  Background filter and search scans on large files, the ANSI strip/render modes and the
  time window all see the combined filter. Terms are saved per stream in the workspace
  and in sessions as `include.2`, `include.3`… next to the existing keys, so older builds
  still open the file with the first term. A `Presets ▾` drop-down saves the stream's
  terms, toggles, minimum level, unknown-level toggle and optionally the time range as
  typed (a bare `14:02` follows the day of the log it is applied to) under a name,
  applies it to the stream or to all open streams in one recomputation, shows the
  matching preset's name (`name *` once edited, with "Update from this stream"), and the
  Filters window renames, reorders and deletes presets. Presets live in `fasttail.ini`
  as `[filter_preset.N]` sections, not in sessions. `--filter` / `--exclude` set the
  first term as before.
- **Standard input as a stream.** `command | fasttail -` copies the command's output,
  64 KB at a time and flushed as it arrives, into a spool file tailed like any followed
  log, so every feature works on it; the tab is titled `stdin` and the footer names the
  spool. Piped or redirected input is also detected without `-`, and its tab appears
  only when the first byte arrives. When the input ends the stream stays open with
  `input ended · N lines`. The spool restarts from empty at `stdin_spool_max_mb`
  (default 2048, in Settings) or when its volume keeps less than 512 MB free, and the
  stream bar says earlier input was discarded. The stream is never saved in the
  workspace, recent files or sessions, and its spool is deleted with the tab, at exit or
  by the next start after a crash. A second `-` is a usage error (exit 2), `-` with
  nothing piped is reported on stderr, and a file named `-` opens as `./-`. Verified on
  Windows from cmd.exe, Git Bash, PowerShell 7 and Windows PowerShell 5.1 (see the
  README for the caveats of each shell).
- **Timeline histogram.** `📊` next to the time range fields opens a strip above the rows
  with the stream's lines per level over time: bars stacked ERROR/FATAL, WARN, INFO,
  DEBUG/TRACE and no level in the theme's level colours, counting every timed line
  whatever the filters, so a burst of errors or a gap in logging stands out. A click on a
  bar sets the time range to its span, a drag to the span between two bars; the bounds are
  written into the from/to fields as whole seconds and applied like a typed window, which
  is shaded on the strip. Hovering a bar shows its span and counts per level; an optional
  lane (`🔍`) marks where the current search hits fall. Opening it times the stream (in the
  background above 16 MB, the bars growing as it goes); a log without usable timestamps
  gets the fields' hint. The histogram is kept incrementally from the timestamp and level
  caches in at most 2,048 buckets, one second wide and doubling as the span grows, and
  follows appends, truncation and rewrites. The histogram is opened per stream and saved
  with it in the workspace and in sessions (`timeline=true`); the search lane is a global
  preference (`timeline_search_lane` in `fasttail.ini`).

### Changed

- **Disabled buttons explain themselves.** Hovering a greyed-out button now shows its
  tooltip: the pattern prompt's Go (the pattern is invalid), the zip picker's Open
  (nothing selected), the ⬆ / ⬇ of highlight rules and filter presets at the ends of
  their lists, the preset rename ✔, and Stop / Refresh in the Find results tab.
- **Shortcuts in the help read `CTRL`, `SHIFT` and `ALT`** in capitals throughout; the
  translated descriptions spell them in capitals too (`STRG`, `UMSCHALT` in German).
- **Less allocation while painting a row.** Highlight spans and ANSI colour parameters
  are worked out in small stack buffers instead of a heap allocation per row or per
  escape sequence.

### Fixed

- **Filters and searches keep their quotes and edge spaces in session files.** A term
  starting with a quote (`"status":500`, `'user'`) came back from a session or the
  workspace without its quotes, and one with leading or trailing spaces (` ERROR `)
  came back trimmed, so the restored stream filtered on different text. Such values are
  now written quoted and read back exactly as typed; filter presets are stored the same
  way.
- **The spool folder is kept private on Linux and macOS.** An existing
  `fasttail-spool` directory is set back to owner-only access (0700) before a
  decompressed stream or standard input is spooled into it, and one owned by another
  user is refused instead of used, so its copies of your logs are not readable by
  other local users.

## [0.10.1] - 2026-09-26

### Changed

- **Release file names carry the version, and every archive holds `LICENSE` and
  `README.md`.** `fasttail-windows-x86_64.zip` becomes `fasttail-windows-x86_64-0.10.1.zip`
  (likewise the Linux and macOS archives and `fasttail-windows-x86_64-symbols-0.10.1.zip`),
  so downloads of different versions no longer overwrite each other. A tag whose version
  differs from `Cargo.toml` now fails the release build.

### Fixed

- **Closing a stream no longer risks freezing another file operation on Windows.** When
  a watched file changed at the moment its stream was closed, the file watcher (notify
  8.2) could re-arm its read on the directory handle it had just closed; Windows could
  already have handed that handle value to another open, which then blocked for good.
  The test suite hung this way now and then on Windows. notify is updated to 9.0.0-rc.5,
  which fixes it, and the CI test run (built in a step of its own) now fails after 5
  minutes instead of hanging.
- **The time range can always be corrected or cleared.** Typing in a "from" or "to"
  field times the log; when fewer than half of its lines turned out to carry a timestamp,
  both fields were disabled with what had been typed still in them and the "invalid
  time" warning stuck, and the clear button only appeared for a window that applied, so
  there was no way out. A field holding text now stays editable, the clear button shows
  whenever either field holds text, and the warning waits until the field is left
  instead of flagging the first keystroke.
- **Zip entries written with `\` separators are kept in sessions.** An entry such as
  `dir\file.log` (written by some Windows tools) was saved with that spelling, so the
  session and the workspace named the wrong archive and reported the stream as missing
  on the next start. The stream now saves the `/`-separated name (`dir/file.log`) and
  reads the entry as the archive spells it; sessions saved by 0.10.0 still resolve.
- **An entry named `./x.log` opens.** Its stream path drops the `.`, and the lookup no
  longer compared the two spellings, so the open failed with "no longer holds this
  entry". Entries of one archive that would share a tab (`a/b.log` and `a\b.log`, or
  names differing only by case on Windows) now open the first of them; the others are
  shown disabled in the entry picker as having the same name, instead of focusing the
  tab of another entry.
- **Bookmarks of a large compressed stream are restored.** When the line index ran as a
  background job, the stream was considered complete while the index was still partial:
  the saved bookmarks were dropped and the encoding detection settled early. Both now
  wait until the index covers everything decompressed.
- **ANSI colours are detected anywhere in an append.** Auto mode looked only at the first
  64 KB of each append, so a colour code further into a large append was never seen, and
  a compressed stream, which arrives in chunks of a megabyte or more, could stay in raw
  mode for good. Until the first colour code is found, every appended byte is now
  examined (a fast scan for the escape byte, read in chunks straight from the file); at
  open, only the first 64 KB of the file are sampled, as before.
- **A date alone is a valid time bound.** `2026-09-18` in the "from" field starts at
  00:00:00 of that day and in the "to" field covers it through 23:59:59.999; it was
  flagged as an invalid time.
- **The time fields stay usable after clearing on logs full of stack traces.** A log was
  judged untimeable when fewer than half of its lines carried a timestamp of their own, so
  on a Java log where stack traces outnumber the entries the fields were disabled as soon
  as the ✖ cleared them. The rule now counts the lines a window can place in time (trace
  lines inherit the timestamp of their entry), and the fields are never disabled: on a log
  that really has no timestamps a hint says so.
- **The external tools cookbook link is translated in every language.** Its label and
  tooltip were shown in English in German, Portuguese, Russian, Ukrainian, Japanese,
  Korean, Turkish, Polish, Dutch, Traditional Chinese and Friulian. The translation test
  now checks every key in every language, so a single string falling back to English is
  caught.
- **Only regular files are opened as streams.** Opening a directory or a device path
  now fails with "Target path is not a regular file" instead of being read as a stream.

## [0.10.0] - 2026-09-25

### Added

- **ANSI colour codes are rendered.** A stream whose first 64 KB, or the first 64 KB of a
  later append, holds an SGR escape sequence (`ESC[31m`) switches from auto to render: the sequences
  are hidden and the 16 base and bright colours (from a palette per theme, readable on
  Light), 256-colour and 24-bit colours, bold, dim, italic, underline and inverse are
  painted as spans of the row. User highlight rules and quick labels win over the ANSI
  colours, which win over the level colouring, within the 64-span budget of a row. An
  `ANSI: auto → render` selector in the stream bar also offers strip (sequences hidden,
  no colour) and raw (the line as stored, `ESC` drawn as `␛`); a chosen mode is saved per
  file in the workspace and in sessions (`ansi=`). In render and strip modes every text
  feature sees the line without its sequences — include/exclude filters (background
  scans included), search, highlight rules and quick labels, level, timestamp and JSON
  detection, copy, export and the `{line}` placeholder of external tools — so
  `ESC[31mERROR` is detected as ERROR and matched by `\bERROR\b`; the text search cursor
  is converted back to the file bytes when switching to HEX, which always shows the file.
  Other sequences (cursor movement, OSC 8 links, titles) are removed without being
  interpreted. A log without an escape byte stays in auto (acting as raw) and is read as
  before, with no extra work per line; the parser is in-house (no new dependency).

- **Compressed logs open directly.** A gzip file (`app.log.1.gz`, multi-member included)
  or a zip archive is recognised by its first bytes, whatever its extension, and
  decompressed on a background thread into a temporary spool file that the normal engine
  reads, so filters, search, levels, time range, bookmarks, HEX and export all work and
  nothing decompressed is held in memory. The first lines appear while the rest is still
  inflating; the stream bar shows `decompressing N%` with a cancel button, then why the
  content is partial if it stopped early, and a button to extract it again. A zip with
  several files opens an entry picker (filter, sort by name or size, multi-select) and
  each entry becomes its own stream, titled `bundle.zip › server.log`. Follow is locked
  off for these streams: the archive is a snapshot and is not watched. Encrypted entries,
  zip methods other than stored/deflate, entries whose name is absolute or climbs out of
  the archive (`../`), and `.tar.gz` are refused with the reason; `--follow` never turns
  follow on for them. A
  zip entry must fit on the spool volume with 512 MB to spare, the free space is checked
  again every 64 MB, and `compressed_max_gb` (default 20 GB) caps one extraction. Spools
  go to `fasttail-spool` in the temporary folder, or under `spool_dir`, are deleted when
  their tab closes and at exit, and the ones left by a crash are swept at the next start.
  The workspace, sessions (`entry=` next to the archive path), recent files and bookmarks
  name the archive, never the spool. `flate2` (pure-Rust miniz_oxide backend) and `zip`
  (read-only, deflate through the same flate2) add about 323 KB (+1.7%) to the Windows release
  executable (19,526,656 → 19,857,408 bytes, thin LTO).
- **Search results pane.** The `☰` button in the stream bar opens a resizable pane under
  the rows of a stream with an active query, listing only the matching lines with their
  line number and the query tinted. It is virtualized over the hit list, so only the rows
  on screen are read, and hits found by a running search or on appended lines appear as
  they are found. A click, or `Enter` on the keyboard selection, makes a hit the current
  match, centres it in the main view and pauses follow; the current match is marked `▶`
  and the pane follows `F3` / `Shift+F3`. With the pane focused the arrows, `PgUp` /
  `PgDown` and `Ctrl+Home` / `Ctrl+End` move its selection without moving the main view,
  `Esc` hands the keyboard back, and the stream shortcuts (`F3`, `Ctrl+F`, `Ctrl+G`,
  bookmarks, `Ctrl+C`) keep working. Open state and height are one preference for every
  stream, saved in `fasttail.ini` (`search_pane`, `search_pane_height`); in HEX view the
  pane shows a notice.
- **Overview strip.** A narrow column beside the main view's scroll bar marks the search
  hits, the bookmarks, the ERROR/FATAL lines and the current match at their position among
  the visible rows, with the viewport drawn as a box; click or drag it to scroll there,
  hover it for the line number. Hit marks (for the listed hits) and bookmark marks are
  exact; error marks follow the level scan and are exact
  without a filter (from per-4096-line error counts kept beside the level cache) and on
  filtered views up to 4 million rows, sampled above (the tooltip says so). The marks are
  cached and recomputed only when their inputs change, at most four times a second while
  the stream grows. Settings checkbox `overview_strip`, on by default.

### Changed

- **Search keeps up to 1,000,000 hits and counts the rest.** The stored match list per
  stream grows from 20,000 to 1,000,000 line hits (8 MB at most); past the cap the search,
  synchronous or on the worker, and the refresh on appended lines keep counting without
  storing, and the counter reads the true total with a "first 1,000,000 listed" note.
  `F3` wraps within the stored hits; the export of matches writes the stored ones. HEX
  byte hits keep their 20,000 cap. Visible-row lookups on a filtered view are now a
  binary search instead of a linear scan.

- **Building from source needs Rust 1.88.** `zip` 8, used to read `.zip` logs, requires it;
  the release binaries are unaffected.

### Fixed

- **A log opened while empty detects its encoding once it has content.** The encoding
  and binary check ran only on the bytes present when the file was opened, so a log
  created empty and filled later as UTF-16 was read as UTF-8; it now runs again when the
  file first holds 512 bytes.
- **The background search checks the hit cap on its running total.** The worker compared
  the cap with the current batch of hits, which is emptied every 4,096 hits, so on
  files above 16 MB it never saw the cap and sent every hit to the end of the file for
  the engine to discard. It now lists hits up to the cap and only counts the rest.

## [0.9.1] - 2026-09-25

### Fixed

- **The About dialog and the status bar are translated everywhere.** "Website" in About
  and "No file open" in the footer were English in every language; both now follow the
  interface language like the rest of the window.
- **The first use of the time controls no longer blocks the window.** Closes the 0.9.0
  known issue: when more than 16 MB of a stream remain to be timed, the timestamps are
  read by a background scan with `timing lines 37%` in the stream bar, like indexing,
  filtering and search. A time window typed meanwhile is held — nothing is hidden, a hint
  next to the fields says it applies when timing finishes — and applies by itself at
  100%; a time entered in `Ctrl+G` waits in the popup with the progress and jumps when the
  scan completes, and `Esc` drops the jump without stopping the scan. The scan gives way
  to a filter or search typed without a window and resumes where it stopped, and the
  cache it builds is identical to the synchronous one. Small files and appended lines are
  still timed at once.
- **Filter and search scans honour the time window on large files.** With a window set,
  filtering a file above 16 MB no longer falls back to the interface thread, and search
  hits outside the window are no longer listed: the background scans apply the window to
  the lines they return.
- **Lines appended under a time window are timed before they are filtered**, so a new
  line inside the window shows up at once instead of after the next full refresh.
- **Background scans of a pattern stream read the file it follows.** On a `*`/`?`
  stream above 16 MB the worker opened the pattern instead of the matched file, so a
  filter or search there never produced results.

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

[0.16.0]: https://github.com/matteobaccan/FastTail/compare/v0.15.0...v0.16.0
[0.15.0]: https://github.com/matteobaccan/FastTail/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/matteobaccan/FastTail/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/matteobaccan/FastTail/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/matteobaccan/FastTail/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/matteobaccan/FastTail/compare/v0.10.1...v0.11.0
[0.10.1]: https://github.com/matteobaccan/FastTail/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/matteobaccan/FastTail/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/matteobaccan/FastTail/compare/v0.9.0...v0.9.1
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
