<div align="center">
  <img src="assets/logo.svg" alt="FastTail Logo" width="700" />

  <p><strong>Next-generation ultra-fast multi-stream log monitor & tail viewer with Cyberpunk UI aesthetic.</strong></p>

  <p>
    <a href="https://github.com/matteobaccan/FastTail/actions/workflows/build.yml"><img src="https://github.com/matteobaccan/FastTail/actions/workflows/build.yml/badge.svg" alt="Build Status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="MIT License" /></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/language-Rust-orange.svg?style=flat-square" alt="Rust 2021" /></a>
    <a href="https://github.com/matteobaccan/FastTail/releases"><img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platforms" /></a>
  </p>

  <p>
    <img src="assets/screenshots/fasttail_main_view.png" alt="FastTail Main Interface" width="850" />
  </p>
</div>

---

## ⚡ Overview

**FastTail** is a modern, high-performance cross-platform log tailing application written in Rust. Designed as the successor to legacy tools like BareTail, it combines a **zero-lag 64-bit streaming engine** with a **Cyberpunk UI**, rich log intelligence, audio alert presets and a modular docking workspace.

Whether you are monitoring multi-gigabyte production logs, inspecting raw binary streams in Hex mode, reading Markdown or HTML reports, or isolating errors with priority-ordered highlight rules, FastTail stays instantly responsive with a minimal memory footprint.

---

## 🚀 Feature Highlights

- **Zero-Lag Streaming Engine**: never holds the file in memory. Rows are read on demand through a 4 MB block cache per stream; the per-file state is the line index (8 bytes per line) plus, once scanned, a level byte and a timestamp per line and the search hits (at most 1,000,000, 8 MB). Appends are indexed incrementally, log rotation, truncation and in-place rewrites are detected without locking the file for the writer, and files above 16 MB run filters, search and the timestamp scan of the time range on a worker thread with progress shown in the stream bar (above 256 MB the initial index too). Ten 50 MB logs growing continuously cost about 60 MB of RAM and a few milliseconds per frame.
- **Named Sessions**: save the open streams, filters, layout and bookmarks under a name and switch between projects in one click; session files keep relative paths so a log bundle can move with its session.
- **Three View Modes per Stream**:
  - **TXT**: virtualized text view with highlight rules, inline JSON pretty-printing and stack-trace grouping.
  - **HEX**: live hexadecimal + ASCII dump with byte columns in multiples of 8 (16, 24, 32...).
  - **MD**: rendered Markdown. `.md` files open in this mode automatically and HTML documents are converted to Markdown on the fly.
- **Powerful Search**: a search box per stream, `F3` / `Shift+F3` navigation scoped to the focused window, match counter, wrap-around beep, last 10 queries history, matches refreshed live as the file grows. In HEX mode the query is matched at byte level (text or `0A 0D` patterns), and a marker column (`▶` current hit, `●` other hits) plus full-row highlight is shown in every view. The `☰` button opens a **search results pane** under the rows listing only the matching lines (line number, text, query tinted), virtualized so a million hits cost what ten do; a click or `Enter` makes a hit current and centres it in the main view. Up to 1,000,000 hits are listed per stream; past that the search keeps counting and the counter shows the true total with a "first 1,000,000 listed" note.
- **Search All Streams**: `Ctrl+Shift+F` (or the `🔎` button next to a stream's search box) opens a **Find results** tab with the focused stream's query; `Enter` searches every open stream with the same case-insensitive match as the per-stream search, over the lines each stream shows under its own include/exclude, level and time filters (ANSI codes stripped where the stream strips them; streams in HEX view are skipped). Each stream is searched by its own background job, independent of the stream's own scans, at most four at a time (fewer on smaller machines), the others queued. The results are grouped by stream (name, match count, progress, collapsible) in one virtualized list; a click or `Enter` activates that stream's tab, centres the line and pauses follow without touching the stream's own search. Up to 100,000 hits are listed per stream, with the true total counted. Results are a snapshot: Refresh runs the query again, Stop or closing the tab cancels it, and a stream reloaded since (truncated, rotated, rewritten) is marked stale instead of jumping to the wrong line. The tab is not saved in the layout.
- **Overview Strip**: a narrow column beside the scroll bar marks where the search hits, the bookmarks and the ERROR/FATAL lines sit among the visible rows, with the viewport drawn as a box; click or drag it to jump there. Hit marks cover the listed hits (the first 1,000,000) and bookmark marks are exact; error marks appear as the level scan reaches the lines and are exact, except on a filtered view of more than 4 million rows, where they are sampled (the tooltip says so). Switchable off in Settings.
- **Live Include / Exclude Filters**: plain text or regex, case-sensitive or not, applied as you type. Filtered rows are virtualized so the viewport is always full. Up to 8 include terms (a line must contain **all** of them) and 8 exclude terms (it must contain **none**) per stream, and named **filter presets** that bring a whole setup back in two clicks, on one stream or all of them.
- **Multi-Rule Highlighting**: foreground, background, **bold**, *italic*, top-down priority (reorder with ⬆ / ⬇), and a sound alert preset per rule (Beep, Chime, Warning, Critical).
- **Capture-Group Highlighting and Quick Labels**: a regex rule can paint only its capture groups (`req=(\d+)` colours the request id, not the row) with "Captures only"; the first rule wins per byte, top-down, and at most 64 spans are painted per row. `Ctrl+Shift+1..9` turns the current search text into a quick colour label with preset colour 1..9, painted in every stream and listed in a strip above the rows with a remove button; labels rank below the rules and are not saved across restarts.
- **ANSI Colour Codes**: container logs (`docker logs`, `kubectl logs`), CI job logs and colourised CLI output show their colours instead of `[32m` fragments. A stream that contains SGR escape sequences switches to **render** by itself (checked on the first 64 KB and on appended data, so colours that start after a banner are caught): 16, 256 and 24-bit colours, bold, dim, italic, underline and inverse are painted with a palette tuned for each theme, below your highlight rules and quick labels and above the level colouring. The `ANSI` selector in the stream bar also offers **strip** (codes hidden, no colour) and **raw** (the codes shown as `␛[31m`), saved per file. In render and strip modes filters, search, rules, level and timestamp detection, copy, export and the `{line}` of external tools all see the text without the codes, so `ESC[31mERROR` is an ERROR line and `\bERROR\b` matches it; HEX still shows the file bytes. A regex written against the codes themselves (`\x1b\[31m`) works in raw mode. A log without escape bytes stays in auto and is read exactly as before.
- **Cyberpunk Themes**: **Tron** (obsidian & neon cyan), **Matrix** (phosphor green), **Blade** (charcoal & amber/magenta) and a clean **Light** theme.
- **Modular Docking Workspace** (egui_dock): dock, split, float or tab any number of streams. Layout, floating window positions, dialog positions and window geometry are persisted and restored.
- **Multi-Encoding Support**: automatic detection and manual override for ASCII, ANSI (Windows-1252), UTF-8 (with/without BOM), UTF-16 LE and UTF-16 BE.
- **Log Intelligence**: inline `[+] JSON` detection with pretty-printing, multiline stack-trace continuation kept together with its parent line.
- **Log Level Detection**: the level of every line (`FATAL`, `ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`, plus syslog `<n>` priorities) is detected from the common layouts without configuration. Rows are coloured by level when no highlight rule matches them (switchable in Settings), a `≥ level` selector above the buffer filters by minimum level together with the include / exclude filters, and the stream bar counts the lines per level live.
- **Directory Wildcard Tail**: open `C:\logspp-*.log` (type it in the `📂*` prompt, drop a folder on the window, or pass it on the command line) and the stream follows the newest file matching the pattern, switching by itself when the logger rotates to a new day or hour. Filters, highlight rules, search and wrap survive the switch; the stream bar shows the pattern, the current file and a "switched to" notice. The pattern, not the resolved file, is what the workspace and the recent list remember.
- **Compressed Logs**: rotated `app.log.1.gz` files and `.zip` support bundles open directly, recognised by their content rather than their extension. The archive is decompressed on a background thread into a temporary spool file that the normal engine reads, so filters, search, levels, time range, bookmarks, HEX and export all work and nothing decompressed is held in memory; the first lines appear while the rest is still inflating, with `decompressing N%` and a cancel button in the stream bar. A zip with several files opens an entry picker (filter, sort by name or size, multi-select), each entry in its own stream. A free-space check and a configurable output cap stop runaway archives, and the spool is deleted when the tab closes.
- **Time range**: a "from / to" pair above the buffer keeps only the lines stamped inside the window, with the entry's stack trace travelling with it; `Ctrl+G` takes `14:02` as readily as a line number, and the status bar shows the span you are looking at. Timestamps are read from the line itself (ISO 8601, syslog, Apache/nginx, epoch seconds or millis) with no format to configure; a log FastTail cannot time says so instead of hiding everything.
- **Time deltas**: the `Δt` button next to `# 123` adds a column with the time since the previous visible row (`+0.125`, `+4:05.120`), so a stall stands out, tinted in the accent colour from 1 s up; it follows the filters, so with `payment` typed it reads the time between payment lines. "Set time anchor here" in the row menu measures every row from one line instead (`-0.500`, `+1.250`), and selecting rows shows their elapsed time in the status bar (`Δ +2.357 · 14 rows`).
- **External Tools**: configurable commands run on a row from the right-click menu, the stream menu or a shortcut (`code -g "{file}:{lineno}"`, `ssh {match}`), with the placeholders `{line}`, `{file}`, `{dir}`, `{lineno}`, `{selection}` and `{match}` (first capture of the tool's own regex). A tool can be bound to a highlight rule and runs when the rule matches an appended line, at most once per second and with at most 10 children at a time. Arguments reach the program as separate argv entries, never through a shell, unless "run via shell" is deliberately switched on.
- **Line Wrap**: a per-stream `↩ Wrap` toggle (`Alt+W`) soft-wraps long lines (JSON payloads, stack traces, URLs) at the window width instead of scrolling horizontally. Wrapped rows keep their line number, marker and colours; search, bookmarks, go-to and paging still navigate by line. Only the rows in view are laid out, so wrapping stays cheap on huge files; the scroll bar thumb is approximate in wrap mode. The toggle is saved per file.
- **Telemetry & FX**: CPU and memory in the title bar, per-stream throughput, optional borderless window, Matrix digital rain screensaver after a configurable idle time (10 minutes by default).
- **BareTail Migration Bridge**: one-click import of recent files and highlight colors from BareTail / BareTailPro on Windows.
- **Multilingual UI**: 16 languages — English, German, Spanish, French, Italian, Dutch, Polish, Portuguese (Brazil), Turkish, Russian, Ukrainian, Japanese, Korean, Chinese (simplified and traditional) and Friulian — picked automatically from the system locale and switchable in Settings (the first entry of the picker, "System language", keeps following the OS), with CJK font fallback for Chinese, Japanese and Korean.
- **Crash Logger**: an unexpected panic writes `fasttail_crash.log` with version, commit and build timestamp so it can be reported.

---

## 📊 Comparison with Other Tail Tools

| Feature | FastTail | BareTail (Free/Pro) | Tailviewer | SnakeTail | `tail -f` / CLI |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Engine / Architecture** | **Rust** | Win32 C++ (2006) | .NET / C# | C# / WPF | POSIX C |
| **Binary Size** | **~8 MB download, ~20 MB executable (single binary)** | ~220 KB | ~45 MB | ~1.5 MB | ~50 KB |
| **Runtime Dependencies** | **Zero (standalone native)** | Zero (Win32 native) | .NET Runtime required | .NET Framework / WPF | POSIX coreutils |
| **Large Files (>50 GB)** | **Instant** | Good | Slow / High RAM | Moderate | Fast |
| **Cross-Platform** | **Windows, Linux, macOS** | Windows only | Windows only | Windows only | Linux/macOS |
| **User Interface** | **Cyberpunk UI (GPU)** | Win32 Classic | Modern Windows | Classic Windows | Terminal CLI |
| **Multi-Tab / Docking** | **Full modular docking** | Tabs only | Tabs & Panels | Tabs & Splits | Multiple terms |
| **Binary Hex View** | **Yes, with byte-level search** | No | Plugin required | No | No (`xxd`) |
| **Markdown / HTML View** | **Yes** | No | No | No | No |
| **Line Wrap** | **Per stream (`Alt+W`), navigation by line kept** | Yes | No | No | Terminal wrap |
| **Directory Wildcard Tail** | **`app-*.log` follows the newest match, switch keeps filters** | No | No | Yes | `tail -F` one file |
| **Include / Exclude Filters** | **Live, text or regex, several ANDed terms, saved presets** | Pro version only | Yes | Yes | `grep` pipe |
| **Log Level Detection** | **Built-in: colouring, `≥ level` filter, counters** | No | Yes | No | N/A |
| **Compressed Logs** | **`.gz` (multi-member) and `.zip` entries, decompressed in the background** | No | No | No | `zcat \| tail` |
| **Time Range Filter** | **`from / to` window, go-to-time, visible span** | No | Yes | No | `awk` by hand |
| **Search Results Pane** | **Matching lines listed under the log, overview strip beside the scroll bar** | No | No | No | `grep` output |
| **Search All Open Files** | **One query over every stream, results grouped by stream (`Ctrl+Shift+F`)** | No | No | No | `grep` over files |
| **Highlighting Styles** | **FG, BG, Bold, Italic** | FG, BG | FG, BG | FG, BG | ANSI codes |
| **ANSI Colour Codes in Logs** | **Rendered, stripped or shown raw, per stream; filters see the text without codes** | No | No | No | Rendered by the terminal |
| **Capture-Group Highlight / Quick Labels** | **Captures-only rules, `Ctrl+Shift+1..9` labels** | No | No | No | No |
| **Rule Priority Reordering**| **Yes (⬆ / ⬇ top-down)** | Limited | Yes | Yes | N/A |
| **External Tools** | **Placeholders, shortcuts, rule-bound runs, no shell by default** | No | No | Yes | Pipes |
| **Sound Alerts** | **Presets (Beep/Chime/Crit)** | No | Plugins | Limited | Bell (`\a`) |
| **Encoding Support** | **ASCII, ANSI, UTF-8, UTF-16 LE/BE** | ANSI, UTF-8, Unicode | UTF-8, ANSI | UTF-8, ANSI | Terminal enc |
| **JSON Formatter** | **Inline pretty-print** | No | Plugin required | No | `jq` pipe |
| **Screensaver Mode** | **Matrix digital rain** | No | No | No | No |
| **Open Source & License** | **MIT License** | Proprietary | MIT | GPL | Open Source |

---

## ⌨️ Keyboard Shortcuts Reference

Search and navigation shortcuts act on the stream in the **focused dock panel** (click a panel to focus it).

| Shortcut | Description |
| :--- | :--- |
| `Space` | Toggle Follow mode (auto-scroll to the latest line) |
| `Ctrl + F` | Focus the search box of the focused stream |
| `Ctrl + Shift + F` | Open or focus the Find results tab to search every open stream, prefilled with the focused stream's query (`Enter` runs it) |
| `↑` / `↓` / `PgUp` / `PgDown` / `Ctrl + Home` / `Ctrl + End`, `Enter`, `Esc` in the Find results list | Move the selection across the groups; `Enter` on a header collapses / expands it, on a result shows it in its stream; `Esc` leaves the list |
| `F3` / `Shift + F3` | Next / previous search match in the focused stream |
| `Enter` / `Shift + Enter` | Next / previous match while typing in the search box |
| `↑` / `↓` / `PgUp` / `PgDown` / `Ctrl + Home` / `Ctrl + End` in the results pane | Move the pane selection (after a click in the pane); the main view stays where it is |
| `Enter` in the results pane | Make the selected hit the current match and centre it in the main view (pauses follow) |
| `Esc` in the results pane | Give the keyboard back to the main view; `F3`, `Ctrl + F`, `Ctrl + G`, bookmarks and `Ctrl + C` keep acting on the stream while the pane has it |
| `↑` / `↓` / `←` / `→` | Scroll by one line / column (`Ctrl` + `←` / `→` scrolls 5x faster) |
| `PgUp` / `PgDown` | Scroll by one page |
| `Home` / `End` | Scroll horizontally to the far left / right |
| `Ctrl + Home` | Jump to the top of the file and pause follow |
| `Ctrl + End` | Jump to the latest line and resume follow |
| `Click` / `Shift + Click` / `Ctrl + Click` | Select a row / extend the selection over the visible rows / toggle a row |
| `Ctrl + A` | Select every visible row of the focused stream |
| `Ctrl + C` | Copy the selected rows (or the current search hit) as plain text |
| `Ctrl + G` | Go to line N, `+N` / `-N` from the current line, or a time such as `14:02` (first line at or after it); hidden lines resolve to the next visible one |
| `Ctrl + Shift + T` | Toggle always-on-top (also the 📌 pin in the title bar and a Settings checkbox) |
| `Ctrl + L` | Lock the window behind the PIN (needs a PIN set in Settings → PIN lock) |
| `Ctrl + F2` / `F2` / `Shift + F2` | Bookmark the current row (`★` in the marker column) / jump to the next / previous bookmark, wrapping around; bookmarks are saved per file |
| `Alt + W` | Toggle line wrap for the focused stream (also the `↩ Wrap` button in the stream bar); saved per file |
| `Ctrl + Shift + 1..9` | Create, recolour or remove the quick colour label (preset 1..9) for the current search text of the focused stream; labels apply to every stream and are listed above the rows |
| `Alt + 1..9` | Switch to stream tab #1 through #9 |
| `Ctrl +` / `Ctrl =` | Zoom the interface in (+10%) |
| `Ctrl -` | Zoom the interface out (-10%) |
| `Ctrl 0` | Reset the zoom to 100% |
| `Ctrl + MouseWheel` | Smooth zoom with the wheel |
| `F1` | Open the Help & Keyboard Shortcuts dialog |
| `Esc` | Close the active dialog, or leave the search box |

Zoom scales the whole interface, log text included, and is shown as a percentage in the title bar next to the always-on-top pin (dim at 100%, accent colour when zoomed; click it to go back to 100%). Settings → Zoom has the same control, and the value is saved in `fasttail.ini` (`zoom_factor`), so the window reopens at the scale you left it. The separate **Font size** setting sets the log text in points, independently of the zoom.

Files can also be opened by **drag & drop** onto the window or from the command line:

```text
fasttail [OPTIONS] [PATH...]

  PATH...            log files to open in addition to the restored workspace
  -                  read standard input (`command | fasttail -`); a file named `-`
                     is opened as `./-`
  --fresh            start with an empty workspace instead of the saved one
  --filter <TEXT>    include filter for the files opened from the command line
  --exclude <TEXT>   exclude filter for those files
  --follow / --no-follow
                     follow mode for those files (ignored for compressed files)
  --renderer <NAME>  auto (default), glow, wgpu or software
  --config <FILE>    configuration file to use (same as FASTTAIL_CONFIG)
  --session <FILE>   load a session file (*.fasttail-session.ini) at startup
  -V, --version      print the version and exit
  -h, --help         print the usage and exit
```

Example: `fasttail --fresh --filter ERROR app.log err.log`.

### Standard input
`command | fasttail -` copies the command's output into a spool file (in the same `spool_dir` as compressed logs) that is tailed like any followed log, so filters, search, levels, time range, highlight rules, bookmarks and export all work on it. The tab is titled `stdin`; the footer and the tab tooltip name the spool. When the command ends the stream stays open and its bar says `input ended · N lines`. Without `-`, piped or redirected input (`command | fasttail`, `fasttail < app.log`) is picked up too, but the tab only appears once the first byte arrives, so a launcher that hands FastTail a silent pipe does not get an empty tab. `-` given twice is a usage error (exit code 2); `-` with nothing piped prints `standard input is not a pipe; nothing to read` on stderr and the rest of the command line still opens. `--filter`, `--exclude` and `--follow` / `--no-follow` apply to the stdin stream too.

The stdin stream is never saved in the workspace, the recent files or a session (saving a session says it was left out) and is kept when another session is loaded; its spool is deleted when the tab is closed and at exit. Closing the tab closes FastTail's end of the pipe, so the producer gets a broken pipe on a later write. The spool is bounded by `stdin_spool_max_mb` (default `2048`, 64–65536, Settings → Performance & refresh) and by a 512 MB free-space margin checked every 64 MB: at either limit the spool restarts from empty, the view restarts with the new lines, and the stream bar says that earlier input was discarded.

```text
# bash, zsh, Git Bash
kubectl logs -f pod-7 | fasttail --filter ERROR -
# cmd.exe
ping -t localhost | fasttail -
# PowerShell: for a live producer, go through cmd (see the table)
cmd /c "kubectl logs -f pod-7 | fasttail -"
```

Checked on Windows 11 with the GUI-subsystem build (`#![windows_subsystem = "windows"]`), which reads the standard-input handle the shell passes:

| Shell | Result |
|---|---|
| cmd.exe | `type app.log \| fasttail -`, `ping -t localhost \| fasttail -` and `fasttail - < app.log` work, lines arrive live and UTF-8 is kept byte for byte; cmd waits for FastTail to close before showing the prompt again. Auto-detection without `-` works. |
| Git Bash (MSYS2) | `cat app.log \| ./fasttail -` and `./fasttail - < app.log` work, bytes unchanged. `< /dev/null` counts as a console: nothing is read. |
| PowerShell 7.6 | The pipe is connected and the prompt returns at once. `Get-Content app.log \| fasttail -` arrives complete (UTF-8 kept, lines re-written with CRLF). Between two native programs (`ping -t localhost \| fasttail -`) the output only arrived when the producer ended, so a live producer is not live: use `cmd /c "producer \| fasttail -"`, which streams. |
| Windows PowerShell 5.1 | The pipe is connected, but text is re-encoded with `$OutputEncoding` (US-ASCII by default): non-ASCII characters arrive as `?`. Set `$OutputEncoding = [System.Text.UTF8Encoding]::new($false)` first, or use `cmd /c "producer \| fasttail -"`, which keeps the bytes. |
| Start-Process, `start`, shortcuts | Launched without redirection, standard input is absent: no stdin tab. Launching from Explorer, a desktop shortcut and Windows Terminal was not checked directly. |

Not verified: Ctrl+C in the console while a producer runs (a producer that ends, here a killed `ping`, is shown as `input ended`), and the producer receiving a broken pipe when the tab is closed (covered by an automated test of the copier, not by a manual run).

---

## ⚙️ Configuration

Settings are stored in a single `fasttail.ini` file, looked up in this order:

1. the path in the `FASTTAIL_CONFIG` environment variable, if set;
2. `fasttail.ini` in the current working directory (portable layout);
3. `fasttail.ini` next to the executable;
4. the per-user directory: `%APPDATA%\FastTail` on Windows, `$XDG_CONFIG_HOME/FastTail` or `~/.config/FastTail` elsewhere.

New installs write next to the executable and fall back to the per-user directory when that folder is read-only (for example `Program Files`). A legacy `fasttail.toml` from older versions is migrated automatically on first start.

### Sessions
The 🗂 button in the title bar saves the workspace under a name and loads it back. A session file (`name.fasttail-session.ini`) holds the open files and patterns, the dock layout, and for every stream its include/exclude filters, search query, line wrap, encoding, ANSI mode and bookmarks (and the entry of a zip, `entry=`); theme, language, highlight rules and the other preferences stay in `fasttail.ini`. Loading a session replaces the current streams; if the current named session has unsaved changes (a `*` after its name in the title bar) FastTail asks first. Files that no longer exist are listed and skipped.

Paths are written absolute and, when the file lies under the session's folder, also relative to it: a session saved next to a log bundle still opens after the bundle is moved or copied elsewhere. "Save current as default workspace" writes the workspace back into `fasttail.ini` and leaves the named session. `fasttail --session incident.fasttail-session.ini` loads a session at startup.

### External tools
What they are for, with ready-made recipes (open the row in an editor, SSH to the host in the line, pretty-print its JSON, alert on a rule): **[External tools cookbook](docs/external-tools-cookbook.md)**.

Settings → External tools. Each tool has a name, a program, an argument list and optional extras. The arguments are split like a command line (quotes group words) and every entry is expanded and passed to the program as its own argv element, so a log line containing `; rm -rf /` is only text.

| Placeholder | Value |
|---|---|
| `{line}` | text of the row |
| `{file}` | path of the file being tailed (the resolved file of a pattern stream, the archive of a compressed one) |
| `{dir}` | its directory |
| `{lineno}` | 1-based line number |
| `{selection}` | the selected rows as text, or the row itself |
| `{match}` | first capture group of the tool's own regex applied to the row (the whole match without a group, empty when it does not match) |

Extras: a **shortcut** such as `Ctrl+Shift+F9` (a modifier is required) runs the tool on the current row of the focused stream; **run on rule** binds the tool to a highlight rule, so it runs when the rule matches an appended line, at most once per second per tool and with at most 10 children running at the same time, the excess being counted as dropped runs in the settings row. **Run via shell** wraps the program in `cmd /c` (Windows) or `sh -c`, with every expanded argument quoted for that shell; operators written in the argument list (`|`, `>`, `&&`) are quoted too, so a pipeline belongs in a script. The Windows quoting is weaker than the POSIX one, so keep it off unless the log is trusted. Tools get no standard input and their output is discarded. Tools are stored as `[tool.N]` sections of `fasttail.ini`.

### Compressed logs
A gzip file (`1f 8b`) or a zip archive (`PK\x03\x04`) is recognised by its first bytes, whatever its name, and opened read-only. The data is decompressed on a background thread, 1 MB at a time, into a spool file that the engine tails like a growing log; the stream bar shows `decompressing N%` (compressed bytes read) with a ✖ to stop, which keeps what was already read. Follow is off for these streams — the archive is a snapshot and is not watched; the ⟳ button extracts it again. Multi-member gzip (`cat a.gz b.gz`) and zip entries stored or deflated (Zip64 included) are supported; encrypted entries, other zip methods (bzip2, zstd, lzma...), tar archives inside gzip (`.tar.gz`, `.tgz`) and zip entries with unsafe names (absolute or `../`) are refused with the reason; `.bz2`, `.xz` and `.zst` files are not decompressed and open as they are (binary content in HEX view).

A zip with a single file opens it directly; with several, the entry picker lists them with their size. The workspace, sessions, recent files and bookmarks remember the archive (plus the entry, stored as `entry=` in a session file), never the spool, and a restored stream is decompressed again.

| Setting in `fasttail.ini` | Default | Meaning |
|---|---|---|
| `spool_dir` | empty = the system temporary folder | folder whose `fasttail-spool` subfolder receives the decompressed copies (Settings → Performance & refresh; point it at a larger disk when `%TEMP%` is on a small system drive) |
| `compressed_max_gb` | `20` (1–1024) | output cap of one decompression; the lines read so far stay browsable and the stream says the content is partial |

Before a zip entry is decompressed its exact size plus a 512 MB margin must fit on the spool volume; during any extraction the free space is checked again every 64 MB and the job stops when less than 512 MB would remain. Spool files are named after the process id, deleted when their stream is closed or reloaded and at exit, and the ones left behind by a crash are swept at the next start.

### PIN lock
Settings → PIN lock. Set a PIN of 4 to 12 digits and the window can be locked behind it: with **Lock when the screensaver ends** on, coming back from the Matrix screensaver asks for the PIN, and `Ctrl+L` (or the **Lock now** button) locks on demand. While locked, an opaque animated backdrop covers the window and every keyboard shortcut is ignored — `Esc` included — while the streams keep tailing behind it, so nothing is missed. `Enter` confirms the PIN, and three wrong PINs in a row replace the entry field with a one-minute countdown.

The PIN is scrambled before it is written to `fasttail.ini` (`lock_pin`), so it is not readable at a glance. That is the extent of it: **the lock is a deterrent against someone walking past the screen, not a security boundary.** The log files stay readable on disk, the config file can be edited, and a maintenance unlock phrase opens the prompt whatever the PIN is. Do not use it to protect sensitive logs — use the operating system's screen lock and file permissions for that.

### Filters and presets
The stream bar has an **Include** and an **Exclude** field; each takes plain text or a regular expression, under the stream's `Aa` (case-sensitive) and `.*` (regex) toggles. A condition that needs two words on the same line — "payment" *and* "timeout", without "healthcheck" *or* "retry=0" — does not need a regex: the `+` next to a field adds another term row and opens the Filters window on that stream, where every stream lists its terms under **All of** (include: a line must contain every one) and **None of** (exclude: a line containing any of them is hidden), up to 8 per side. The stream bar keeps showing the first term of each side, with a `+N` badge (the other terms in its tooltip) so a hidden condition is never invisible. Terms are matched exactly as typed — `a && !b` finds the text `a && !b`; there is no operator syntax. For OR inside one term use a regex such as `timeout|refused`, and for a single case-insensitive term in a case-sensitive stream the inline flag `(?i)`. A regex term that does not compile is flagged in its row and matches nothing. The minimum level and the time range then apply as before, and a stack-trace line still follows its entry unless an exclude term matches it. Every term is saved per stream in the workspace and in session files (`include`, then `include.2`, `include.3`…, so an older FastTail still reads the first).

`Presets ▾` in the stream bar saves the stream's filter state under a name — include and exclude terms, the `Aa` / `.*` toggles, the minimum level and its `?` toggle, and, when **Include the time range** is ticked, the two time fields as typed, so a bare `14:02` follows the day of whichever log the preset is applied to. Click a preset to apply it to the stream, or **all** beside it to apply it to every open stream; a preset without a time range leaves each stream's window as it is. The drop-down shows the name of the preset the stream currently equals, and `name *` once the stream has been edited after applying it, with **Update "name" from this stream** in the menu. **Manage presets…** opens the Filters window, where presets are renamed, reordered and deleted (after a confirmation). Presets are global preferences stored in `fasttail.ini` as `[filter_preset.N]` sections, not in sessions.

### Time range
Above the buffer, `🕘 from → to` keeps only the lines stamped inside the window. Both sides are optional, so `from 14:02` alone means "everything after 14:02". The fields take a bare time (`14:02`, `14:02:05`), a full `YYYY-MM-DD HH:MM[:SS]`, a date alone (`2026-09-18`: from midnight, or through 23:59:59 on the "to" side), or a timestamp copied straight out of a log line; a bare time belongs to the **day of the log**, not to today, so yesterday's file reads the way it is written.

The timestamp of each line is read from the line itself — ISO 8601 (with `T` or a space, optional fraction and zone), syslog (`Sep 18 14:02:05`), Apache/nginx (`[18/Sep/2026:14:02:05 +0200]`) and bare epoch seconds or milliseconds — within the first 64 bytes, with no format to configure. Times are compared **on the clock the log printed**: a zone suffix such as `+0200` is not applied, so typing `14:02` finds the line that says `14:02` (epoch values, which print no clock, read as UTC). A line that carries no timestamp of its own **inherits the one above it**, so the stack trace of an entry stays with the entry instead of falling out of the window.

`Ctrl+G` accepts a time as readily as a line number (`14:02` jumps to the first line at or after it), and the stream status bar shows the span of what is currently visible. When fewer than half the lines can be placed in time (a timestamp of their own, or one inherited from the entry above, as stack trace lines do), a hint says so, since a window would hide them.

A stream is timed the first time the time range or a time jump needs it, not when it is opened. Up to 16 MB that is instant; above it the file is timed in the background, with `⏳ timing lines 37%` in the stream bar, and the window stays responsive. A window typed meanwhile is held — every line stays visible and a hint next to the fields says it applies when timing finishes — and a time entered in `Ctrl+G` waits in the popup with the same progress and jumps when the scan completes (`Esc` drops the jump, not the scan). Once timed, the cache follows the file as it grows, and filters and search on a large file keep running in the background with the window applied to their results.

### Time deltas
The `Δt` button in the stream toolbar, next to the line-number toggle, shows a column with the time since the **previous visible row** for each line that carries a timestamp of its own: `+0.125` below a minute, `+4:05.120` below an hour, `+2:03:04` below a day, `+3d 04:05` beyond. Because it follows the filters, an include filter such as `payment` makes it read the time between the payment lines. Continuation lines (stack trace frames) stay blank; a delta of at least `time_delta_gap_ms` (1000 by default, `0` turns it off, also in Settings) is drawn in the accent colour. The switch is global and saved in `fasttail.ini` as `show_time_delta`.

Right-click a row and pick **Set time anchor here** to measure every row from that line instead: rows below read `+1.250`, rows above `-0.500`, the anchor shows `⚓`, and the gap tint is off. **Clear time anchor** returns to the previous-row delta; the anchor survives filter changes, is dropped when the file is truncated or rewritten, and is not saved. With two or more rows selected the stream status bar shows the time from the first to the last one, `Δ +2.357 · 14 rows` (`Ctrl+A` spans the first and last visible rows).

The times are those of the time range, compared **on the clock the log printed**: zones are not applied, so a log that changes offset (daylight saving, `Z` mixed with local times) shows the jump. Turning the column on never pauses the window: up to 16 MB the file is timed at once, above it in the background, and rows not timed yet show `…`. On a stream without usable timestamps the column stays hidden and the button's tooltip says why.

### Rendering backend
FastTail starts on `wgpu` (Direct3D 12 or Vulkan on Windows, Vulkan on Linux, Metal on macOS) and, if that backend cannot be created, retries automatically with OpenGL. Both backends run without vsync because many drivers (NVIDIA on Windows among them) busy-wait for the vertical blank and burn CPU cores whenever egui repaints; running without vsync and with paced rendering keeps continuous repaints lightweight. The status bar shows which backend is active: `WGPU`, `GL`, or `GL fallback` when the retry happened; hover it, or open About, for the adapter details.

A `software` renderer is also available as a last-resort fallback for machines with no usable GPU (or for troubleshooting driver problems). It forces the wgpu CPU rasterizer (WARP on Windows, llvmpipe on Linux). **This mode is not optimized and is CPU-hungry: WARP is designed to spread rasterization across every logical core, so the process keeps several cores busy even when the log is idle and the window is unfocused.** The cost only drops when the window is minimized. On a machine with a working GPU use `auto` (the default), `wgpu` or `glow` instead; software mode only makes sense when no hardware backend can start. A banner reminds you while the software renderer is active.

| Setting | Values | Where |
|---|---|---|
| `renderer` in `fasttail.ini` | `auto` (default), `glow`, `wgpu`, `software` | Settings dialog, applies at the next start |
| `FASTTAIL_RENDERER` | same values, overrides the config | environment, useful for support: `FASTTAIL_RENDERER=wgpu fasttail` |

When the first backend fails, the error is printed to stderr together with `renderer: falling back to OpenGL`.

---

## 🛠️ Building & Installation

### Prerequisites
- [Rust](https://rustup.rs/) 1.88 or newer
- On Linux: `libasound2-dev libudev-dev pkg-config libx11-dev libxcb1-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libwayland-dev`

### Build from Source
```bash
git clone https://github.com/matteobaccan/FastTail.git
cd FastTail

# Run development build (optionally with files to open)
cargo run -- app.log other.log

# Run the test suite
cargo test

# Build optimized release binary
cargo build --release
```

The standalone executable is written to `target/release/fasttail` (`target/release/fasttail.exe` on Windows).

### Benchmarks
`cargo bench` runs `benches/filter_bench.rs`, which times the engine hot paths (indexing, include/exclude and regex filters, search, highlight scanning) on a synthetic 200 MB log generated in a temporary directory. It is built only on demand, never by `cargo test`.

| Variable | Effect |
|---|---|
| `FASTTAIL_BENCH_LOG=<file>` | benchmark an existing log instead of generating one |
| `FASTTAIL_BENCH_BYTES=<n>` | size of the generated log (default 200000000; 1100000000 was used for the LTO decision) |
| `FASTTAIL_BENCH_ROUNDS=<n>` | rounds per phase, best time reported (default 3) |

### Prebuilt binaries
Tagged versions are published on the [Releases](https://github.com/matteobaccan/FastTail/releases) page as one archive per platform. Each name carries the version (`<version>` below, for example `0.10.1`) and each archive holds the program with `LICENSE` and `README.md`:

| Platform | Asset | Contents |
|---|---|---|
| Windows x86_64 | `fasttail-windows-x86_64-<version>.zip` | `fasttail.exe`, `LICENSE`, `README.md` |
| Windows x86_64 debug symbols | `fasttail-windows-x86_64-symbols-<version>.zip` | `fasttail.pdb` (only needed to read a crash dump) |
| Linux x86_64 | `fasttail-linux-x86_64-<version>.tar.gz` | `fasttail`, `LICENSE`, `README.md` |
| Linux ARM64 | `fasttail-linux-arm64-<version>.tar.gz` | `fasttail`, `LICENSE`, `README.md` |
| macOS Apple Silicon | `fasttail-macos-arm64-<version>.tar.gz` | `fasttail`, `LICENSE`, `README.md` |

```bash
# Linux / macOS
tar -xzf fasttail-linux-x86_64-0.10.1.tar.gz && ./fasttail app.log

# Windows (PowerShell)
Expand-Archive fasttail-windows-x86_64-0.10.1.zip -DestinationPath fasttail; .\fasttail\fasttail.exe app.log
```

To read a crash report (`fasttail_crash.log`) with function names instead of addresses, unpack `fasttail-windows-x86_64-symbols-<version>.zip` (the same version as the program) and keep `fasttail.pdb` next to `fasttail.exe`.

#### macOS: "Apple cannot verify fasttail is free of malware"
The macOS build is **not signed with an Apple Developer ID**, so the first launch is blocked by Gatekeeper with exactly that message, and the dialog only offers to move the file to the bin. The binary is fine — it is simply unsigned, and macOS quarantines everything downloaded from a browser.

Clear the quarantine flag and run it:

```bash
tar -xzf fasttail-macos-arm64-0.10.1.tar.gz
xattr -d com.apple.quarantine ./fasttail   # or: xattr -cr ./fasttail
./fasttail app.log
```

Or, without the terminal: try to open it once, let it be blocked, then go to **System Settings → Privacy & Security** and press **Open anyway** next to the message about `fasttail`.

Signing and notarizing the build would remove the prompt for everyone, and needs a paid Apple Developer account — see [distribution channels](docs/distribution-channels.md). Every push and pull request runs the test suite in [GitHub Actions](https://github.com/matteobaccan/FastTail/actions); release binaries are built only for `v*` tags and manual workflow runs. FastTail is not published to crates.io, Scoop or winget: [docs/distribution-channels.md](docs/distribution-channels.md) records what each of those would take, for when it is worth deciding.

---

## ❓ FAQ

### What is FastTail?
FastTail is a free, open-source (MIT) desktop application for viewing and following log files in real time — a graphical `tail -f`. It is written in Rust, renders on the GPU, runs on Windows, Linux and macOS, and opens several logs side by side in a docking workspace with live filters, highlighting, search and a hex view.

### Is FastTail a good BareTail alternative?
Yes: it was designed as a successor to BareTail. It keeps the things BareTail users rely on — instant opening of huge files, follow mode, coloured highlight rules — and adds include / exclude filters (text or regex) in the free version, regex capture-group highlighting, log level detection, a hex view, Markdown rendering, docking, and Linux and macOS builds. On Windows, when FastTail finds BareTail settings in the registry it offers to import the recent files and highlight colours in one click.

### How do I tail a log file in real time on Windows?
Download `fasttail-windows-x86_64-<version>.zip` from the [Releases](https://github.com/matteobaccan/FastTail/releases) page, unpack it and run `fasttail.exe app.log` (or drag the file onto the window). Follow mode is on by default and scrolls to every new line; `Space` toggles it. There is no installer and no runtime to install: it is a single executable.

### Is there a GUI for `tail -f` on Linux or macOS?
FastTail is one. The same features ship for Linux x86_64, Linux ARM64 and macOS Apple Silicon as a single binary: `tar -xzf fasttail-linux-x86_64-<version>.tar.gz && ./fasttail /var/log/syslog`. The macOS build is unsigned, see [the Gatekeeper note](#macos-apple-cannot-verify-fasttail-is-free-of-malware) for the one-time `xattr` command.

### Can FastTail open very large log files (multi-GB)?
Yes. The file is never loaded into memory: rows are read on demand through a small block cache, and the per-file state is a line index of 8 bytes per line plus, once scanned, a level byte and a timestamp per line and the search hits (at most 1,000,000, 8 MB). Files above 16 MB filter, search and read their timestamps on a background thread with progress in the stream bar, so the window stays responsive while a multi-gigabyte log is scanned.

### How do I show only the lines that match a pattern, or hide the noise?
Every stream has an **Include** and an **Exclude** box above the rows. Both accept plain text or a regular expression, case-sensitive or not, and apply as you type — `ERROR|CRITICAL` in Include, `healthcheck|ping` in Exclude. The `+` beside a box adds more terms: a line must match every include term and none of the exclude terms, so "payment and timeout, without health checks" needs no regex. The `≥ level` selector adds a minimum log level on top, and `Presets ▾` saves the whole setup under a name to apply again later. From the command line: `fasttail --filter ERROR --exclude DEBUG app.log` (the first term of each side).

### How do I highlight errors in colour?
Open **Color Filters** and add a rule: text or regex, foreground and background colour, bold, italic and optionally a sound (Beep, Chime, Warning, Critical). Rules are applied top-down and can be reordered. A regex rule with "Captures only" colours just its capture groups, and `Ctrl+Shift+1..9` turns the current search into a quick colour label.

### Can I monitor several log files at the same time?
Yes. Each file opens in its own stream that can be tabbed, split, docked or floated; the layout is restored at the next start. Named sessions save the whole set — files, filters, layout, bookmarks — so you can switch between projects in one click.

### Does FastTail follow rotated logs?
Yes. Rotation, truncation and in-place rewrites of a file are detected without locking it for the writer. To follow a logger that creates a new file per day or hour, open a wildcard pattern such as `C:\logs\app-*.log`: the stream follows the newest matching file and keeps its filters when it switches.

### How do I see only the log lines between two times?
Fill the `🕘 from → to` pair above the buffer: `from 14:02 to 14:10` keeps only the lines stamped inside that window, together with the include / exclude and level filters. Either side can stay empty (`from 14:02` means everything after it). A bare time refers to the day of the log, not to today, and a full `YYYY-MM-DD HH:MM[:SS]`, a date alone (the whole day) or a timestamp pasted from a line works too. Lines without a timestamp of their own, such as the frames of a stack trace, inherit the one above them and stay with their entry.

### Which timestamp formats does FastTail recognise?
ISO 8601 (with `T` or a space, optional fraction and time zone), syslog (`Sep 18 14:02:05`), Apache / nginx (`[18/Sep/2026:14:02:05 +0200]`) and bare epoch seconds or milliseconds, found within the first 64 bytes of the line. There is no format to configure, and a zone suffix is not applied: `14:02` means the `14:02` written in the line. When fewer than half the lines can be placed in time, the time controls show a hint, since a window would hide those lines.

### How do I jump to a specific time in a log?
Press `Ctrl+G` and type a time such as `14:02`: FastTail jumps to the first line at or after it. The same box still takes a line number or `+N` / `-N`. The stream status bar shows the time span of the lines currently on screen.

### Can FastTail open compressed logs (`.gz`, `.zip`)?
Yes. Open `app.log.1.gz` like any other file (the format is read from the content, so a gzip named `trace.dat` works too): it is decompressed on a background thread into a temporary file and every feature — filters, search, levels, time range, bookmarks, HEX — works on the result while the first lines are already on screen. A `.zip` with several logs shows an entry picker and each chosen entry opens in its own stream. Encrypted zip entries, bzip2/zstd/lzma zip entries and `.tar.gz` are not supported and say so. The decompressed copy costs disk space equal to its size, bounded by `compressed_max_gb` (20 GB by default) and a free-space check, and is deleted when the tab is closed.

### Can I pipe a command into FastTail, like `less`?
Yes: `kubectl logs -f pod | fasttail -` (or `docker compose logs -f`, `journalctl -f`, `ssh host tail -f app.log`). The output is copied into a temporary spool file and opened as a followed `stdin` stream with every feature available; when the command ends the stream stays open and says so. The copy is capped by `stdin_spool_max_mb` (2 GB by default), after which it restarts from empty, and it is deleted when the tab is closed. On Windows it works from cmd.exe and Git Bash; from PowerShell, use `cmd /c "command | fasttail -"` for a live command (see [Standard input](#standard-input)).

### Why does my `docker logs` capture show `[32m` everywhere, and can FastTail show the colours?
Those are ANSI colour codes written by the logger. FastTail detects them and renders the colours (16, 256 and 24-bit, bold, underline...) with a palette readable on the active theme; the `ANSI` selector in the stream bar switches a stream to **strip** (codes hidden, no colour) or **raw** (codes visible as `␛[32m`). While the codes are hidden, filters, search, highlight rules, level detection, copy and export work on the plain text, so an include filter `\bERROR\b` or the `≥ WARN` level filter keeps a red `ERROR` line. To match the codes themselves (`\x1b\[31m`), use raw mode.

### Can it view binary files or non-UTF-8 logs?
The **HEX** mode shows a live hexadecimal + ASCII dump with byte-level search (text or `0A 0D` patterns). Text encodings are detected automatically and can be overridden: ASCII, ANSI (Windows-1252), UTF-8 with or without BOM, UTF-16 LE and UTF-16 BE.

### How does FastTail compare with Tailviewer, SnakeTail, klogg or lnav?
Tailviewer and SnakeTail are Windows-only .NET applications; FastTail is a native single binary on three platforms. klogg is a fast Qt log viewer focused on searching large files; lnav is a terminal log navigator with SQL queries. FastTail sits between them: a GUI built for following live logs, with docking, per-rule sound alerts, hex and Markdown views and external tools bound to rows. See the [comparison table](#-comparison-with-other-tail-tools).

### Does FastTail send any data over the network?
No. It reads local files and writes only its own `fasttail.ini`, session files, the temporary decompressed copies of the compressed logs it opens (deleted when their tab closes) and, after a crash, `fasttail_crash.log`. There is no telemetry upload: the "System Telemetry" setting only shows CPU and memory in the title bar.

### Is FastTail free for commercial use?
Yes. It is released under the MIT License, which allows use, modification and redistribution, commercial included.

---

## 📐 Specifications

Behaviour is documented as [OpenSpec](https://github.com/Fission-AI/OpenSpec) specifications under [`openspec/specs`](openspec/specs): stream engine, search and navigation, filters and highlighting (time range included), log intelligence (levels, timestamps, JSON, stack traces), ANSI escape codes, compressed input in the stream engine, docking UI and named sessions, themes, localization, external tools, window lock, screensaver, telemetry, BareTail migration, crash reporting, the release pipeline, the rendering backend, the command line, selection and export. Proposals not yet implemented live in [`openspec/changes`](openspec/changes).

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2026 **Matteo Baccan**
