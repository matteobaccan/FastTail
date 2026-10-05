# FastTail in a terminal

`fasttail-tui` is FastTail's terminal interface: the same engine, the same `fasttail.ini`
(workspace, dock layout, sessions, bookmarks, filters, highlight rules, theme and
language) and the same features as the window, drawn in a terminal. Use it over SSH, on a
server without a desktop, or in Windows Terminal. The window and the terminal can take
turns on one configuration; each writes back only what it changed, so the last writer
wins.

## Starting

```bash
fasttail-tui                          # the saved workspace
fasttail-tui app.log err.log          # two streams, as tabs of one window
fasttail-tui --split app.log err.log  # side by side
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
  Inside, the window's two rows: the **stream bar** (follow, monitor, TXT / HEX, line
  numbers, encoding, ANSI mode, collapse, context lines, the time span, which opens the
  time range, and the line counts) and the **filter row** (include, exclude, `Aa`, `.*`,
  minimum level, presets, and the global filter when it applies). A small window keeps
  only the stream bar, or neither.
- **Status bar** at the bottom: the stream commands as buttons, and messages.
- **Tab strip** with two or more streams; `+` marks a stream that is not on screen and
  received lines.

## Keys

`?` or `F1` lists every key as a menu (`Enter` runs one); `:` opens the command palette
(the window's commands by name, with their keys; a number goes to that line).

| Keys | Action |
|---|---|
| `↑` `↓` / `j` `k`, `PgUp` `PgDn` (`Ctrl+B` `Ctrl+F`) | cursor row, page |
| `Home` `g` / `End` `G` | first row / last row and follow |
| `←` `→`, `Ctrl+←` `Ctrl+→`, `0` | sideways by 1, by 10, back to column 0 |
| `Shift+↑` `Shift+↓` | extend the selection |
| `Space` | follow on / off |
| `/`, `n` `N` (`F3` `Shift+F3`), `Esc` | search, next / previous hit, clear |
| `i`, `x`, `l`, `c`, `a`, `#` | include, exclude, minimum level, collapse, ANSI mode, line numbers |
| `t` | time range (fields, calendar, time) |
| `e` `E`, `w` `W` (`10j`, `3e`: counts) | next / previous ERROR, WARN line |
| `b` (`Ctrl+F2`), `]` `[` (`F2` `Shift+F2`), `m` | bookmark, next / previous, note |
| `Ctrl+K` | the row in context (filters off) and back |
| `Ctrl+G` | go to a line (`N`, `+N`, `-N`) or a time (`14:02`) |
| `h` | HEX view (go to `1024` or `0x400`) and back |
| `J` | the cursor row's JSON as a tree (`y` value, `Y` path) |
| `y` (`Ctrl+C`) | copy the selection or the cursor row |
| `s` `\|` `_` | new window beside / below |
| `<` `>`, `Ctrl+PgUp` `Ctrl+PgDn` | move the stream to another window, tabs of a window |
| `Alt+F`, `Alt+X`, `Alt+arrows` | float / dock a window, close a window, move a divider or a floating window |
| `Tab`, `Alt+1..9` | next window or stream, stream N |
| `Ctrl+W` | close the stream |
| `o`, `O`, `S` | open a file (folder browser), load / save a session |
| `,`, `r`, `p`, `F` `f`, `!` | Settings, highlight rules, presets, global filter (edit / on-off), external tools |
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
side borders. Every bar chip, button, list item and dialog button can be clicked; a click
outside a dialog closes it. While the mouse is captured, `Shift`+drag (`Option` on macOS
terminals) selects text natively.

## Settings, editors, tools

- **Settings** (`,`): interface, theme, language (or the system's), view, automatic token
  highlighting, performance and refresh, new-stream defaults, bookmarks, sound and the
  bell on background alerts, external tools, PIN lock. Changes apply at once; `[ OK ]`
  saves, `Esc` brings back the values Settings opened with.
- **Highlight rules** (`r`): every field of the window's rules, order, colours, quick
  labels. **Presets** (`p`), **global filter** (`F`), **external tools** (`!`, with their
  shortcuts and rule-bound runs, and their editor in Settings).
- **Lock** (`Ctrl+L`, and after the idle minutes when the lock is on): a PIN dialog on a
  blank screen. The lock deters onlookers; it is not security.

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

Not in the terminal: line wrap, the overview strip and the timeline histogram, Find results
across streams (`Ctrl+Shift+F` in the window), the Markdown view, the screensaver, and the
window-only settings (renderer, zoom, font, always-on-top, borderless, tray). Filter tabs
made in the window are kept in the workspace but not shown.

## Appendix: measurements

From the feasibility spike (`docs/tui-feasibility.md`, a 1.04 GB log with 10 million lines
on Windows 11): a frame at 200 × 60 takes 1–2 ms to lay out (p95 1.7 ms), under 1 ms to
write in Windows Terminal or conhost; idle CPU with the file open and following is about
0.3 % of a core. `frame_time_at_200_by_60` (ignored by default) keeps the p95 at 5 ms or
less: `cargo test --release --no-default-features --features tui --lib frame_time -- --ignored`.
