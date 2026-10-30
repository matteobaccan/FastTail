# FastTail in a terminal

`fasttail-tui` is FastTail's terminal interface: the same engine, the same `fasttail.ini`
(workspace, dock layout, sessions, bookmarks, filters, highlight rules, theme and
language) and most of the window's features, drawn in a terminal (see [Limits](#limits)
for what only the window has). Use it over SSH, on a
server without a desktop, or in Windows Terminal. The window and the terminal can take
turns on one configuration; each writes back only what it changed, so the last writer
wins.

## Starting

```bash
fasttail-tui                          # the saved workspace
fasttail-tui app.log err.log          # two streams, as tabs of one window
fasttail-tui --split app.log err.log  # side by side
fasttail-tui app.log:1204             # cursor on line 1204 (also --line 1204)
some-command | fasttail-tui -         # standard input
fasttail-tui --session incident.fasttail-session.ini
fasttail-tui --ascii --no-mouse app.log
```

`fasttail-tui` takes every option of `fasttail` (`--since`, `--until`, `--filter`,
`--follow`, `--fresh`, `--print`, ...); `--renderer` is accepted and ignored.

- **Windows.** The download holds `fasttail.exe` (the window) and `fasttail-tui.exe`.
  From cmd, PowerShell or Windows Terminal run `fasttail-tui.exe` directly.
  `fasttail.exe --tui` (or `interface=tui` in `fasttail.ini`, a double click included)
  starts `fasttail-tui.exe` in a new console window; standard input cannot follow it
  there, so `command | fasttail.exe --tui` is refused and points to `fasttail-tui.exe`.
- **Linux and macOS.** One executable has both interfaces: `fasttail --tui`, or
  `interface=tui`, runs the terminal interface in the same process when there is a
  terminal. Linux also has a *terminal only* archive per architecture
  (`fasttail-tui-linux-<arch>-<version>.tar.gz`): `fasttail-tui` built without the
  window, for servers with no graphical libraries; its debug info is in the matching
  `-symbols-` archive.
- **Switching.** Settings > *Interface at start* chooses the interface; picking the other
  one offers *Switch now*, which saves the workspace and reopens it there.
  `fasttail-tui --gui` starts the window and exits.

At least 40 columns by 10 rows are needed; below that a line asks for a larger terminal.

## The screen

- **Top bar**, as the window's title bar and toolbar: the version, the author and the
  session; the CPU and RAM meters (Settings > *CPU and RAM in the top bar*), the global
  filter state and the command palette; then Open, Sessions, Rules, Play / Pause of every
  stream, Settings, About and Help (the second row from 16 rows up). Every `[ ]` is a
  button.
- **Windows.** Each stream is a bordered window, as the window's dock lays them out (the
  same `[dock] layout` of `fasttail.ini` and session files): its title, FOLLOW or PAUSED
  and `[x]` in the top border, the line counts and the filter state in the bottom one.
  After the counts, a text window shows the cursor's file line, `Ln 1,234` (the number
  the gutter gives it, whatever the filters; a click opens Go to), and while the rows are
  scrolled sideways the first column drawn, `Col 41`; a narrow window leaves out `Col`,
  then `Ln`. HEX and ASM show their byte counts instead.
  Inside, the window's two rows: the **stream bar** (the `[x] Follow` and
  `[x] Auto-update` check boxes, TXT / HEX, line numbers, `[x] Δt` (the time delta column:
  the gap to the row above, in the accent colour from the *Time delta gap* setting; `Alt+D`),
  `🌐` the time display (as written, UTC, local time: a click moves to the next), encoding, ANSI mode, collapse, context lines, the time span, which opens the
  time range, and the line counts) and the **filter row** (include, exclude, `Aa`, `.*`,
  minimum level, `[≤96 B]`, where the level is looked for (a click sets the stream's own
  value, 16 to 4096 bytes; empty goes back to the setting), presets, and the global
  filter when it applies). A small window keeps
  only the stream bar, or neither.
- **Status bar** at the bottom: the stream commands as buttons, and messages.
- **Tab strip** with two or more streams; `+` marks a stream that is not on screen and
  received lines.

## Keys

`?` or `F1` lists every key as a menu (`Enter` runs one); `:` opens the command palette
(the window's commands by name, with their keys; a number goes to that line). Every
command of the window is there except the ones a terminal cannot show (zoom, always on
top, wrap, the Markdown view, the timeline strip): among them *Select all* (`Ctrl+A`),
*Play all* / *Pause all*, *Line numbers*, *Auto-update*, *Remove bookmark*, *Save
session* (the current one; without one it asks for a file), *Save as default session*,
*Clear recent sessions*, *Open folder pattern…* (the Open dialog, which takes
`logs/*.log`) and *Recent files…*: a list where `Enter` opens a file, `Delete` removes it
and `c` clears the list (also *Clear recent files*).

| Keys | Action |
|---|---|
| `↑` `↓` / `j` `k`, `PgUp` `PgDn` (`Ctrl+B` `Ctrl+F`) | cursor row, page; a page key held down moves further: 2 pages a press after a second, 4 after two, 8 after three (`page_acceleration=false` in `[general]` of `fasttail.ini` turns it off) |
| `Home` `g` / `End` `G` | first row / last row and follow |
| `←` `→`, `Ctrl+←` `Ctrl+→`, `0` | sideways by 1, by 10, back to column 0 (`Col N` in the bottom border while scrolled) |
| `Shift+↑` `Shift+↓` | extend the selection |
| `Space` | follow on / off |
| `/`, `n` `N` (`F3` `Shift+F3`), `Esc` | search (`re:` before the query makes it a regular expression, `mre:` a multi-line one over the visible lines joined with `\n`, `(?-i)` case-sensitive), next / previous hit, clear |
| `Alt+/` (`Ctrl+Shift+F`) | search all streams: a Find results window with the focused stream's query; `Enter` searches every open stream (four background jobs at a time) under each stream's own filters and the global filter, HEX and ASM streams skipped. The results are grouped by stream (`▼ api.log  12 matches`, the progress while it runs, `stale: press r` after a reload); arrows, `PgUp` `PgDn`, `Home` `End` walk them, `Space` or `←` `→` fold a stream, `Enter` focuses that stream on the line with follow paused and its own search unchanged (`Ctrl+K`: in context), and hides the window (the key shows it again); `r` searches again, `s` stops, `Tab` or `/` goes back to the query, `Esc` closes and cancels. Up to 100,000 hits per stream, the true total counted; below 60 columns the line numbers are left out |
| `i`, `x`, `l`, `c`, `a`, `#` | include, exclude, minimum level, collapse, ANSI mode, line numbers |
| `H` | hide the selected lines (or the cursor row); `[🙈 N hidden]` in the filter row lists them: `Enter` shows a range again, `a` all |
| palette | *Split file…*: asks `100000` (lines), `10mb`, `1h`, `1d` or `30m`; the pieces go next to the file (`app.part-001.log`, `app.2026-10-05T14.log`), the progress and the outcome in the message line; existing pieces are kept unless the answer ends with `!` |
| Windows | `dbwin://local` (or *Capture debug output…* in the palette, with no stream open too) follows the `OutputDebugString` messages; the filter row shows `N debug messages · N dropped` |
| any | `--listen syslog://127.0.0.1:5514` (or `tcp://`, `udp://` URLs, or *Listen on a port…* in the palette, with no stream open too) receives lines on a local port; Settings has the three listener limits; the filter row shows `listening on … · N connections · N dropped` |
| any | *Paste as stream* in the palette (works with no stream open) opens the system clipboard's text as a new stream `clipboard 14:02`, spooled like standard input and not kept by the workspace; *Append the clipboard to this stream* adds the next paste at the end of the focused clipboard stream; without a readable clipboard (over SSH) the status line says so |
| any | `ssh://user@host/var/log/app.log` on the command line, typed in the Open dialog or given to *Open remote file (SSH)…* in the palette (which works with no stream open) follows a remote file; Settings has *SSH client*, *Remote files: first fetch* and *Remote files: spool*; the filter row shows `[following]`, `[reconnecting (n)]` or `[disconnected: … · Reconnect]`, a click on the last one (or *Reconnect source* in the palette) connects again |
| palette | *Open system source…*: asks for `docker://<container>`, `k8s://<namespace>/<pod>` (`/<container>`, `deploy/<name>`, `<context>@`), `journald://<unit>` or `journald://user/<unit>` (works with no stream open; the Open dialog accepts a typed URI too); an ended source shows `[⏹ ended (exit N) · Reconnect]` in the filter row, a click (or *Reconnect source* in the palette) runs it again from the last timestamp |
| palette | *Next / Previous line out of order*: a line whose time is earlier than the line above's shows `<` after its number, and `[↶ N out of order]` in the filter row goes to the next one |
| `C` | column view of the fields (JSON, logfmt, regex presets, CSV / TSV) on or off, also `[x] Columns` in the stream bar: a header row of the field names over one row per line, each field as wide as its widest cell on screen (at most 40 cells), then the message; a line that does not parse runs across, dimmed. `[Fields...]` lists every known field: `Space` shows or hides one, `Shift+Up/Down` moves it, `r` resets, `f` pins or unpins the selected column (marked `📌`, `^` with ASCII borders; pinned columns come first and stay at the left while `Left` / `Right` scroll the others; the time column is pinned by default, as in the window); `p` plots the selected numeric field as a sparkline row at the bottom of the window (`📈 ms avg ▁▂▅█▃ 10 - 500`, one block per column over the time its values cover; the label goes on narrow windows; `p` again removes it) and `a` cycles avg / max / min / p95. Saved in the same `fields_view` / `fields_columns` and `plot=` keys as the window, which draws up to three plotted fields; the terminal draws the first |
| `t` | time range (fields, calendar, time) |
| `e` `E`, `w` `W` (`10j`, `3e`: counts) | next / previous ERROR, WARN line |
| `b` (`Ctrl+F2`), `]` `[` (`F2` `Shift+F2`), `m` | bookmark, next / previous, note |
| `Ctrl+K` | the row in context (filters off) and back |
| `R` | when the stream bar offers `R: include N rotations`: open the stream again with its file's rotations (`app.log.1`, `.2.gz` …) as one stream, oldest first; on a stream read with its rotations (`+N rotations (R: pieces)`), the list of its pieces: `Enter` goes to a piece's first line. The first line of each piece after the first has `~` in the gutter |
| `Ctrl+O` | open the cursor row's source location (`Orders.java:42`, `src/x.rs:3:9`) in the editor of Settings; pressed again on the same row, the next location |
| `Alt+O` | open the cursor row's source location in FastTail itself, as a stream at that line |
| `Alt+A` | the analysis views of the focused stream, over the windows, from the window's own background jobs: **Statistics of a field** (a field, or `/regex/` for its first group: the most frequent values with count, share, a bar and their ERROR / WARN lines, the numeric summary min / p50 / p95 / p99 / max / mean in the field's unit; on a timed stream a sparkline per value of its count over time; `Enter` includes the value as a filter term, `x` excludes it; also `S` in `[Fields...]`), **Query** (the window's pipeline typed at the top, `Enter` runs it, the result as a table; `Enter` on a row result goes to its line), **Patterns** (the message templates most frequent first, each with its volume over time as a sparkline; `Enter` shows only that pattern's entries, `x` hides them; also in the palette), **Explain a span** (`14:00..14:05` against the rest of the log: the messages and `field=value` pairs over-represented in the span with their ratio; `Enter` goes to a message's first line or filters on a pair) and **Operations** (grouped by a field: id, name, start, duration, lines, worst level and a text Gantt bar; `Enter` goes to the first line; also `O` in `[Fields...]`). Every table sorts by a column with `s` (`S` reverses) and narrows with `/` |
| `Alt+V` | the volume row above the focused stream's rows: the window's timeline histogram in one row (lines per time slice, a block per column scaled to the busiest, in the ERROR or WARN colour where such lines are); a click on a cell goes to that time. Shared with the window's timeline strip (`timeline=` per stream); also *Timeline* in the palette |
| `Alt+Y` | the selected rows (or the cursor row) as an excerpt for a ticket: a header with the stream, the lines and the time span, then the rows in a Markdown, Jira or Slack code block (Settings: *Copy as excerpt for*) |
| `Alt+I` | the focused stream's information: path, source kind, size, lines and visible lines, how long the index took, the memory of the index and of the block cache, the encoding, the field parser and whether it has timestamps (also *Stream information* in the palette) |
| `Alt+.` `Alt+,` | the next / previous line of the cursor row's operation (its `trace_id`, `request_id`, `req_id`, `opid`, `correlation_id` or `thread` field, or the Operations view's source when it is open) |
| `Alt+K` | keep the search typed (`/`): it stays highlighted in its own colour with the other kept searches (up to 8), a chip `timeout 12` per search in the filter row (a click walks only that one, again: all), and with no search typed `n` / `N` go to the next or previous line holding any of them; `Alt+K` with no search typed forgets the focused kept search (else the last). Saved per stream, shared with the window |
| `Alt+M` | a merged view: the open streams with timestamps as check boxes (`Space`, the focused one checked), `Enter` opens one stream window of their lines interleaved by time, each row tagged `[app.log]` in its source's colour (a coloured mark only in windows under 60 cells) and numbered by its source line; it follows the sources live and is saved in the workspace and sessions as the window's merged views are (the window's own merged views open in the terminal too). On a merged view `Alt+M` lists its sources: `Space` leaves one out or takes it back. Also *New merged view…* in the palette |
| `Alt+T` | the focused stream's filter (include / exclude terms, level, time range, hidden lines) as a new tab of the same file, as the window's *Open filter as new tab*: the filter is frozen, the tab follows the source and numbers its rows by their source lines, `Ctrl+K` on a row shows that line in the source; saved in the workspace and sessions, and the window's filter tabs open in the terminal. Also in the palette |
| `=` | compare: two selected rows side by side at once; otherwise the selection (or the cursor row) is marked, and `=` on another selection (in any stream) compares the two regions. The window aligns the lines as the window's Compare tab does, marks the differing tokens, draws removed lines in the error colour and added ones in the info colour; `t` `u` `i` `w` `c` `J` ignore the time, numbers, ids, spaces, case or compare as JSON; `]` / `[` go to the next / previous change, `y` copies the compare as a unified diff, `Esc` closes |
| `Ctrl+N`, `Alt+N` | send the selected rows (or the cursor row) to the session's scratchpad under a `── app.log:1204 ──` reference line; show the scratchpad: an editor over the same file the window uses (`scratchpad.txt` beside `fasttail.ini`, or beside the session), saved a second after a change and on exit; `Ctrl+G` goes to the reference line at or above the cursor, `Ctrl+Z` undoes the last change, `Esc` closes |
| `I` | the include side as an expression (`ƒx` in the filter row; `i` then edits the expression) or back to terms |
| `Ctrl+G` | go to a line (`N`, `+N`, `-N`) or a time (`14:02`) |
| `h` | HEX view (go to `1024` or `0x400`) and back |
| `d`, `Shift+D` | ASM view of x86 code (go to an offset, `0x401000` or `entry`) and back to HEX; the next architecture (16, 32, 64 bit), shown in the window's bottom border |
| `J` | the cursor row's JSON as a tree (`y` value, `Y` path) |
| `y` (`Ctrl+C`), `Y` | copy the selection or the cursor row; as shown (the column view's cells separated by tabs, a closed group with its badge) |
| `Alt+R` | the search results as a list (line number and text in the level colour); `Enter` goes to the hit, `PgUp` `PgDn` `Home` `End` move |
| `Alt+S` | search in a range: lines `1200-5000` (`1200-`, `-5000`, `1200`) or times `14:00..15:30` (an open side allowed); empty searches everywhere, and the counter says `in range` |
| `Alt+E` | export the visible lines, the selected lines or the search matches as text, CSV (the window's separator, BOM and formula protection) or HTML (the theme's colours, at most 200,000 lines): the file is asked for (next to the stream's file, `app-matches.csv`), a trailing `!` replaces an existing one, the progress shows in the status line |
| `s` `\|` `_` | new window beside / below |
| `<` `>`, `Ctrl+PgUp` `Ctrl+PgDn` | move the stream to another window, tabs of a window |
| `Alt+D` | the time delta column on or off |
| `@` | the time anchor on the cursor row (again on the same row: off; also *Clear time anchor* in the palette): the Δt column, shown with it, measures every row from that line (`-0.500`, `+1.250`) and marks it `@`. A selection of timed rows (`Shift+Up` / `Down`) shows its elapsed time at the bottom of the status bar, `Δ +2.357 · 14 rows` |
| `Alt+F`, `Alt+X`, `Alt+arrows` | float / dock a window, close a window, move a divider or a floating window |
| `Tab`, `Alt+1..9` | next window or stream, stream N |
| `Ctrl+W` | close the stream |
| `o`, `O`, `S` | open a file (folder browser), load / save a session |
| `,`, `r`, `p`, `F` `f`, `!` | Settings, highlight rules, presets (pinned first, `*` pins, then the five applied last, then by name), global filter (edit / on-off), external tools |
| `Ctrl+Shift+1..9` (or `L` and a digit) | the search text as a quick label |
| `Shift+T` | next theme |
| `Ctrl+L` | lock with the PIN |
| `q` | quit |

Where the window has a shortcut, the terminal accepts it too; every action also has a
plain key, because terminals and multiplexers swallow some modifier combinations
(`Ctrl+B` in tmux, `Ctrl+Shift` letters in conhost). The Kitty keyboard protocol is used
when the terminal supports it, so `Ctrl+Shift` combinations are told apart.

## Mouse

On by default (`--no-mouse` turns it off). A click focuses a window, puts the cursor on a
row and shows a tab; `Shift`+click or a drag selects; a double click toggles a bookmark;
the wheel scrolls the window under the pointer. Drag a divider to resize; drag a title
onto a window's edge to split it, onto its centre to add a tab, anywhere else to float it.
A floating window moves by its title and resizes from any corner, its bottom row or its
side borders. Settings and Keys move and resize the same way, show `[x]` in their top
border (it closes them as `Esc`; Settings brings back the values it opened with) and
open again where they were left while the interface runs; a double click on their
title centres them at the default size. Every bar chip, button, list item and dialog
button can be clicked; a click outside a dialog closes it, a drag released outside does
not. While the mouse is captured, `Shift`+drag (`Option` on macOS
terminals) selects text natively.

## Remapping the keys

The keys above are the defaults. *Keyboard shortcuts* in the palette opens the **Keys**
dialog: every remappable action by its name (`follow`, `search.next`, `bookmark`, ...) and
its keys, `*` beside the ones changed. `Enter` records the next key pressed for the action
(a key another action had is taken from it, which keeps its other keys), `d` leaves the
action without a key, `r` brings its default keys back, `R` every default. The keys are
saved in `fasttail.ini` under `[shortcuts]` with a `tui.` prefix, one line per action
changed (`tui.follow=f`, `tui.search.next=n, F3`, an empty value for no key); the window
keeps its own keys there under the registry's names, and an entry that does not read is
named in the dialog. The arrows, `PgUp` / `PgDn`, `Tab`, `Alt+1..9`, the count digits and
the keys inside dialogs stay as they are.

The palette also lists the window's settings that apply to a terminal (line numbers and the
time delta column in new streams, level colours, automatic highlighting, sound, the bell on
background alerts, the PIN lock, telemetry): choosing one switches it and saves it.

## Settings, editors, tools

- **Settings** (`,`): interface, theme, language (or the system's), view, automatic token
  highlighting, performance and refresh, new-stream defaults (with the bytes the level
  is looked for in), bookmarks, sound and the
  bell on background alerts, desktop notifications (and whether a stream on screen
  notifies too), external tools, PIN lock. Changes apply at once; `[ OK ]`
  saves, `Esc` brings back the values Settings opened with.
- **Highlight rules** (`r`): every field of the window's rules (the *Desktop notification*
  box included: a stream not on screen notifies a matching appended line), order, colours,
  quick labels; `i` imports a `*.fasttail-rules.ini` rule set (or a Notepad++ AnalysePlugin / UDL `.xml`) (the rules already there are
  skipped; a trailing `!` replaces every rule) and `x` exports the rules. `F4` / `Shift+F4`
  go to the next / previous line of the rule matching the cursor row (wrapping round, in
  the background on a large file). **Presets** (`p`), **global filter** (`F`: its
  *Expression* box turns the include rows into one expression, `a AND (b OR c)`, built from
  the terms the first time; an expression that does not compile says why and the last valid
  one filters), **external tools** (`!`, with their shortcuts and rule-bound runs, and their
  editor in Settings).
- **Log formats** (palette): the formats (built-in ones locked, plugin ones named by their
  plugin) and the format files that did not load with their error. `Enter` edits a format
  as a form (name, up to 8 regexes, time field, format and zone, level field and map,
  message field, units, entry start, files, columns) with what it reads from the focused
  stream's first 200 lines (`Reads 92% of 180 entries`, the lines marked `+` / `-`); `Enter`
  saves and the streams read with the new version, `Esc` goes back. `n` new, `c` copy (a
  built-in format is changed through a copy), `d` delete, `i` import, `x` export: the same
  files as the window's dialog.
- **Editor preset** (Settings): VS Code, JetBrains, Sublime Text, Notepad++, Vim, Emacs fill
  the editor command `Ctrl+O` runs.
- **Lock** (`Ctrl+L`, and after the idle minutes when the lock is on): a PIN dialog on a
  blank screen. The lock deters onlookers; it is not security.

## Web view, AI assistants and plugins

- **Web view** and **MCP server**: Settings > *Web view and AI assistants* turns on the
  read-only web view of the open streams (`[web]`, its port) and the MCP server for AI
  assistants (`[mcp]`, its port, whether assistants may change filters and bookmarks).
  The terminal serves its own streams with the window's servers, the same pages and tools;
  the status bar's bottom border says `web :8080 · MCP :7777` while they listen, and a
  port taken by another interface is reported there. The token and the link are the
  window's (`fasttail.ini`).
- **Plugins** (Settings > *Plugins*, or a key given in the Keys dialog): the plugins of the
  `plugins` folder with their kind, version and state, and the folders that did not load.
  `Space` enables or disables one; a source plugin asks first, showing the SHA-256 of its
  manifest (`y` runs its programs). `o` opens an enabled source plugin as a stream: the
  prompt holds `plugin://name?key=value` with the defaults, secrets typed there are given
  to the program and never saved. `r` reads the folder again.

## Languages

The terminal speaks the language of `fasttail.ini` (Settings > Language), or the system's
with *Follow the system language*; a change shows at once. Messages printed before the
interface starts (the hand-offs, a missing terminal) follow the system's language; the
command-line usage stays in English.

## Terminals

- **Colours.** Truecolor when `COLORTERM` says so, in Windows Terminal (`WT_SESSION`) or
  in a Windows console with virtual-terminal support; the xterm 256-colour cube when
  `TERM` holds `256color`; otherwise the 16 basic colours (the terminal's own background
  and text). `FASTTAIL_TUI_COLORS=16|256|truecolor` overrides the detection.
- **ASCII.** `--ascii`, `FASTTAIL_TUI_ASCII`, or a legacy console without virtual-terminal
  support draws the borders with `+ - |` (the focused window with `=`).
- **Windows Terminal and conhost** both work. In conhost QuickEdit is switched off while
  the mouse is captured and restored at exit; `--no-mouse` keeps it.
- **Git Bash (mintty)** is not a console: run `fasttail-tui` in Windows Terminal, cmd or
  PowerShell, or through `winpty fasttail-tui`.
- **Clipboard.** `y` uses the system clipboard; over SSH the text goes through OSC 52,
  which the terminal must allow.

## Limits

Not in the terminal: line wrap, the overview strip and the timeline histogram (one plotted
field is drawn as a sparkline row), the time scope of Find results, the Markdown view, the
screensaver, and the window-only settings (renderer, zoom, font, always-on-top, borderless,
tray). Filter tabs and merged views made in the window are kept in the workspace but not
shown.

Only in the window for now: the charts of the analysis tabs (the terminal draws tables and
text bars).

## Appendix: measurements

From the feasibility spike (`docs/tui-feasibility.md`, a 1.04 GB log with 10 million lines
on Windows 11): a frame at 200 × 60 takes 1–2 ms to lay out (p95 1.7 ms), under 1 ms to
write in Windows Terminal or conhost; idle CPU with the file open and following is about
0.3 % of a core. `frame_time_at_200_by_60` (ignored by default) keeps the p95 at 5 ms or
less: `cargo test --release --no-default-features --features tui --lib frame_time -- --ignored`.
