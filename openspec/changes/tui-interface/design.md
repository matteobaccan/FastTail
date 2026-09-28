## Context

The prototype (`feat/tui-prototype`, draft PR #131) is a `fasttail-tui` binary behind a
`tui` Cargo feature, off by default, built on ratatui 0.30 and crossterm 0.29, with
arboard for the clipboard. It opens one `TailEngine` per path, draws bordered windows,
dialogs with `[ OK ]` / `[ Cancel ]`, a two-way split, and handles search, include /
exclude filters, minimum level, collapse, compressed files and archive entries given as
`archive.zip/entry.log`, mouse with a pure hit map, `--ascii` and `--no-mouse`. Its
measurements (`docs/tui-feasibility.md`): about 1 ms per full redraw on a 10-million-line
file, 0.26 % of one core idle, 97 MB working set for a 1 GB file (the engine's index),
`fasttail-tui.exe` 3.5 MB against 21.7 MB for `fasttail.exe`. A workspace restore from
`fasttail.ini` is being added to the prototype in parallel and is assumed as input here.

What still binds every build to the GUI stack, from the feasibility doc:
`HighlightStyle { fg, bg: egui::Color32 }` and `CyberTheme`'s colour functions,
`TailEngine::markdown_cache: egui_commonmark::CommonMarkCache`,
`external_tools::Shortcut { key: egui::Key }` and `modifiers() -> egui::Modifiers`, the
monolithic `lib.rs` that compiles `ui` and `renderer` (eframe, wgpu, winit, egui_dock,
rfd, egui_commonmark) for every binary, and the "open anything" logic
(`open_log_file`, `open_engine_for`, `open_archive` in `ui/app.rs`, the go-to parser in
`ui/dock.rs`, the time-range text parser in `ui/time_range.rs`) living in GUI methods.

`fasttail.exe` is built with `#![windows_subsystem = "windows"]`. FastTail had a
terminal front end until 0.7.1; `--gui` has been accepted and ignored since then.

## Goals / Non-Goals

**Goals:**
- A terminal interface good enough for daily log reading over SSH or in a console:
  follow, search, filters, collapse, bookmarks with notes, show in context, go to line,
  HEX view, compressed files, ANSI colours, the user's theme, rules and language.
- One engine, one configuration, one workspace format for both front ends, with no
  engine logic duplicated in the terminal code.
- A build of the terminal interface that does not compile eframe, egui, wgpu or winit.
- The GUI and its release artifacts unchanged apart from the new Settings entry, the
  `--tui` / `--gui` options and the size budget below.

**Non-Goals:**
- Parity with every GUI feature (see the scope table and the proposal's non-goals).
- Writing `fasttail.ini` from the terminal.

## Decisions

### 1. Crate layout: one crate with `gui` and `tui` features, both on by default

`Cargo.toml` gets `default = ["gui", "tui"]`. `gui` enables the optional `eframe`,
`egui`, `egui_dock`, `egui_commonmark` and `rfd` and compiles `ui` and `renderer`;
`tui` enables `ratatui`, `crossterm` and `arboard` and compiles a new library module
`tui` (the prototype's `app`, `view`, `keys`, `mouse`, `colors`, `clipboard`, moved from
`src/bin/fasttail-tui/`). Everything else (engine, config, session, compressed,
filters, i18n, the new `workspace` and `color` modules) compiles with neither.
Binaries: `fasttail` (`src/main.rs`, `required-features = ["gui"]`) and `fasttail-tui`
(`src/bin/fasttail-tui.rs`, a thin `main` calling `fasttail::tui::run`,
`required-features = ["tui"]`). `cargo install fasttail` keeps working and gets both.

*Rejected:* a Cargo workspace with a `fasttail-core` crate. It is the cleaner end state,
but it moves some 30 modules and their tests (history and blame churn), needs a second
crate published to crates.io before `fasttail` can be, and gives the same compile-time
separation as a feature for this project's single maintainer. It stays possible later:
the feature boundary is the crate boundary.
*Rejected:* keeping `tui` off by default. The Linux and macOS `fasttail` binary must
contain the terminal interface (maintainer decision 3), and a default build that tests
only half the code would let the terminal side rot, as it did before 0.7.1.

### 2. Engine decoupling (first task group)

- `src/color.rs`: `pub struct Rgba { r, g, b, a: u8 }` with constants and
  `from_rgb`. `HighlightStyle`, `CyberTheme::level_style`, the accent / border / dim
  colours and every engine-facing colour use it; `impl From<Rgba> for egui::Color32`
  lives under `gui`. `CyberTheme::apply(ctx)` (egui `Visuals`) moves to `ui`.
- `markdown_cache` moves from `TailEngine` to the GUI's per-tab state in `ui/dock.rs`,
  keyed by stream id; dropped with the tab.
- `external_tools::Shortcut` stores `key: KeyName` (an enum of the keys the parser
  accepts today: letters, digits, F1 to F12, Space, Enter, arrows...) and a `Mods`
  bit set; the GUI maps them to `egui::Key` / `egui::Modifiers` at the edge. The
  `tool.N` ini format does not change.
- `src/workspace.rs`: `open_target(path) -> OpenOutcome` (plain file, pattern,
  compressed file, archive with one entry, archive needing an entry choice with the
  entry list, standard input), `restore(config) -> Vec<StreamSpec>`, the go-to parser
  (`N`, `+N`, `-N`, a time) and the time-range text parser, all UI-agnostic. The GUI's
  `open_log_file`, `open_engine_for`, `open_archive`, go-to box and time-range fields
  call them; the terminal interface calls the same functions.
- The search wrap-around beep becomes a flag the engine returns (`wrapped: bool`); the
  GUI plays it under `audio`, the terminal interface rings the terminal bell only when
  `sound_enabled` is true in `fasttail.ini`.
- A `test` job step `cargo check --no-default-features --features tui` proves the
  boundary (see the release-pipeline delta).

*Rejected:* mapping `Color32` at the terminal edge as the prototype does. It works, but
it keeps egui in the terminal build forever and makes decision 1 impossible.

### 3. Interface resolution and start-up

Order: `--tui` or `--gui` on the command line (both together is a usage error, exit 2),
then `interface` in `[general]` of `fasttail.ini` (`gui` or `tui`, anything else read as
`gui`), then `gui`. There is no environment variable: the command line already covers
scripts. The value is read before any window or terminal is touched.

**Windows.** `fasttail.exe` (GUI subsystem) resolving to `tui`:
1. looks for `fasttail-tui.exe` in the directory of `std::env::current_exe()`;
2. if found, starts it with `CreateProcessW` and `CREATE_NEW_CONSOLE`, the same working
   directory and environment, and the original arguments minus `--tui`, plus the
   hidden `--handoff`; it does not wait and exits with code 0;
3. if missing, or if `CreateProcessW` fails, it starts the GUI and shows a modal error
   on the first frame: "fasttail-tui.exe was not found next to fasttail.exe (or could
   not start: <OS error>); the graphical interface is open instead." The error names
   the path looked at;
4. `-` or piped standard input together with the terminal interface is refused: the
   message (on the parent console, as `--version` does) says to run
   `fasttail-tui.exe` from the terminal, and the process exits with code 2. Handing a
   pipe to a child in a new console needs `STARTF_USESTDHANDLES` with console handles
   that do not exist yet; not worth it for a case the direct executable already covers.

`--handoff` tells `fasttail-tui.exe` that its console was created for it: if it exits
with an error (unreadable files only, bad options), it prints the error and waits for a
key before the console closes, so a double click does not flash an unreadable window.

From a terminal the user runs `fasttail-tui.exe`. `fasttail.exe --tui` typed in cmd or
PowerShell still works but opens a second console window: a GUI-subsystem process does
not make the shell wait, so the shell prints its prompt and reads the console at the
same time as any program that attached to it. The README says so.

*Rejected:* `AttachConsole(ATTACH_PARENT_PROCESS)` in `fasttail.exe`: the shell and the
application both read keys from one console, the known garbled-input case.
*Rejected:* one console-subsystem executable that calls `FreeConsole` for the GUI: a
double click flashes a console window before the GUI appears, on every start.

**Linux and macOS.** `fasttail` resolving to `tui` starts the terminal interface in
process when standard output is a terminal, `TERM` is not `dumb`, and a terminal can be
opened for keys (standard input when it is a terminal, otherwise `/dev/tty`, which is
what crossterm reads when standard input is a pipe, so `cmd | fasttail --tui -` works).
Otherwise: with `interface=tui` from the ini (a desktop launcher, a file manager) the
GUI opens and stderr gets one line `interface=tui ignored: no terminal`; with `--tui`
the process prints `--tui needs a terminal` to stderr and exits with code 1.

**`fasttail-tui` (every platform where it is built)** always runs the terminal
interface: `interface` is ignored, `--tui` is accepted and ignored, `--gui` is a usage
error that names `fasttail` / `fasttail.exe`.

### 4. Size budget (release artifacts)

Measured in the prototype: `fasttail-tui.exe` 3.5 MB, `fasttail.exe` 21.7 MB, Linux
x86_64 `fasttail` about 72 MB uncompressed / 20 MB as `.tar.gz`. Budget for 0.20.0,
checked in the release job's log against the previous release:
- `fasttail-tui.exe` at most 6 MB; the Windows zip at most 3 MB larger;
- the Linux and macOS `fasttail` executable at most 3 MB larger uncompressed and its
  `.tar.gz` at most 1.5 MB larger (ratatui and crossterm are small; the shared engine
  is already there);
- `fasttail.exe` at most 0.5 MB larger (it only gains the hand-off).

The release build stays one `cargo build --release` per target with default features
(`--bins` on Windows to get both executables): the linker drops egui and wgpu from
`fasttail-tui.exe` because it never reaches them, as the prototype measured. *Rejected:*
a second `--no-default-features --features tui` release build: it compiles the engine a
second time (minutes per job) for no size gain; the GUI-free `cargo check` in the `test`
job already guards the boundary.

### 5. Shared configuration, read-only, and sessions

At start the terminal interface loads `fasttail.ini` (or `--config`) with the same
`FastTailConfig::load` as the GUI and uses: `theme`, `language`, the workspace (open
files and patterns, per-stream `stream_N` state: filters, search, ANSI mode, collapse,
encoding, bookmarks and `bookmark_note.<line>`, archive entry), `highlight_N` rules and
quick labels, `[global_filter]`, `recent_files`, `recent_sessions`, `search_history`,
`show_line_numbers`, `level_colors` and `sound_enabled`. It ignores the dock layout (it shows streams in workspace
order), wrap, the timeline flag, window, zoom, font, renderer, lock and screensaver
keys. `--fresh`, `--session` and paths on the command line behave as in the GUI.

It **never writes** `fasttail.ini`: the GUI rewrites the whole file on save, so two
writers would silently drop each other's state (the last writer wins). If the file
cannot be parsed (for example mid-write by a running GUI) the load is retried once
after 100 ms, then defaults are used and the status bar says so.

**Session save is in scope.** "Save session as" (`S`) writes the open streams and their
state to a named `*.fasttail-session.ini` through the existing `session` module, with
no `[dock]` section (the GUI opens such a session with its default layout). It refuses
the path of the active `fasttail.ini`, asks before overwriting an existing file, and
does not add the file to `recent_sessions` (that list is in `fasttail.ini`). "Open
session" (`O`) lists `recent_sessions` and accepts a typed path. When bookmarks or
notes changed since the start or the last save, `q` asks "Save session as / Quit /
Cancel", because otherwise they are lost. *Rejected:* deferring session save: bookmarks
and notes made in the terminal would have no way to survive. *Rejected:* writing
`fasttail.ini` under a lock file: the GUI does not take the lock, and changing that is a
separate change.

### 6. Scope of 0.20.0

| Item | 0.20.0 | Reason |
|---|---|---|
| Prototype scope: windows, focused border, dialogs, mouse / `--no-mouse`, `--ascii`, split, follow, search, filters, level, collapse, compressed files | yes | Done in the spike; completed with i18n and tests |
| Keyboard cursor row; bookmarks toggle / next / previous; notes; show in context; go to line or time | yes | Without a cursor row, half the engine's row actions have no key; engine calls exist |
| HEX view | yes | Maintainer decision; the engine serves the bytes and hits, a widget only |
| ANSI colours rendered | yes | Container and CI logs carry colours; the engine already parses the spans (`RowText::ansi`) |
| Archive entry picker dialog | yes | A tar or multi-entry zip is unusable without it; reuses the list dialog |
| Time range dialog | yes, text fields | From / to fields with the GUI's parser; no calendar (mouse-heavy, little gain in a terminal) |
| Highlight rules, quick labels, global filter (applied, toggled with `f`) | yes | Read from the ini; drawing only |
| i18n in 16 languages | yes | Project rule: every visible string goes through `i18n` |
| Open file dialog (typed path) | yes | Needed to open anything after start; path completion later |
| Save / open session | yes | Decision 5 |
| Find results across streams | 0.21.0 | Needs a third pane type and its own keyboard focus in the layout; per-stream `/` and `n` cover the daily case |
| Filter presets, editing the global filter terms, highlight-rule editor | later | Form dialogs (lists, colour pickers) are the biggest share of the port; the GUI edits the ini |
| Wrap, overview strip, text sparkline of the timeline | later | Wrap needs a cell-width layout of its own; the others are extras |
| Free resizable tiling, more than two windows | later | Two-way split covers side-by-side reading |
| External tools | later | Key type decoupled now; launching and the output capture need their own UX |
| Markdown view, screensaver, window lock, renderer / GPU settings | never | Proposal non-goals |

### 7. Keys (defaults; `?` / `F1` lists them)

Cursor and scrolling: `↑` `↓` / `j` `k` move the cursor row; `PgUp` `PgDn` /
`Ctrl+B` `Ctrl+F` move it by a page; `Home` / `g` and `End` / `G` go to the first row
and to the last row with follow on; `←` `→` / `0` scroll sideways. `Space` follow,
`/` search, `n` `N` and `F3` `Shift+F3` next / previous hit, `i` `x` include / exclude,
`l` level, `c` collapse, `s` split, `Tab` next window or stream, `Alt+1..9` stream,
`y` or `Ctrl+C` copy. New: `b` or `Ctrl+F2` toggle bookmark, `]` `[` or `F2`
`Shift+F2` next / previous bookmark, `m` edit note, `Ctrl+K` show in context (and back),
`Ctrl+G` or `:` go to, `h` HEX view, `a` ANSI mode (render, strip, raw), `t` time range,
`f` global filter on / off, `T` next theme (this run only), `o` open file, `O` open
session, `S` save session as, `w` close stream, `q` quit. Where the GUI has a shortcut,
the terminal accepts the same one too.

### 8. HEX view

`h` toggles the focused stream between text and HEX; the choice is per stream and not
persisted. Row layout: an offset of at least 8 hex digits (more above 4 GiB), two
spaces, `n` bytes as two hex digits and a space each with one extra space every 8
bytes, a space, and the `n` bytes as ASCII between `|` (printable 0x20 to 0x7E as is,
anything else `.`). `n` is the largest of 8, 16, 24, 32, 48, 64 whose row fits the
window's inner width; below the width of an 8-byte row the row is clipped. An
80-column terminal (78 inner columns) gets 16 bytes, a 200-column one 32, a 240-column
one 48. Only the visible rows are read (`get_bytes(offset, rows * n)`); rows come from
`total_hex_rows(n)`; the hits of the stream search are `search_byte_matches` (text and
hex-pattern queries, as in the GUI), painted in both columns, the current one reversed,
and `n` / `N` walk them. Switching keeps the place: text to HEX opens at the row holding
the first byte of the cursor line, HEX to text at the line holding the cursor row's
first byte. In HEX view `Ctrl+G` takes a decimal or `0x` byte offset; follow keeps the
last row in view; filters, collapse and ANSI do not apply (the bytes of the file, as
in the GUI). No new memory per file.

### 9. Drawing, threads, memory

Everything the terminal interface does runs on the main thread: crossterm events are
polled with a 100 ms timeout (50 ms while a background job runs), each tick calls
`poll_updates()` on every engine, and the frame is redrawn only when its signature
(line counts, generations, job progress, follow, focus, cursor) changes or on input.
Indexing, filtering, search, collapse, timestamp scans and decompression stay on the
engine's worker threads, with the same 16 MB threshold and progress (shown in the
bottom border). Output goes through a 256 KiB `BufWriter` so a frame reaches the
terminal in one write (160 ms against under 1 ms per frame in the spike). Memory per
file is the engine's (8 bytes per line of index plus the level and timestamp caches);
the terminal reads only the visible rows, with no row cache of its own. Growing,
rotated and truncated files, patterns and standard input behave as in the GUI because
the engine handles them; the terminal and the GUI can tail the same file at the same
time (the engine opens files with read / write / delete sharing on Windows).

**Colours.** Truecolor when `COLORTERM` is `truecolor` / `24bit`, `WT_SESSION` is set or
the Windows console accepts VT sequences; 256 colours when `TERM` contains `256color`;
16 otherwise; `FASTTAIL_TUI_COLORS=16|256|truecolor` overrides. Theme, rule, label and
ANSI colours are reduced to the depth by nearest match. Backgrounds and plain text use
the terminal's own colours. ANSI spans are drawn per the stream's ANSI mode; where a
highlight rule or a search hit colours the same cells, the same precedence as the GUI
applies (search hit over rule over ANSI).

**Text width.** Layout is in terminal cells (`unicode-width`), so CJK labels in zh,
zh-TW, ja and ko dialogs and wide characters in log lines are measured, and `←` `→`
scroll by cells.

## Risks / Trade-offs

- [Two front ends drift, as before 0.7.1] → no engine logic in `tui`; shared
  `workspace` module; `default` builds and tests both; the GUI-free `cargo check` in CI;
  TestBackend capture tests for every view and dialog.
- [A running GUI rewrites `fasttail.ini` while the terminal reads it] → read once at
  start, one retry on a parse error, never write.
- [Bookmarks made in the terminal are lost on quit] → the quit dialog offers "Save
  session as"; the README says the terminal does not write `fasttail.ini`.
- [Windows: a second executable triggers SmartScreen or antivirus prompts] → both are
  built and zipped by the same job; documented in the README troubleshooting section.
- [Windows: `fasttail-tui.exe` deleted or not extracted] → the GUI opens with an error
  naming the path; Settings still shows Terminal selected so the user can switch back.
- [Legacy conhost without VT: raster fonts lack box characters] → automatic ASCII
  borders and 16 colours, as in the spike.
- [mintty (Git Bash) is not a console on Windows] → `fasttail-tui.exe` prints "run from
  Windows Terminal, cmd or PowerShell, or through winpty" and exits with code 1 when it
  cannot enter raw mode.
- [Key conflicts with terminals and multiplexers (`Ctrl+B` in tmux, F-keys over SSH)]
  → every action has a plain-letter key as well.
- [Linux binary grows for users who never use the terminal] → budget of 3 MB
  uncompressed; measured in the release log.

## Open Questions

1. **Headless Linux archive.** Should the release also publish a small
   `fasttail-tui-linux-<arch>-<version>.tar.gz` (no GUI code, about 4 MB) for servers
   and containers, next to the single `fasttail` binary? The design allows it at the
   cost of one more asset per Linux target.
2. **Window lock.** The terminal interface does not honour `lock_enabled` / `lock_pin`.
   Acceptable, given the lock is a deterrent and the terminal session has its own
   locking, or should the terminal refuse to start (or ask for the PIN) when the lock is
   enabled?
3. **`recent_sessions`.** A session saved from the terminal is not added to the GUI's
   recent list because the terminal does not write `fasttail.ini`. Acceptable for
   0.20.0?
4. **Find results in 0.20.0 or 0.21.0.** Deferred here for scope; confirm.
5. **When does "experimental" go.** Proposed: after one release without terminal-only
   bug reports, drop the label from Settings and the README.
6. **`fasttail-tui --gui`.** Proposed as a usage error; alternatively it could start
   `fasttail.exe` / `fasttail` from its own directory, mirroring the hand-off.
