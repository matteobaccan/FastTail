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
monolithic `lib.rs` that compiles `ui` and `renderer` for every binary, and logic living
in GUI methods: the "open anything" dispatch (`open_log_file`, `open_engine_for`,
`open_archive` in `ui/app.rs`), the workspace save (`save_dock_layout`, which also
writes open files and per-stream state), the go-to parser (`ui/dock.rs`), the time-range
parser (`ui/time_range.rs`) and the PIN attempt counter (`LockAttempts` in
`ui/app.rs`). `FastTailConfig::save` serialises the whole configuration and writes it
only when the bytes differ from the file on disk (`write_if_changed`); the GUI calls it
every 2 s from `save_dock_layout` and after most setting changes.

`fasttail.exe` is built with `#![windows_subsystem = "windows"]`. FastTail had a
terminal front end until 0.7.1; `--gui` has been accepted and ignored since then.

Maintainer decisions (2026-09-28): the terminal interface reads and writes
`fasttail.ini` like the GUI; it has Settings, the rule editor, presets, global filter
editing and external tools in 0.20.0; it honours the PIN lock; hand-off works in both
directions; there is a terminal-only Linux archive; no "experimental" label; Find
results in 0.21.0; 0.20.0 ships only at parity for everything in scope.

## Goals / Non-Goals

**Goals:**
- A terminal interface a user can pick instead of the GUI and get the same things:
  viewing, filtering, bookmarks, rules, presets, tools, settings, lock, workspace.
- One engine, one configuration, one workspace format, one set of validation rules for
  both front ends, with no engine or settings logic duplicated in the terminal code.
- A build of the terminal interface that does not compile eframe, egui, wgpu or winit.

**Non-Goals:**
- GUI-only features (Markdown view, screensaver, renderer and window settings) and the
  later items of the scope table.
- Merging concurrent edits of `fasttail.ini`.

## Decisions

### 1. Crate layout: one crate with `gui` and `tui` features, both on by default

`Cargo.toml` gets `default = ["gui", "tui"]`. `gui` enables the optional `eframe`,
`egui`, `egui_dock`, `egui_commonmark` and `rfd` and compiles `ui` and `renderer`;
`tui` enables `ratatui`, `crossterm` and `arboard` and compiles a library module `tui`
(the prototype's `app`, `view`, `keys`, `mouse`, `colors`, `clipboard`, plus the new
dialogs). Everything else (engine, config, session, compressed, filters, tools, i18n and
the new `color`, `workspace`, `lock`, `settings_model` modules) compiles with neither.
Binaries: `fasttail` (`src/main.rs`, `required-features = ["gui"]`, and on Linux and
macOS it also runs the terminal interface) and `fasttail-tui` (`src/bin/fasttail-tui.rs`,
a thin `main` calling `fasttail::tui::run`, `required-features = ["tui"]`).

*Rejected:* a Cargo workspace with a `fasttail-core` crate. It is the cleaner end state,
but it moves some 30 modules and their tests, needs a second crate published before
`fasttail` can be, and gives the same compile-time separation as a feature for a
single-maintainer project. The feature boundary can become a crate boundary later.
*Rejected:* keeping `tui` off by default: the Linux and macOS `fasttail` must contain
it, and a default build that tests half the code let the old front end rot before 0.7.1.

### 2. Decoupling (first task group)

- `src/color.rs`: `Rgba { r, g, b, a: u8 }`. `HighlightStyle`, rule colours, the
  theme palette and every engine-facing colour use it; `From<Rgba> for egui::Color32`
  and `CyberTheme::apply(ctx)` live under `gui`.
- `markdown_cache` moves from `TailEngine` to the GUI's per-tab state.
- `external_tools::Shortcut` stores a neutral `KeyName` and `Mods`; each front end maps
  them to its own key events. The `tool.N` ini format does not change.
- `src/workspace.rs`: `open_target(path) -> OpenOutcome` (plain file, pattern,
  compressed, archive entry, archive needing a choice with its entries, standard input),
  `restore(config)`, `snapshot(engines) -> WorkspaceState` (open files, `stream_N`
  state, bookmarks and notes) which both front ends write into `FastTailConfig`, the
  go-to parser and the time-range text parser.
- `src/lock.rs`: `LockAttempts`, `LOCK_MAX_FAILURES`, `LOCK_COOLDOWN`, `pin_matches`
  and an `IdleClock` (last input instant, arming rule), shared by the GUI lock screen
  and the terminal one.
- `src/settings_model.rs`: the ranges, defaults and validation of every setting both
  interfaces edit (the clamps now inside `FastTailConfig::load` and the GUI Settings
  widgets), highlight-rule and tool validation, so the terminal dialogs accept exactly
  what the GUI accepts.
- The search wrap-around beep and rule sound alerts become events the engine returns;
  the GUI plays them under `audio`; the terminal rings its bell when `sound_enabled`.
- A `test` job step `cargo check --no-default-features --features tui` guards the
  boundary.

*Rejected:* mapping `Color32` at the terminal edge as the prototype does: it keeps egui
in the terminal build for good and makes decision 1 and the terminal-only archive
impossible.

### 3. Interface resolution and hand-off in both directions

Order: `--tui` or `--gui` on the command line (both is a usage error, exit 2), then
`interface` in `[general]` (`gui` or `tui`, anything else read as `gui`), then `gui`.
No environment variable: the command line covers scripts.

**Windows (two executables).**
- `fasttail.exe` resolving to `tui` looks for `fasttail-tui.exe` next to
  `std::env::current_exe()` and starts it with `CreateProcessW` + `CREATE_NEW_CONSOLE`,
  same working directory and environment, its arguments minus `--tui` plus a hidden
  `--handoff`, then exits 0 without waiting. `--handoff` makes the new console wait for
  a key after an error exit, so a double click does not flash an unreadable window.
  If `fasttail-tui.exe` is missing or fails to start, the GUI opens and shows a modal
  error naming the path and the OS error: a double-clicked GUI-subsystem process has no
  console to print to, so the only visible outcome is a window. `-` or piped standard
  input with the terminal interface is refused with exit 2 and a message (parent
  console, as for `--version`) pointing to `fasttail-tui.exe`: handing a pipe to a
  child in a new console is not worth it when the direct executable covers the case.
- `fasttail-tui.exe --gui` looks for `fasttail.exe` next to itself and starts it
  detached (`DETACHED_PROCESS`), arguments minus `--gui`, and exits 0. If it is missing
  it prints `fasttail.exe was not found in <dir>` to stderr and exits 1: it was started
  only to hand off, and a terminal user reads stderr.
- From a terminal the user runs `fasttail-tui.exe`. `fasttail.exe --tui` typed in cmd
  or PowerShell works but opens a second console window, because a GUI-subsystem process
  does not make the shell wait, so the shell and any program attached to its console
  would read keys together. The README says so.

**Linux and macOS (one binary).** `fasttail` resolving to `tui` runs the terminal
interface in process when standard output is a terminal, `TERM` is not `dumb`, and a
terminal can be opened for keys (standard input, or `/dev/tty` when standard input is a
pipe, so `cmd | fasttail --tui -` works). Otherwise: from the ini, the GUI opens with the
stderr line `interface=tui ignored: no terminal`; from `--tui`, stderr gets
`--tui needs a terminal` and the exit code is 1. The terminal-only build
(`fasttail-tui` from the Linux terminal-only archive) answers `--gui` with
`the graphical interface is not in this build (fasttail-tui-linux-...); use the
fasttail archive` and exit code 2, and its Settings shows the Graphical entry disabled
with the same note.

**Switching from Settings, in either interface.** Changing the interface saves
`interface` and offers "Switch now" or "At next start". "Switch now" saves the
workspace, then:
- GUI to terminal: on Windows the same hand-off as above, then the GUI closes; on Linux
  and macOS the GUI usually has no terminal, so the choice applies at the next start
  from a terminal and the dialog says so;
- terminal to GUI: on Windows `fasttail.exe` is started detached, on Linux and macOS the
  same executable with `--gui` when `DISPLAY` or `WAYLAND_DISPLAY` is set (always on
  macOS); the terminal interface then quits and restores the terminal.
If the other executable is missing, or there is no display, a message (GUI) or a
bordered dialog (terminal) says why, and the current interface keeps running.

*Rejected:* `AttachConsole(ATTACH_PARENT_PROCESS)` in `fasttail.exe` (shell and
application read one console: the known garbled-input case). *Rejected:* one
console-subsystem executable that calls `FreeConsole` for the GUI (a console window
flashes on every double click).

### 4. Builds and size budget

Measured in the prototype: `fasttail-tui.exe` 3.5 MB, `fasttail.exe` 21.7 MB, Linux
x86_64 `fasttail` about 72 MB uncompressed (debug info kept, not stripped) and 20 MB as
`.tar.gz`. With Settings, the rule editor, presets, global filter and tools dialogs
added (an estimated 6,000 more lines of terminal code), the 0.20.0 budget against the
previous release is:

| Artifact | Budget |
|---|---|
| `fasttail-tui.exe` | at most 8 MB |
| Windows zip | at most 4 MB larger |
| `fasttail.exe` | at most 1 MB larger (hand-off, lock and settings moved to shared modules) |
| Linux / macOS `fasttail` | at most 5 MB larger uncompressed, 2 MB larger `.tar.gz` |
| Linux `fasttail-tui` (terminal-only, per arch) | at most 25 MB uncompressed (debug info kept), `.tar.gz` at most 8 MB |

Builds: Windows `cargo build --release --bins` (one build, both executables; the linker
drops egui and wgpu from `fasttail-tui.exe`); macOS `cargo build --release`; Linux
x86_64 and ARM64 `cargo build --release` for `fasttail`, then
`cargo build --release --no-default-features --features tui --bin fasttail-tui` for the
terminal-only archive. The second Linux build compiles the engine again (a few minutes
per job) but it is the only way to prove the archive has no GUI crate; the job fails if
`cargo tree` for that build lists eframe, egui, wgpu or winit. *Rejected:* taking the
terminal-only binary from the default build (as on Windows): it would link fine, but
nothing would guarantee the GUI crates are absent from a binary sold as GUI-free, and
the ELF keeps more of them than the MSVC linker does.

### 5. Configuration: read and written like the GUI; last writer wins

The terminal interface loads `fasttail.ini` (or `--config`) with the same
`FastTailConfig::load` and saves it with the same `FastTailConfig::save`, including the
fallback to the user directory. It writes what it owns (open files and patterns in
workspace order, `stream_N` state, bookmarks and notes, recent files and sessions,
search history, theme, language, every setting of its Settings dialog, rules, quick
labels, presets, global filter, tools, `interface`) and carries every other key
through unchanged as it read it (dock layout, window geometry, zoom, font, renderer,
frame rates, dialog positions). It saves when the GUI would: every 2 s when its state
changed, after a Settings `[ OK ]` or any editor confirmation, on session operations, and
on exit. The GUI, when it restores a dock layout whose tabs differ from `open_files`,
drops tabs of files no longer open and adds the new ones to the main area, so a
workspace changed in the terminal opens cleanly.

**Concurrent instances: last writer wins per file**, as with two GUI instances today,
with one fix applied to both front ends: an instance writes only when its **own**
serialised state differs from the bytes it last loaded or wrote, not when the file on
disk differs. Today `save_dock_layout` calls `save` every 2 s and `write_if_changed`
compares with the disk, so two instances with different state overwrite each other
every 2 s for as long as both run. With the fix, a GUI and a terminal open on the same
file write only when their user changes something; whoever changes something last (or
exits last with changes) defines the file. The fix ships **early, as a separate GUI
bugfix in 0.13.0** (maintainer's answer, 2026-09-29): two GUI instances already suffer
from it today; this change only relies on it and applies the same rule to the terminal. Neither instance reloads the file while
running. The README documents this.

*Rejected:* reload and merge before each save. It needs a three-way merge of some 40
sections (lists of streams, bookmarks per file, rules, tools, presets), where a stream
absent from one side is ambiguous (closed here or opened there), it would have to be
built into both front ends, and without a file lock two merges can still interleave.
*Rejected:* a single-instance lock on `fasttail.ini`: it would stop the common "GUI on
the desk, terminal over SSH" case from starting at all.

Sessions work as in the GUI: `--session`, open session (recent list or path), save
session and save as (with the dock layout), `recent_sessions` updated.

The windows follow the GUI's dock: `src/dock_layout.rs` reads and writes the `egui_dock`
RON of `[dock] layout` without egui (mirror types, a GUI layout comes back byte for
byte) and gives the terminal a `Pane` tree of splits and leaves of tabs. The terminal
lays the tree out in cells (`tui/dock.rs`), draws each leaf as a window with its tabs in
the top border, and changes it with keys and the mouse (dividers, title drag with edge
and centre drop zones). The GUI's floating windows and panels are kept; the layout is
written back only after a change, like every other key.

### 6. Scope of 0.20.0

| Item | When | Reason |
|---|---|---|
| Prototype scope: windows, focused border, dialogs, mouse / `--no-mouse`, `--ascii`, split, follow, search, filters, level, collapse, compressed files | 0.20.0 | Done in the spike; completed with i18n and tests |
| Cursor row; bookmarks toggle / next / previous; notes; show in context; go to line or time | 0.20.0 | Without a cursor row the engine's row actions have no key |
| HEX view | 0.20.0 | Engine serves the bytes and hits; a widget only |
| ANSI colours rendered | 0.20.0 | The engine parses the spans; container logs carry colours |
| Archive entry picker, time-range dialog (text fields) | 0.20.0 | Multi-entry archives are unusable without the picker; the calendar is mouse-heavy and adds little in a terminal |
| Settings dialog (terminal-relevant keys) | 0.20.0 | Same settings expected in both interfaces |
| Highlight-rule editor, quick labels, filter presets, global filter editing | 0.20.0 | Same |
| External tools: editor, run on the cursor row, rule-bound runs | 0.20.0 | Same |
| PIN lock with idle lock | 0.20.0 | Ignoring a protection the user set is wrong |
| Reading and writing `fasttail.ini`, sessions | 0.20.0 | Decision 5 |
| i18n in 16 languages | 0.20.0 | Project rule |
| Find results across streams | 0.21.0 | Needs a third pane type and its own focus in the layout; `/` and `n` per stream cover the daily case |
| Line wrap, overview strip, timeline sparkline, free resizable tiling, more than two windows | later | Wrap needs its own cell-width layout; the rest are extras |
| Markdown view, screensaver | never | No meaning in a terminal; the terminal goes straight to the lock screen |
| Renderer, GPU, frame rate, zoom, font, always-on-top, borderless, window settings | never in the terminal | They concern the GUI only; kept untouched in the ini |

### 7. Keys (defaults; `?` / `F1` lists them)

Cursor and scrolling: `↑` `↓` / `j` `k` move the cursor row; `PgUp` `PgDn` /
`Ctrl+B` `Ctrl+F` by a page; `Home` / `g` first row, `End` / `G` last row with follow
on; `←` `→` / `0` sideways. `Space` follow, `/` search, `n` `N` and `F3` `Shift+F3`
next / previous hit, `i` `x` include / exclude, `l` level, `c` collapse, `s` `|` `_` new window
beside / below, `Ctrl+W` close window, `<` `>` move the stream to another window,
`Ctrl+PgUp` `Ctrl+PgDn` tab of the window, `Alt+arrows` divider, `Tab` next window or
stream, `Alt+1..9` stream, `y` or `Ctrl+C` copy, `b` or `Ctrl+F2`
bookmark, `]` `[` or `F2` `Shift+F2` next / previous bookmark, `m` note, `Ctrl+K` show
in context, `Ctrl+G` or `:` go to, `h` HEX view, `a` ANSI mode, `t` time range, `f`
global filter on / off, `F` global filter editor, `p` presets, `r` rule editor, `!`
external tools menu, `,` Settings, `Ctrl+L` lock, `T` next theme (saved like a theme
change in the GUI), `o` open file (folder browser), `O` open session, `S` save session as, `w` close
stream, `q` quit. From the competitor analysis after 0.12.0 (section 7): `e` / `E` and
`w` / `W` jump to the next / previous error and warning (lnav); movement keys take a
count (`10j`, `3n`, less / vim); `?` and `F1` work even with no file open; `:` opens the
command palette over the shared action registry of `command-palette` (0.13.0), where a
number jumps to that line, so the go-to dialog keeps `Ctrl+G`; the default bindings come
from that registry, so `remappable-shortcuts` (0.22.0) rebinds both interfaces; the
Kitty keyboard protocol is enabled when the terminal supports it, so `Ctrl+Shift`
combinations are told apart. The status bar keeps the key hints on screen, idle drawing
stays event-driven (toolong #17 burned a core polling), and pipes work as
`cmd | fasttail-tui -` next to `fasttail --print` (`headless-print`, 0.13.0). Where the GUI has a shortcut, the terminal accepts it too; every
action also has a plain key, because multiplexers and terminals swallow some modifier
combinations (`Ctrl+B` in tmux, `Ctrl+Shift` letters in conhost). Tool shortcuts the
terminal cannot deliver are still reachable from the `!` menu.

### 8. HEX view

`h` toggles the focused stream between text and HEX; per stream, not persisted. Row:
an offset of at least 8 hex digits (more above 4 GiB), two spaces, `n` bytes as two hex
digits and a space each with one extra space every 8 bytes, a space, and the `n` bytes
as ASCII between `|` (0x20 to 0x7E as is, else `.`). `n` is the largest of 8, 16, 24,
32, 48, 64 whose row fits the inner width (80 columns: 16; 200: 32; 240: 48); narrower
windows clip an 8-byte row. Only visible rows are read (`get_bytes`), rows come from
`total_hex_rows(n)`, hits from `search_byte_matches` (text and hex-pattern queries),
painted in both columns, the current one reversed, walked by `n` / `N`. Switching keeps
the place (cursor line's first byte, and back). In HEX view `Ctrl+G` takes a decimal or
`0x` offset, follow keeps the last row in view, and filters, collapse and ANSI do not
apply (the bytes of the file, as in the GUI). No new memory per file.

### 9. Lock screen

Arming follows the GUI rule through the shared `lock` module: `Ctrl+L` locks when a PIN
is set (nothing happens otherwise); with `lock_enabled` and a PIN set,
`screensaver_timeout_mins` minutes without a key or mouse event lock the terminal,
whatever `screensaver_enabled` says: that switch only decides whether the GUI shows the
Matrix screensaver before locking, and the terminal has no screensaver (maintainer's
answer, 2026-09-29). While locked, the whole screen is one bordered PIN
dialog on a blank background: no log line, file name, count or status text is drawn,
and every key except the PIN field's editing keys and `Enter` is dropped, `Esc`, `q` and
`Ctrl+C` included. Three wrong PINs start the 60 s cooldown (countdown instead of the
field), the maintenance phrase works as in the GUI, and the counter lives in memory.
Streams keep tailing while locked. The terminal's own scrollback is not drawn into: the
alternate screen is used, so the shell's history above holds nothing of the log.

### 10. Settings, editors and tools

All dialogs share one form toolkit in `tui` (text field, number field with range,
check box, radio list, list with reorder, colour field), and all validation comes from
`settings_model`.
- **Settings** (`,`): Interface; Appearance (theme, language with "follow system",
  line numbers, level colours, time delta and its gap); Performance and refresh
  (`poll_interval_ms`, `size_check_interval_ms`, `spool_dir`, `compressed_max_gb`,
  `stdin_spool_max_mb`); New streams and bookmarks (the defaults the GUI applies to new
  streams, `auto_bookmark_max`, `size_unit`); Sound (`sound_enabled`); PIN lock (set /
  change / remove PIN, `lock_enabled`, which also arms the idle lock, and the idle
  minutes, `screensaver_timeout_mins`; `screensaver_enabled` is kept as read; with the
  deterrent statement).
  `[ OK ]` validates, applies and saves; `[ Cancel ]` discards.
- **Rule editor** (`r`): the ordered list of highlight rules (pattern, regex and case
  flags, capture-only, fg and bg colour, bold, italic, auto-bookmark, sound alert, bound
  tool, enabled), add, edit, delete, move up / down (`Alt+↑` `Alt+↓` or `K` `J`). Colours
  are picked from the theme's swatches or typed as `#RRGGBB`, previewed at the
  terminal's depth. Quick labels are listed and removable in the same dialog.
- **Presets** (`p`): apply, save current filters as, rename, delete.
- **Global filter** (`F`): the on switch, up to 8 include and 8 exclude terms, `Aa`
  and `.*` toggles, applied 300 ms after the last key as in the GUI.
- **External tools**: the editor is a Settings page (name, command, arguments with the
  placeholder list, regex, shortcut, shell flag, rule binding, dropped-run count);
  `e` opens the menu of tools for the cursor row. A tool's child process gets standard
  input, output and error set to null, never the terminal, so it cannot draw over the
  interface or read its keys; rule-bound runs keep the limit of one run per second per
  tool and 10 concurrent children.

### 11. Drawing, threads, memory

Everything the terminal interface does runs on the main thread: crossterm events are
polled with a 100 ms timeout (50 ms while a background job runs), each tick calls
`poll_updates()` on every engine and checks the idle clock and the 2 s save, and the
frame is redrawn only when its signature changes or on input. Indexing, filtering,
search, collapse, timestamp scans and decompression stay on the engine's workers, with
the same 16 MB threshold and progress in the bottom border; tool children run as in the
GUI. Output goes through a 256 KiB `BufWriter` (160 ms against under 1 ms per frame in
the spike). Memory per file is the engine's (8 bytes per line of index plus the level
and timestamp caches); only visible rows are read. Growing, rotated and truncated
files, patterns and standard input behave as in the GUI; both interfaces can tail the
same file at once (read / write / delete sharing on Windows). Configuration saves are
a few kilobytes on the main thread, as in the GUI.

**Colours.** Truecolor when `COLORTERM` is `truecolor` / `24bit`, `WT_SESSION` is set or
the Windows console accepts VT sequences; 256 colours when `TERM` contains `256color`;
16 otherwise; `FASTTAIL_TUI_COLORS=16|256|truecolor` overrides. Colours are reduced by
nearest match; at 256 colours and truecolor the theme paints backgrounds, text and
borders as in the GUI (at 16 colours the terminal's own stay), and dialogs cast a shadow.
Precedence: search hit over rule over ANSI, as in the GUI. **Text width** is measured in cells
(`unicode-width`) for CJK dialogs and wide characters.

### 12. Release gate

0.20.0 is tagged only when every task of groups 1 to 7 is done and the parity checklist
(task 7.1) passes: every item marked 0.20.0 in the scope table works in the terminal on
Windows (Windows Terminal and conhost) and Linux, and each setting, rule, preset, tool
and bookmark changed in one interface is found unchanged in the other after a restart.
No partial terminal interface ships under a label.

## Effort

One developer, from the prototype:

| Step | Effort |
|---|---|
| Decoupling (colours, Markdown cache, key type, features, `workspace`, `lock`, `settings_model`) | 1–1.5 weeks |
| Viewer completion: cursor row, bookmarks, notes, context, go-to, HEX, ANSI, picker, time range, sessions | 1.5–2 weeks |
| Configuration write, own-change saves in both front ends, dock reconciliation | 3–4 days |
| Form toolkit, Settings, rule editor, presets, global filter, external tools | 2.5–3.5 weeks |
| Lock screen, hand-offs both ways, Settings switch | 4–5 days |
| i18n (16 languages), packaging incl. Linux terminal-only, docs, parity pass | 1–1.5 weeks |

About **8–10 weeks** in all, against 4–6 weeks estimated for the smaller scope.

## Risks / Trade-offs

- [Two front ends drift, as before 0.7.1] → no engine, settings or lock logic in `tui`;
  shared modules; default builds test both; GUI-free check in CI; `TestBackend` capture
  tests for every view and dialog; the parity checklist gates the release.
- [GUI and terminal on one `fasttail.ini`: an older state overwrites a newer one] →
  own-change saves remove the 2 s ping-pong; last writer wins is documented; each save
  still goes through `write_if_changed`.
- [The GUI's dock layout goes stale after the terminal changed the open files] →
  reconciliation at restore (decision 5), covered by a test.
- [A tool's child writes to the terminal] → null standard handles for every child
  started by the terminal interface.
- [Lock screen leaks log text] → the locked frame is built from the lock dialog only;
  a capture test asserts that no line of the open files appears in it.
- [Windows: a second executable triggers SmartScreen or antivirus prompts] → both are
  built and zipped by the same job; README troubleshooting.
- [Legacy conhost without VT, mintty] → ASCII borders and 16 colours; mintty gets a
  message and exit code 1.
- [Size grows with the dialogs] → budget table, sizes printed by every build job.

## Resolved Questions (2026-09-29)

1. **Idle lock condition** → the terminal locks on idle whenever `lock_enabled` is on and
   a PIN is set, after `screensaver_timeout_mins`; `screensaver_enabled` only concerns the
   GUI's screensaver (decision 9).
2. **Own-change saves in the GUI** → yes, shipped early as a separate bugfix in 0.13.0
   (decision 5).
