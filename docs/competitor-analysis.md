# Competitor analysis

How FastTail compares with the log viewers people actually use, and where FastTail stands:
✅ done, ⬜ to do. It is refreshed after every release (the post-release scan); each refresh
replaces the tables, so the file says what is true now, not how it got there.

**Last scan:** 2026-10-30, against FastTail **0.30.0**. Release dates and star counts come
from the GitHub release and repository pages on that day (stars rounded as GitHub shows
them), Seq's from Docker Hub; every other claim links its source. LogViewPlus, LogFusion and
Seq rest on the earlier scans (no new release seen).

## Contents

1. [The field in one table](#1-the-field-in-one-table)
2. [Desktop viewers](#2-desktop-viewers)
3. [Terminal and server-side tools](#3-terminal-and-server-side-tools)
4. [Market signals](#4-market-signals)
5. [What FastTail covers](#5-what-fasttail-covers)
6. [Gaps, ranked](#6-gaps-ranked)
7. [The terminal interface against the window](#7-the-terminal-interface-against-the-window)
8. [To do](#8-to-do)

## 1. The field in one table

| Product | Kind | Licence / price | Platforms | Last release | Activity |
|---|---|---|---|---|---|
| **FastTail** | GUI (GPU) and TUI (`fasttail-tui`) | MIT | Windows, Linux, macOS | 0.30.0, 2026-10-29 | active |
| BareTail / BareTailPro | GUI | freeware / $35 | Windows | 3.50a (2006-11-02) / 2.50a (2006) | abandoned |
| Tailviewer | GUI | MIT | Windows (.NET 4.7.1) | 1.0.0-rc1, 2022-09-12 | dormant (last commit 2022-09) |
| LogExpert (LogExperts fork) | GUI | MIT | Windows (.NET 10) | 1.50.0, 2026-10-03 | very active (1.2k ★) |
| klogg | GUI | GPLv3 | Windows, Linux, macOS | 22.06, 2022-06 (continuous builds to 2024-11-26) | unmaintained (3.5k ★) |
| glogg | GUI | GPLv3 | Windows, Linux, macOS | no release or commit since 2021-05 | abandoned |
| SnakeTail | GUI | GPLv3 | Windows (.NET 2.0) | 1.9.8, 2024-02-29 | quiet |
| LogViewPlus | GUI | $45 / $95 per user | Windows | 3.2.9, 2026-05 | active |
| LogFusion | GUI | free personal / Pro $19–$899 | Windows | 7.2, 2026-07-21 | active |
| lnav | TUI | BSD-2 | Linux, macOS, BSD | 0.14.1, 2026-09-05; 0.15.0-beta1, 2026-09-28 (no later release; master active to 2026-10-28) | very active (10.8k ★) |
| tailspin | CLI highlighter | MIT (Rust) | all | 7.0.0, 2026-07-14 | active (8k ★) |
| hl | CLI formatter | MIT (Rust) | all | 0.36.5, 2026-10-21 | active (3.3k ★; bug-fix releases) |
| gonzo | TUI dashboard | MIT (Go) | all | 0.4.3, 2026-07-15 | active (2.8k ★) |
| nerdlog | TUI, multi-host SSH | BSD-2 | Linux, macOS | 1.13.0, 2026-10-03 | active (1.6k ★; no release since) |
| toolong | TUI | MIT (Python) | all | 1.4.0, 2024-03 | dormant (4k ★) |
| Seq | server + web UI | proprietary, free single user | Windows, Docker | 2026.1 patch build 2026.1.17473, 2026-10-23 on [Docker Hub](https://hub.docker.com/r/datalust/seq/tags) (no 2026.2; notes not reachable) | active |
| Grafana Loki / Drilldown | server + web UI | AGPL-3.0 | server | 3.7.9, 2026-10-20 (fixes) | active (29.1k ★) |

Also watched: Chipmunk (Rust/Electron, DLT / PCAP, 4.3.0 2026-09-18 and 4.4.0 2026-09-29), LogFX, LogoRRR, LogMX,
LogSquirl (a klogg fork with a table view, beta), angle-grinder, logdy, lazyjournal,
otel-tui, logana (a young Rust viewer now closest to FastTail's shape: **0.8.0**
(2026-10-23) added a native desktop GUI (GPUI) beside its TUI, from one code base, with an
MCP server, an OTel collector and remappable keys; **0.8.2** (2026-10-26) adds `n` / `N` to
the next line of the selected filter, a file picker with name search, nested archives in
the archive picker, **numbered rotations opened as one stream** (`app.log.1`, `.2`, …), a
time-only timestamp filter and an editor for several field conditions; **0.8.3** (2026-10-28)
edits and removes annotations, edits and reorders filters with the right button, matches
exact field values (`--exact`), keeps five recent files and expands / collapses multi-line
messages; 0.7.7 imported
Notepad++ AnalysePlugin / UDL filters; 46 ★,
[releases](https://github.com/pauloremoli/logana/releases),
[changelog](https://raw.githubusercontent.com/pauloremoli/logana/main/CHANGELOG.md)). LogoRRR moved to a freemium
product (LogoRRR Pro).

## 2. Desktop viewers

**BareTail / BareTailPro** — the tool FastTail set out to replace. Tail, regex search, a Pro
include/exclude mode and saved searches with a sortable result table; unchanged since 2006–08
([baremetalsoft](https://www.baremetalsoft.com/baretailpro/)). FastTail covers it, import
bridge included.

**Tailviewer** — merge of several files by timestamp, multiline entries, level and time
filters, bookmarks, folder sources, a plugin API
([GitHub](https://github.com/Kittyfisto/Tailviewer)). Dormant; its most-voted requests were
a dark theme (#365) and OR filters (#368, #139).

**LogExpert** — the most active open-source desktop competitor
([releases](https://github.com/LogExperts/LogExpert/releases)): **columnizers** that split
lines into a grid (CSV, JSON, CLEF, Log4j XML, regex, auto-detect), filter-to-tab, triggers
that bookmark or run a plugin, an **SFTP** plugin, sessions, audio alerts, dark mode. Its
top request is word wrap (#33, which FastTail has); then multiline messages (#11) and
readable Unix timestamps (#139). **1.50.0** (2026-10-03,
[notes](https://github.com/LogExperts/LogExpert/releases/tag/v1.50.0)) added a
command-line option to open a file **at a given line** (#58), folder drops with a
cancellable file picker, a configurable **overview marker bar** beside the scroll bar,
**hiding single lines** by hand (#338), a configurable selection colour and copying of
highlight entries. An open request asks for **Drain-style log-template mining** (#654).

**klogg** — the fast cross-platform C++ viewer, unmaintained since 2022
([releases](https://github.com/variar/klogg/releases)): 10+ GB files, **boolean AND / OR /
NOT between patterns**, search limited to a part of the file, a scratchpad, configurable
shortcuts, 7z. Its most-voted requests — search in all files (#41), ANSI colours (#338),
quick highlights (#259) — are all in FastTail; open ones are cross-line regex (#333) and a
table mode (#510). Users who want a maintained, native, cross-platform GUI have few choices
left: that is FastTail's opening.

**SnakeTail** — Windows Event Log tailing without admin rights, MDI, external tools,
folder tail, tray icon ([GitHub](https://github.com/snakefoot/snaketail-net)). Requests:
partial-line selection (#60), hiding lines through rules (#70), collapsing stack traces
(#90, which FastTail's collapse covers), SSH (#21).

**LogViewPlus** — the deepest analysis in the category, Windows-only and paid
([release notes](https://www.logviewplus.com/dist/ReleaseNotes.txt)): parsers with
auto-detection (Log4j, JSON, XML, CSV, IIS, CLEF), a **SQL engine** over parsed fields,
**dashboards and charts**, merged views, EVTX, **SFTP / FTP / SCP**, a TCP listener,
time-zone settings, auto-highlight of the selected text, CSV / HTML export, an AI prompt
that writes SQL.

**LogFusion** — polished Windows viewer, most features Pro-only
([compare](https://www.logfusion.ca/Compare/)): Event Log (remote too), OutputDebugString
capture, custom columns, watched folders, **lines before / after a filter match**,
highlight sync between computers, scroll-bar markers.

## 3. Terminal and server-side tools

**lnav** — the reference TUI ([NEWS](https://raw.githubusercontent.com/tstack/lnav/master/NEWS.md)):
format auto-detection, all files **merged by time**, SQLite / PRQL over parsed fields, a
histogram (`i`), a Gantt timeline, SQL filter expressions, pretty-print, comments and tags,
sessions as scripts, `ssh` / `docker://` / `journald://` sources, less/vim keys, `e`/`E`
error jumps, Kitty keyboard protocol. **0.15.0-beta1** (2026-09-28,
[notes](https://github.com/tstack/lnav/releases/tag/v0.15.0-beta1)) adds:
- `:filter-context`, ±N messages around filter matches (`z` / `Z` change N), the feature
  FastTail has ✅;
- **named searches**: several searches stay highlighted at once, each with its own colour,
  `n` / `N` across all of them or `.` / `,` to focus one, a row per search in the timeline,
  saved in the session;
- CSV / TSV as **tabular formats** with separator and header detection (quoted multi-line
  cells joined), and a `metrics_log` format whose numeric columns can be drawn as
  **sparklines over the timeline** (up to four);
- per-column statistics in the details overlay (min..max, p50 / p90 / p99, distinct count);
- `lnav file split`, cutting a large log into pieces by lines, size or time;
- arrow keys moving by field rather than half a screen, and indexing of several files in
  parallel;
- **units** on format fields (suffix and divisor, `measure_value()`), so durations and sizes
  compare and show as read, per-file indexing time and memory in the files panel, and
  `log2src`, which maps messages to the logging statements of a source tree.

Since the beta, lnav's master (2026-10-24 to 2026-10-28,
[commits](https://github.com/tstack/lnav/commits/master)) made **source locations in stack
traces clickable**, opening the user's editor at that line, and cached thread and operation
ids, then (to 2026-10-28) kept working on `log2src` and bound `o` / `O` to the next /
previous message of the cursor row's operation id. Its most-reacted open requests include forcing a format or switching between several
JSON formats per file (#454, #1274, #634), which FastTail's per-stream parser choice covers,
and keeping the time column visible while scrolling sideways (#674).

**tailspin** — zero-configuration highlighting of dates, levels, URLs, IPs, UUIDs, paths and
key=value pairs; pages through `less` ([GitHub](https://github.com/bensadeh/tailspin)).
8k stars for highlighting alone.

**hl** — JSON / logfmt formatter with field filters (`-f`, `-q` boolean queries),
`--since -3h`, time-zone conversion, time-ordered merge at ~2 GiB/s
([GitHub](https://github.com/pamburus/hl)). 0.36.4 and 0.36.5 (2026-10-19 / 21,
[releases](https://github.com/pamburus/hl/releases)) are fixes: lines with the same
timestamp kept, no busy loop while lines wait, logfmt quoted keys, a lower glibc floor.

**gonzo** — k9s-style dashboard: severity and time charts, heatmap, **Drain pattern
clustering**, optional AI analysis, stdin / files / Kubernetes / OTLP inputs, vim keys and
mouse ([GitHub](https://github.com/control-theory/gonzo)). No release since 0.4.3; its
main branch has since added copying a log entry (`y` / `Ctrl+Y`), hiding the AI chat pane
and a Supabase source.

**nerdlog** — tails many hosts over SSH with no server, histogram on top; on its Show HN the
most requested feature was journald ([HN](https://news.ycombinator.com/item?id=43750765)).
1.12.0 and 1.13.0 ([releases](https://github.com/dimonomid/nerdlog/releases)) show
malformed and **out-of-order** records instead of dropping them, add `logfile` /
`loglineno` fields, vim-style search, more flexible timestamp parsing and files that mix
time zones.

**Chipmunk** — the Rust/Electron viewer for DLT, PCAP and text
([releases](https://github.com/esrlabs/chipmunk/releases)). 4.2.0 to 4.4.0 (2026-08 to
2026-09-29) added find within search results, recursive folder selection, pinned filter
presets kept across sessions, a UTF-16 parser, and a CLI `--preset` that filters files,
TCP and UDP sources with a preset exported from the GUI.

**toolong** — dormant, but its issue tracker lists what users of a TUI viewer expect: copy
(#23, #65), filtering (#37), pipes (#4), less-style jumps (#40), vim keys (#66), and no
100 % CPU polling (#17) ([issues](https://github.com/Textualize/toolong/issues?q=sort%3Areactions-%2B1-desc)).

**Seq / Loki / Kibana** — servers, not tailers, but they set users' expectations: structured
events and a query language, **pattern grouping** (Loki Logs Drilldown, Kibana `CATEGORIZE`),
live tail with a volume histogram, alerts, and in 2026 MCP servers for AI assistants
([Seq 2026.1](https://datalust.co/blog/seq-2026-1-released),
[Logs Drilldown](https://grafana.com/docs/grafana/latest/explore/simplified-exploration/logs/)).

## 4. Market signals

- **Structured logs**: the requests with the most votes in lnav (mixed JSON formats, forcing
  a format), klogg (#510) and LogExpert (#197) ask for fields and columns.
- **Merging files by time** is standard in every tool that is growing (lnav, hl, LogViewPlus,
  Chipmunk, LogoRRR, logana 0.7.8); FastTail has it ✅.
- **Context lines** became standard in 2026: lnav 0.15 (`:filter-context`) followed
  LogFusion and FastTail ✅.
- **Tabular and metric data**: lnav 0.15 reads CSV / TSV as a format and draws numeric
  columns as sparklines; logs are no longer the only text people open in a log viewer.
- **Malformed input is shown, not dropped**: nerdlog 1.12 keeps out-of-order and
  unparseable records visible and marks them.
- **Sources beyond local files**: journald, Docker / Kubernetes, SSH / SFTP, TCP / syslog.
- **Patterns and summaries**: grouping similar lines that are not adjacent (Drain), top-N
  values, charts. Drain is now requested in LogExpert too (#654).
- **AI and MCP**: Seq, gonzo, logana and LogViewPlus added AI prompts or MCP servers in 2026;
  FastTail has one ✅.
- **From the log to the code**: lnav links stack-trace locations to the editor and messages
  to their logging statements (`log2src`); IDE consoles have done the first for years.
  FastTail opens source locations in the editor ✅ and messages at their logging statements ✅.
- **One viewer, two faces**: logana 0.8.0 added a native GUI beside its TUI from one Rust
  code base, the shape FastTail has ✅.
- **Basics still win**: tailspin's growth shows zero-configuration highlighting sells; the
  top requests of older viewers (wrap, dark theme, all-files search, ANSI) are table stakes.

## 5. What FastTail covers

Requested elsewhere and done in FastTail:

- ✅ Word wrap, themes (dark and light), ANSI colours, multilingual interface
- ✅ Search across all open files; regex and multi-line search; search in a range
- ✅ Quick colour labels, highlight rules with priorities, rule sets import / export,
  selection highlight, automatic highlighting of IPs, UUIDs, URLs, durations, paths
- ✅ Collapse of repeated lines and stack traces; context lines around matches; hide lines
  by hand; show in context; global filter; boolean filter expressions; field filter terms
- ✅ Bookmarks with notes, rule-driven bookmarks, the Markdown bookmark report
- ✅ Time range filter, timeline histogram, time zones and epoch dates, out-of-order lines
- ✅ Compressed files, zip / tar / 7z entries, standard input, folder sources, rotated files
  as one stream, the clipboard as a stream
- ✅ Remote and system sources: SSH files, journald, Docker / Podman, TCP / UDP / syslog
  listener, OTLP/HTTP receiver, Windows Event Log, Windows debug output, source plugins
- ✅ Structured logs: JSON / logfmt / regex / CSV / TSV fields, user log formats, column view
  with pinned columns, JSON rows as a foldable tree, values with units
- ✅ Analysis: field statistics, the Query tab, Drain pattern grouping, Explain a spike,
  fields plotted over the timeline, the operations timeline, merged timeline of several files
- ✅ Output: copy as shown, export as text / CSV / HTML, split a file, filter as a new tab,
  compare lines, scratchpad, headless `--print` with JSON / logfmt / CSV / template records
- ✅ Sessions, workspace restore, external tools with triggers, desktop notifications, sound
  alerts, tray icon (Windows, Linux), PIN lock, command palette, remappable shortcuts
- ✅ Source locations in stack traces opened in the editor at the line
- ✅ Integrations: read-only web view, MCP server for AI assistants, plugins
- ✅ The same viewer as a native window and as a terminal interface (`fasttail-tui`) on
  Windows, Linux and macOS, with a terminal-only Linux archive for servers

## 6. Gaps, ranked

Value for FastTail's users against effort in this code base (S < 1 week, M 1–3 weeks,
L > 3 weeks). The numbers are stable: other documents and the OpenSpec changes refer to them.

| # | Gap | Who has it | Value | Effort | Status |
|---|---|---|---|---|---|
| 1 | **Structured logs**: JSON / logfmt / regex fields, a column view, hide / show fields | LogExpert, LogViewPlus, lnav, hl, Seq, Loki | High | L | ✅ |
| 2 | **Merged timeline** of several files by timestamp | lnav, Tailviewer, hl, LogViewPlus, Chipmunk | High | L | ✅ |
| 3 | **Context lines** (±N) around filter matches | LogFusion, lnav 0.15 | High | S–M | ✅ |
| 4 | **Field-scoped and boolean filters** (`status>=500`, OR / NOT) | klogg, hl, Seq, Loki, LogViewPlus | High | M | ✅ |
| 5 | **Remote and system sources**: journald, Docker / Kubernetes, SSH / SFTP | lnav, LogExpert, LogViewPlus, nerdlog, lazyjournal, gonzo | Med–High | M–L | ✅ (Kubernetes as `k8s://`) |
| 6 | **Pattern grouping** of similar lines (Drain) | gonzo, Loki, Kibana; requested in LogExpert (#654) | Medium | M | ✅ |
| 7 | **Headless mode**: `fasttail --print` | hl, tailspin, lnav, Chipmunk CLI | Medium | S | ✅ |
| 8 | **Time zones** and epoch dates | hl, LogViewPlus | Medium | S–M | ✅ |
| 9 | **Automatic highlighting** of IPs, UUIDs, URLs, durations, paths | tailspin, lnav | Medium | S | ✅ |
| 10 | **Selection helpers**: every occurrence of the selection; next line of a rule | LogViewPlus, SnakeTail, LogExpert | Medium | S | ✅ |
| 11 | **Rule sets** import / export | LogFusion, LogViewPlus | Medium | S | ✅ |
| 12 | **Windows Event Log** source | SnakeTail, LogFusion, LogViewPlus | Medium | M | ✅ |
| 13 | **TCP / UDP / syslog listener** | LogViewPlus, Chipmunk | Medium | M | ✅ (with an OTLP/HTTP receiver) |
| 14 | **Remappable shortcuts** | klogg, LogViewPlus, lnav | Medium | M | ✅ |
| 15 | **Top-N and statistics** of a field | angle-grinder, goaccess, lnav, LogViewPlus | Medium | M | ✅ |
| 16 | **Markdown report** of bookmarks and notes | logana, lnav | Medium | S | ✅ |
| 17 | MCP server / AI assistance | Seq, gonzo, logana, LogViewPlus | Medium | M | ✅ |
| 18 | CSV / HTML export | LogFusion, LogViewPlus | Low | S | ✅ |
| 19 | Partial-line selection | requested (SnakeTail #60, Tailviewer #231) | Low–Med | M | ✅ |
| 20 | 7z archives, cross-line regex, plugin API, tray icon, OutputDebugString | various | Low | S–L | ✅ (Linux tray icon: hand check on KDE / GNOME ⬜) |
| 21 | **Terminal interface** for servers and SSH sessions | lnav, gonzo, nerdlog, toolong | High | L | ✅ (remaining gaps in section 7) |
| 22 | **Rotated files as one stream** | logana 0.8.2; lnav and hl through globs | Med–High | M | ✅ |
| 23 | **Source locations clickable**, opening the editor at the line | lnav (master), IDE consoles | High | M | ✅ |
| 24 | **Units on field values** (`900ms` < `1.2s`, sizes) | lnav 0.15 | Medium | M | ✅ |
| 25 | **Pinned columns** in the column view | requested in lnav (#674); spreadsheets | Medium | S | ✅ |
| 26 | **Clipboard as a stream** | requested in Chipmunk (#495); editors | Low–Med | S | ✅ |
| 27 | **Machine output of `--print`** (JSON, logfmt, CSV, template) | requested in hl (#611, #1481) | Low–Med | S | ✅ |
| 28 | **Named searches**: several searches highlighted and walked at once | lnav 0.15 | High | M | ✅ (timeline lanes ⬜) |
| 29 | **Next line of the same operation id** | lnav (master, `o` / `O`) | Medium | S | ✅ |
| 30 | **Kubernetes source** (`k8s://`), not only through standard input | gonzo, lazyjournal, lnav | Medium | S–M | ✅ |
| 31 | **Copy as excerpt** for tickets (Markdown, Jira, Slack) | gonzo, logana | Medium | S | ✅ |
| 32 | **Stream information**: indexing time, memory, encoding, parser | lnav 0.15 | Low–Med | S | ✅ |
| 33 | **Pinned and recent presets** | Chipmunk 4.3 | Low–Med | S | ✅ |
| 34 | **Message linked to its logging statement** (`log2src`) | lnav | Low–Med | L | ✅ |
| 35 | **Remote Windows Event Log** | LogFusion | Low–Med | M | ✅ (current credentials) |
| 36 | **Notepad++ filter import** (AnalysePlugin, UDL) | logana 0.7.7 | Low | S | ✅ |
| 37 | **DLT automotive files** | Chipmunk, logana | Low | L | ⬜ `dlt-format` (optional) |

Seen in the competitors and not planned (⬜, to weigh when the maintainer asks):

- ⬜ BLF and PCAP files (Chipmunk; a source plugin can serve them)
- ⬜ Tab groups and a tab strip on the left (LogExpert #53, #142; docking covers it)
- ⬜ AI analysis inside the viewer (gonzo, LogViewPlus; the MCP server lets the user's own
  assistant do it)

## 7. The terminal interface against the window

The terminal interface (`fasttail-tui`, guide in `docs/tui.md`) shares the engine, the
settings and the workspace with the window. Where it stands against the window:

- ✅ Views: text, HEX, ASM, JSON tree of a row; line numbers, encoding, ANSI modes
- ✅ Search (text, regex, multi-line), Find all streams, go to line or time
- ✅ Filters: include / exclude, field terms, presets, global filter, level, time range,
  context lines, collapse, hide lines, show in context, out-of-order lines
- ✅ Highlight rules (all their actions), quick labels, automatic highlighting
- ✅ Bookmarks with notes; sessions; workspace and dock layout
- ✅ Column view with the fields list, pinned columns, one plotted field as a sparkline row
- ✅ Sources: files, archives, standard input, SSH, Docker / journald, listener, Event Log
  and debug output (Windows), rotated files with their pieces, the clipboard with Append
- ✅ Source locations to the editor (`Ctrl+O`), external tools, notifications, sounds, PIN
  lock, themes, languages, `--print` with every output format, `--mcp` over stdio
- ✅ Compare lines and regions
- ✅ Statistics, Query, Patterns, Explain and Operations (tables and text bars)
- ✅ Search results list, search in a range, export (text / CSV / HTML), Copy as shown,
  bookmark report
- ✅ Scratchpad
- ✅ Rule sets import / export, next / previous match of a rule, clear all bookmarks
- ✅ Log formats dialog, editor presets, Open in FastTail from a source location
- ✅ Filter expression toggle per stream, global filter expressions editable
- ✅ Remappable keys, the palette's setting toggles
- ✅ Web view and the MCP server of a running interface; plugin management (enable,
  approve, parameters)
- ✅ Time delta column, time display (as written / UTC / local), the OTLP hint in the
  listener prompt
- ✅ Merged timeline view of several streams (`Alt+M`)
- ✅ Filter as a new tab (`Alt+T`)
- ✅ Time anchor of the Δt column and the elapsed time of a selection (`@`)
- ✅ Palette entries: select all, play / pause all, line numbers, auto-update, save session,
  remove bookmark, open folder pattern, recent files
- ✅ Charts: a volume row (`Alt+V`), sparklines in Statistics and Patterns
- ✅ PgUp / PgDn acceleration while the key is held
- By design not in a terminal: Markdown view, line wrap, overview strip, timeline histogram,
  screensaver, tray icon, renderer / zoom / font settings

## 8. To do

The plan of 2026-10-30: three minors, then the list is empty. Each change is an OpenSpec
change in `openspec/changes/`, one PR each; the changes of 0.32.0 and 0.33.0 land in the
window and the terminal together.

**0.31.0, terminal completion** (section 7), shipped 2026-10-30:

- ✅ `tui-merged-view` (M): merged timeline of several streams in the terminal
- ✅ `tui-filter-tabs` (S): filter as a new tab, saved filter tabs restored
- ✅ `tui-time-anchor` (S): time anchor, elapsed time of a selection
- ✅ `tui-palette-completion` (S): the missing palette entries, recent files, a coverage test
- ✅ `tui-charts` (M): volume row, Statistics and Patterns sparklines
- ✅ `tui-page-acceleration` (S): held PgUp / PgDn accelerate
- ✅ `release-packages` group 5: the packaging docs and the release check (archived)

**0.32.0, search and navigation** (gaps 28 to 33), shipped 2026-10-30:

- ✅ `named-searches` (M), `operation-jump` (S), `stream-info` (S), `copy-excerpt` (S),
  `preset-pinning` (S), `kubernetes-source` (S–M)

**0.33.0, the long tail** (gaps 34 to 37), shipped 2026-10-30:

- ✅ `log2src-links` (L), `remote-event-log` (M), `npp-filter-import` (S)
- ⬜ `dlt-format` (L, optional): not started; feature work stopped after 0.33.0 (only user issues)

Outside the plan:

- ⬜ Hand checks by the maintainer: the 0.29.0 and 0.30.0 terminal features in a real
  terminal, the clipboard, pinned columns, rotated files on Windows, an editor opened from a
  source location, an MCP client, the Plugins windows and source plugins on Windows, the web
  view in a browser, the Linux tray icon on KDE and GNOME (`linux-tray-icon` task 2.1), the
  Event Log on Windows, real journald / Docker / SSH / rsyslog / OTel sources
- ⬜ egui 0.37 (Renovate #330, #335): waits for an `egui_dock` release on it
- ⬜ The next competitor scan (sections 1 to 4), after the next release
