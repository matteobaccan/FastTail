## ADDED Requirements

### Requirement: Interface Selection and Launch
FastTail SHALL offer two interfaces, graphical and terminal, chosen independently of the renderer. The interface SHALL be resolved from `--tui` or `--gui` on the command line, then `interface` (`gui` or `tui`) in `[general]` of `fasttail.ini`, then `gui`; any other ini value SHALL be read as `gui`. On Windows, `fasttail.exe` resolving to the terminal interface SHALL start `fasttail-tui.exe` from the directory of its own executable in a new console window, with the same working directory and environment and its arguments minus `--tui`, and SHALL then exit with code 0 without waiting. If `fasttail-tui.exe` is missing or cannot be started, `fasttail.exe` SHALL open the graphical interface and show an error naming the path it looked for and the OS error, if any. When the new console was created by this hand-off and `fasttail-tui.exe` exits with an error, it SHALL print the error and wait for a key before its console closes. `fasttail.exe` SHALL refuse the terminal interface together with standard input (`-` or a pipe) with a message pointing to `fasttail-tui.exe` and exit code 2. On Linux and macOS, `fasttail` SHALL run the terminal interface in its own process when standard output is a terminal, `TERM` is not `dumb`, and a terminal can be opened for keys (standard input, or `/dev/tty` when standard input is a pipe); otherwise it SHALL open the graphical interface with one stderr line when the choice came from `fasttail.ini`, and SHALL exit with code 1 and `--tui needs a terminal` on stderr when it came from `--tui`. `fasttail-tui` SHALL always run the terminal interface, ignoring `interface` and `--tui`.

#### Scenario: Windows, double click with interface=tui
- **WHEN** `fasttail.ini` holds `interface=tui` and the user double-clicks `fasttail.exe`, with `fasttail-tui.exe` in the same directory
- **THEN** a console window opens running `fasttail-tui.exe` with the workspace's streams, and no `fasttail.exe` process remains.

#### Scenario: Windows, --tui with the terminal executable missing
- **WHEN** the user runs `fasttail.exe --tui app.log` from a directory where `fasttail-tui.exe` was not extracted next to `fasttail.exe`
- **THEN** the graphical window opens with `app.log`, and a dialog says that `fasttail-tui.exe` was not found and names the full path looked for.

#### Scenario: Windows, --tui from PowerShell
- **WHEN** the user runs `fasttail.exe --tui app.log` in PowerShell with both executables present
- **THEN** a new console window runs `fasttail-tui.exe app.log`, and PowerShell returns to its prompt without sharing its console with FastTail.

#### Scenario: Windows, direct terminal start
- **WHEN** the user runs `fasttail-tui.exe app.log` in cmd
- **THEN** the terminal interface draws inside that cmd window, cmd waits until it quits, and the prompt comes back with the console restored.

#### Scenario: Windows, handed-off start fails
- **WHEN** `fasttail.exe --tui missing.log` hands off and `fasttail-tui.exe` cannot open any stream and exits with an error
- **THEN** the new console shows the error and stays open until a key is pressed.

#### Scenario: Linux, --tui over SSH
- **WHEN** the user runs `fasttail --tui /var/log/syslog` in an SSH session
- **THEN** the terminal interface opens in that terminal and no window or display connection is attempted.

#### Scenario: Linux, interface=tui from a desktop launcher
- **WHEN** `interface=tui` is set and FastTail is started from a desktop menu entry with no terminal
- **THEN** the graphical window opens and stderr has the line `interface=tui ignored: no terminal`.

#### Scenario: Linux, --tui without a terminal
- **WHEN** the user runs `fasttail --tui app.log > out.txt`
- **THEN** nothing is drawn, stderr says `--tui needs a terminal`, and the exit code is 1.

#### Scenario: Linux, piped input into the terminal interface
- **WHEN** the user runs `journalctl -f | fasttail --tui -` in a terminal
- **THEN** the terminal interface opens a `stdin` stream following the command's output and reads keys from the terminal.

### Requirement: Engine Independent of the GUI Toolkit
The engine, configuration, session, compressed-file, filter, theme palette, external-tool definition and i18n code SHALL compile without eframe, egui, egui_dock, egui_commonmark, wgpu, winit and rfd. Engine-facing colours SHALL use a UI-neutral RGBA type, `TailEngine` SHALL NOT hold Markdown render state, external-tool shortcuts SHALL use a UI-neutral key type with the `tool.N` ini format unchanged, and opening a path (plain file, pattern, compressed file, archive entry, archive needing an entry choice, standard input), restoring the workspace, parsing a go-to target and parsing a time-range text SHALL be UI-agnostic functions used by both interfaces. The build `cargo build --no-default-features --features tui --bin fasttail-tui` SHALL succeed.

#### Scenario: GUI-free build
- **WHEN** `cargo tree --no-default-features --features tui -e normal` is run
- **THEN** its output contains none of `eframe`, `egui`, `wgpu`, `winit`, `egui_dock`, `egui_commonmark` or `rfd`.

#### Scenario: One open path for both interfaces
- **WHEN** `logs.tar.gz` with three entries is opened from the GUI and from the terminal interface
- **THEN** both call the same open function, which reports an entry choice with the same three entry names in the same order.

### Requirement: Stream Windows and Layout
Each stream SHALL be drawn as a bordered window whose top border shows `[#N]`, the file name and FOLLOW or PAUSED, and whose bottom border shows the visible and total line counts and any background job with its percentage on the left, and the active filters, level, collapse, time range and search position (`/text i/n`) on the right. The focused window SHALL use a double border in the theme's accent colour, the others a single border. With two or more streams a strip of their names SHALL be shown, marking with `+` a hidden stream that received lines. `s` SHALL cycle one window, two side by side and two stacked; `Tab` / `Shift+Tab` SHALL move the focus between windows and then between streams; `Alt+1..9` SHALL show stream 1 to 9. A status bar SHALL list the main keys and show messages. `--ascii`, `FASTTAIL_TUI_ASCII`, or a Windows console without virtual-terminal support SHALL draw every border with `+`, `-` and `|` (the focused window with `=`). A terminal resize SHALL redraw at the new size within 100 ms. At least 40 columns by 10 rows SHALL be usable; below that a one-line message SHALL ask for a larger terminal.

#### Scenario: Focused window of a split
- **WHEN** two files are open with `--split` and the user presses `Tab`
- **THEN** the right window gets the double border and the left one a single border, and `Space` then toggles follow of the right stream only.

#### Scenario: ASCII borders
- **WHEN** the user runs `fasttail-tui --ascii app.log`
- **THEN** no box-drawing character is written: the focused window's border uses `+` and `=`, and the status bar uses `+`, `-` and `|`.

#### Scenario: Background line arriving on a hidden stream
- **WHEN** three streams are open, stream 1 is shown, and 5 lines are appended to stream 3
- **THEN** the strip shows `+` next to stream 3's name within 200 ms, and `Alt+3` shows those lines.

### Requirement: Keyboard Cursor Row
Each stream window SHALL have a cursor row, drawn reversed, that the keys move: `↑` `↓` / `j` `k` by one row, `PgUp` `PgDn` / `Ctrl+B` `Ctrl+F` by a page, `Home` / `g` to the first row and `End` / `G` to the last row with follow on. The view SHALL scroll to keep the cursor visible, and moving the cursor up SHALL pause follow. `←` `→` SHALL scroll sideways by one cell and `0` SHALL return to the first column, measuring wide characters by their cell width. Row actions (copy, bookmark, note, show in context) SHALL act on the selection when there is one, otherwise on the cursor row; `Shift+↑` / `Shift+↓` SHALL extend the selection from the cursor. On a collapsed group the cursor row SHALL stand for the whole group, as in the GUI.

#### Scenario: Paging with the cursor
- **WHEN** a 10,000-line stream shows rows 1 to 40 with the cursor on row 40 and the user presses `PgDn`
- **THEN** the cursor is on row 80, it is visible at the bottom of the window, and follow is off.

#### Scenario: Back to the tail
- **WHEN** the cursor is on row 120 of a growing file and the user presses `G`
- **THEN** the cursor is on the last row, follow is on, and appended lines keep the cursor on the new last row.

### Requirement: Search, Filters, Level and Collapse
`/` SHALL open a search dialog; `n` / `N` and `F3` / `Shift+F3` SHALL move to the next and previous hit with wrap-around, moving the cursor to it; hit rows SHALL carry `>` in the gutter, the matched text SHALL be painted and the current hit's line number reversed. `i` and `x` SHALL edit the include and exclude terms, `l` SHALL cycle the minimum level (off, DEBUG, INFO, WARN, ERROR), `c` SHALL cycle collapse (off, exact, numbers, collapsed groups showing a `[Nx]` badge), `f` SHALL switch the global filter from `fasttail.ini` on and off for this run, and `t` SHALL open the time-range dialog. All of them SHALL use the engine's own filter, search and collapse, running on its worker threads above 16 MB with the progress in the bottom border. `Esc` SHALL clear the search and the selection.

#### Scenario: Include filter on a large file
- **WHEN** the user sets the include term `GET` on a 1 GB file with 10,000,000 lines
- **THEN** the bottom border shows the filter job's percentage while the window stays responsive, and when it ends only rows containing `GET` are shown and the count says how many.

#### Scenario: Wrapping search
- **WHEN** the current hit is the last of 4 and the user presses `n`
- **THEN** the cursor moves to the first hit and the bottom border reads `1/4`.

### Requirement: Bookmarks, Notes, Show in Context and Go To
`b` and `Ctrl+F2` SHALL toggle a bookmark on the cursor row, drawn as `*` in the gutter (an automatic bookmark as `o`); `]` / `F2` and `[` / `Shift+F2` SHALL move the cursor to the next and previous bookmark visible under the active filters, wrapping around. `m` SHALL open a note dialog for the cursor row's bookmark, creating the bookmark when needed; bookmarked rows with a note SHALL show the first 40 characters of the note in the status bar when the cursor is on them. `Ctrl+K` on a filtered stream SHALL enter the engine's context view on the cursor row, with a banner line saying the filters are suspended, and `Ctrl+K` or `Esc` SHALL return to the filtered view exactly as before, following the Show In Context requirement of `search-and-navigation`. `Ctrl+G` and `:` SHALL open a go-to dialog accepting `N`, `+N`, `-N` or a time such as `14:02`, with the GUI's parsing and the same "next visible line" resolution.

#### Scenario: Bookmark and walk
- **WHEN** the user presses `b` on lines 120 and 900 and then `[` with the cursor on line 950
- **THEN** the cursor moves to line 900, and a second `[` moves it to line 120.

#### Scenario: Note on a bookmark
- **WHEN** the user presses `m` on line 88, types `payment retried`, and confirms
- **THEN** line 88 is bookmarked, and moving the cursor onto it shows `payment retried` in the status bar.

#### Scenario: Context from the keyboard
- **WHEN** a stream filtered by `ERROR` has the cursor on line 48,211 and the user presses `Ctrl+K`
- **THEN** the window shows every line with 48,211 at the cursor and centred, the banner says the filters are suspended, and `Esc` returns to the `ERROR` rows with the cursor back on 48,211.

#### Scenario: Go to a time
- **WHEN** the user presses `Ctrl+G` and types `14:02` in a stream whose lines carry timestamps
- **THEN** the cursor moves to the first visible line at or after 14:02 on the file's first day.

### Requirement: Terminal HEX View
`h` SHALL switch the focused stream between the text view and a HEX view, per stream and not persisted. Each HEX row SHALL show the byte offset in hexadecimal (at least 8 digits), `n` bytes as two hex digits separated by spaces with one extra space every 8 bytes, and the same bytes as ASCII between `|` (0x20 to 0x7E as is, any other byte as `.`). `n` SHALL be the largest of 8, 16, 24, 32, 48 and 64 whose row fits the window's inner width, recomputed on resize. The bytes SHALL come from the engine's HEX APIs (`total_hex_rows`, `get_bytes`), reading only the visible rows; the stream search's byte hits (`search_byte_matches`, text or hex-pattern queries) SHALL be painted in both the hex and ASCII columns, the current hit reversed, and `n` / `N` SHALL walk them. Switching SHALL keep the place (text to HEX: the row holding the first byte of the cursor line; HEX to text: the line holding the cursor row's first byte). In the HEX view `Ctrl+G` SHALL accept a decimal or `0x` byte offset, follow SHALL keep the last row in view, and filters, collapse and ANSI SHALL NOT apply.

#### Scenario: HEX view in an 80-column terminal
- **WHEN** `data.bin` (1,000 bytes) is open in an 80 x 24 terminal and the user presses `h`
- **THEN** each row shows 16 bytes, the first row starts with offset `00000000`, and the window has 63 rows in all, the last one holding 8 bytes.

#### Scenario: HEX view in a wide terminal
- **WHEN** the same stream is shown in a 240-column terminal
- **THEN** each row shows 48 bytes.

#### Scenario: Search hit in HEX
- **WHEN** in the HEX view the user searches `ERROR` and the first hit starts at byte 0x1A2
- **THEN** the cursor is on the row holding offset 0x1A2, the five bytes `45 52 52 4F 52` and the letters `ERROR` in the ASCII column are painted as the current hit.

#### Scenario: Back to text at the same place
- **WHEN** the HEX cursor is on the row at offset 0x4000, inside line 312, and the user presses `h`
- **THEN** the text view shows line 312 at the cursor.

### Requirement: ANSI Colours in the Terminal
Lines with ANSI SGR sequences SHALL be drawn according to the stream's ANSI mode: `render` SHALL draw the foreground, background, bold, dim, italic, underline and reverse attributes of the engine's parsed spans (`RowText::ansi`), `strip` SHALL draw the text without them, and `raw` SHALL show the escape characters as visible text (`ESC` as `^[`), never sending them to the terminal. `a` SHALL cycle the mode for the focused stream for this run. Search hits and highlight rules SHALL take precedence over ANSI colours as in the GUI. No escape sequence from a log line SHALL ever be written to the terminal as a control sequence.

#### Scenario: Coloured container log
- **WHEN** a line holds `\x1b[31mFAILED\x1b[0m done` in `render` mode
- **THEN** `FAILED` is drawn in the terminal's red (or the nearest colour at its depth) and ` done` in the default colour.

#### Scenario: Hostile escape sequence
- **WHEN** a log line contains `\x1b]52;c;ZXZpbA==\x07` (an OSC 52 clipboard write) or `\x1b[2J`
- **THEN** the clipboard and the screen are not affected, and in `raw` mode the characters appear as `^[]52;c;ZXZpbA==^G`.

### Requirement: Terminal Dialogs
Dialogs SHALL open centred over the windows, clear what is under them, take the keyboard until closed, and offer `[ OK ]` / `[ Cancel ]` (or the named buttons) that `Enter` / `Esc` and the mouse press. The 0.20.0 dialogs SHALL be: help (`?` / `F1`, listing every key), search, include, exclude, go to, bookmark note, archive entry picker, time range (from / to text fields with the GUI's parser, `Enter` applies, `Esc` keeps the previous range, an empty field removes that side), open file (typed path), open session (the `recent_sessions` list and a typed path), save session as, and the quit confirmation. Text fields SHALL support `←` `→`, `Home` `End`, `Backspace`, `Delete`, `Ctrl+U` to clear, and paste. The archive entry picker SHALL list the entries of a zip or tar with more than one entry, filterable by typing, and open the chosen entry.

#### Scenario: Archive entry picker
- **WHEN** the user opens `logs.tar.gz` holding `app.log`, `db.log` and `web.log`
- **THEN** a dialog lists the three entries, typing `db` leaves only `db.log`, and `Enter` opens a stream titled `logs.tar.gz/db.log`.

#### Scenario: Time range by text
- **WHEN** the user presses `t`, types `2026-09-28 14:00` in From and `2026-09-28 14:30` in To, and presses `Enter`
- **THEN** the stream shows only lines timestamped from 14:00 to 14:30, and the bottom border shows the range.

#### Scenario: Unreadable time
- **WHEN** the From field holds `yesterday-ish`
- **THEN** the field is marked as unreadable, `[ OK ]` does nothing, and the stream's range is unchanged.

### Requirement: Terminal Mouse
Mouse capture SHALL be on by default and off with `--no-mouse`. The wheel SHALL scroll the window under the pointer by 3 rows and pause its follow; a left click SHALL focus a window and put the cursor on the clicked row; `Shift`+click or a drag SHALL select a range; a double click SHALL toggle a bookmark; clicks on the stream strip and on a window's top border SHALL show that stream; clicks on dialog buttons SHALL press them, and a click outside an open dialog SHALL close it like `Esc`. The help dialog SHALL say that `Shift`+drag (or `Option` on macOS terminals) selects text natively while the mouse is captured.

#### Scenario: Clicking the second window
- **WHEN** two windows are side by side and the user clicks row 5 of the right one
- **THEN** the right window gets the focused border and its cursor is on its fifth visible row.

#### Scenario: No mouse
- **WHEN** the user runs `fasttail-tui --no-mouse app.log` in conhost with QuickEdit on
- **THEN** clicks select console text natively and the application receives no mouse events.

### Requirement: Terminal Copy
`y`, and `Ctrl+C` when rows are selected, SHALL copy the selection (or the cursor row) as plain text, a collapsed group copying every line it stands for. The text SHALL go to the system clipboard; when no system clipboard is reachable it SHALL be sent as an OSC 52 escape of at most 100,000 bytes, and a longer selection SHALL be refused with a message. `Ctrl+C` with nothing selected SHALL quit as `q` does.

#### Scenario: Copy over SSH
- **WHEN** the user selects 3 rows in an SSH session without a clipboard service and presses `y`
- **THEN** the three lines are sent as one OSC 52 sequence and the status bar says `3 lines copied`.

### Requirement: Shared Configuration, Read-Only
The terminal interface SHALL load `fasttail.ini` (or the file given by `--config`) at start with the same loader as the GUI and SHALL use its theme, language, workspace (open files and patterns in workspace order, and for each stream its filters, search, encoding, ANSI mode, collapse, archive entry, bookmarks and bookmark notes), highlight rules, quick labels, global filter, recent files, recent sessions, search history, `show_line_numbers`, `level_colors` and `sound_enabled`. `--fresh`, `--session` and command-line paths SHALL behave as in the GUI. It SHALL NOT write `fasttail.ini` at any time. When the file cannot be parsed it SHALL retry once after 100 ms and otherwise start with defaults and say so in the status bar.

#### Scenario: Workspace shared with the GUI
- **WHEN** the GUI was closed with `app.log` (include `ERROR`, bookmarks on lines 12 and 40, a note on 40) and `db.log` open, and the user starts `fasttail-tui`
- **THEN** both streams open, `app.log` shows only `ERROR` rows, and `]` walks lines 12 and 40 with the note of 40 in the status bar.

#### Scenario: GUI running at the same time
- **WHEN** the GUI is open with 3 streams and the user starts and quits `fasttail-tui` after opening a fourth file and adding bookmarks
- **THEN** `fasttail.ini` is byte-for-byte unchanged by the terminal interface.

#### Scenario: Italian interface
- **WHEN** `fasttail.ini` holds `language=it`
- **THEN** the status bar, the borders' words and every dialog are in Italian.

### Requirement: Terminal Sessions
`O` and `--session <file>` SHALL load a `*.fasttail-session.ini` session, replacing the open streams. `S` SHALL save the open streams and their state (paths, filters, search, encoding, ANSI mode, collapse, archive entry, bookmarks and notes) to a session file chosen by path, adding `.fasttail-session.ini` when missing, without a dock layout, asking before overwriting an existing file, and refusing the path of the active `fasttail.ini`. When bookmarks or notes changed since start or the last save, `q` SHALL ask "Save session as / Quit / Cancel".

#### Scenario: Saving bookmarks for the GUI
- **WHEN** the user bookmarks line 77 of `app.log` in the terminal, presses `S` and saves as `incident`
- **THEN** `incident.fasttail-session.ini` exists in the current directory, and loading it in the GUI opens `app.log` with a bookmark on line 77.

#### Scenario: Refusing fasttail.ini
- **WHEN** the user types the path of the active `fasttail.ini` in "Save session as"
- **THEN** the dialog says the configuration file cannot be a session and nothing is written.

#### Scenario: Quit with unsaved bookmarks
- **WHEN** the user added a bookmark and presses `q`
- **THEN** a dialog offers "Save session as", "Quit" and "Cancel", and "Quit" exits without writing any file.

### Requirement: Terminal Compatibility and Clean Exit
Colour depth SHALL be truecolor when `COLORTERM` is `truecolor` or `24bit`, `WT_SESSION` is set, or the Windows console accepts virtual-terminal sequences; 256 colours when `TERM` contains `256color`; 16 colours otherwise; `FASTTAIL_TUI_COLORS=16|256|truecolor` SHALL override. Colours SHALL be reduced to the depth by nearest match, and backgrounds and plain text SHALL use the terminal's own colours. On Windows only key presses SHALL act (not releases). Normal exit, `Ctrl+C` quit and a panic SHALL all restore the terminal (mouse capture off, raw mode off, main screen, cursor shown) before anything is printed. When raw mode cannot be entered (for example mintty without winpty), `fasttail-tui` SHALL print that it needs Windows Terminal, cmd or PowerShell, or `winpty`, and exit with code 1.

#### Scenario: Panic restores the terminal
- **WHEN** the terminal interface panics while drawing
- **THEN** the shell gets its normal screen with a visible cursor and echoing keys, and the panic message follows on stderr.

#### Scenario: Plain xterm without COLORTERM
- **WHEN** `TERM=xterm-256color` and `COLORTERM` is unset
- **THEN** the theme's level colours are drawn from the 256-colour palette.

#### Scenario: Git Bash
- **WHEN** the user runs `fasttail-tui.exe app.log` in mintty without winpty
- **THEN** it prints how to run it and exits with code 1, leaving the mintty terminal usable.

### Requirement: Terminal Interface Localization
Every user-visible string of the terminal interface (borders, status bar, help, dialogs, messages, errors) SHALL come from `i18n::t(lang, key)` and SHALL be translated in all 16 languages, covered by the exhaustive translation test. Widths SHALL be measured in terminal cells so that Chinese, Japanese and Korean texts align in dialogs and borders.

#### Scenario: Japanese help dialog
- **WHEN** `language=ja` and the user presses `?` in a 100 x 30 terminal
- **THEN** the help dialog's key column and description column are aligned and no text crosses the dialog border.

### Requirement: Terminal Interface Performance
The terminal interface SHALL poll input with a 100 ms timeout (50 ms while a background job runs), SHALL redraw only when what it shows changed or on input, SHALL read only the visible rows from the engine, and SHALL write each frame to the terminal in one buffered write. On a 10,000,000-line file a full redraw of a 200 x 60 frame SHALL take at most 5 ms at the 95th percentile (widgets and diff, measured on the `TestBackend`), and with that file open and followed but not growing, the process SHALL use at most 1 % of one core.

#### Scenario: Idle cost
- **WHEN** a 1 GB log with 10,000,000 lines is open and followed in the terminal interface and nothing is appended for 12 s
- **THEN** the process uses at most 120 ms of CPU in those 12 s.

### Requirement: Terminal Interface Scope Limits
The terminal interface SHALL open Markdown files as text and SHALL NOT offer a rendered Markdown view. It SHALL NOT offer the timeline histogram, the overview strip, external tools, the screensaver, the window PIN lock, line wrap, filter presets, the highlight-rule editor, Find results across streams, or any renderer, GPU, zoom, font or window setting; the corresponding `fasttail.ini` keys SHALL be left untouched. It SHALL show at most two stream windows at a time.

#### Scenario: Markdown file
- **WHEN** the user opens `README.md` in the terminal interface
- **THEN** its lines are shown as plain text with line numbers, and no key switches to a rendered view.

#### Scenario: PIN lock set in the GUI
- **WHEN** `lock_enabled=true` is in `fasttail.ini`
- **THEN** the terminal interface starts without asking for the PIN, and the key is left as it was.
