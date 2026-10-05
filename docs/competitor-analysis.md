# Competitor analysis

How FastTail compares with the log viewers people actually use, what they have that
FastTail does not, and what that suggests for the next releases. It is refreshed after
every release (the post-release scan); each refresh replaces the tables and adds a line to
the history at the end.

**Last scan:** 2026-10-05, against FastTail **0.20.0**. Release dates and star counts come
from the GitHub API on that day; every other claim links its source. What changed since the
previous scan (2026-09-28, 0.12.0) is summarised in [section 9](#9-scan-history).

## Contents

1. [The field in one table](#1-the-field-in-one-table)
2. [Desktop viewers](#2-desktop-viewers)
3. [Terminal and server-side tools](#3-terminal-and-server-side-tools)
4. [Market signals](#4-market-signals)
5. [What FastTail already covers](#5-what-fasttail-already-covers)
6. [Gaps, ranked](#6-gaps-ranked)
7. [Lessons for the terminal interface (0.20.0)](#7-lessons-for-the-terminal-interface-0200)
8. [Release plan: closing the gap](#8-release-plan-closing-the-gap)
9. [Scan history](#9-scan-history)

## 1. The field in one table

| Product | Kind | Licence / price | Platforms | Last release | Activity |
|---|---|---|---|---|---|
| **FastTail** | GUI (GPU) and TUI (`fasttail-tui`) | MIT | Windows, Linux, macOS | 0.20.0, 2026-10-05 | active |
| BareTail / BareTailPro | GUI | freeware / $35 | Windows | 3.50a (2006-11-02) / 2.50a (2006) | abandoned |
| Tailviewer | GUI | MIT | Windows (.NET 4.7.1) | 1.0.0-rc1, 2022-09-12 | dormant (last commit 2022-09) |
| LogExpert (LogExperts fork) | GUI | MIT | Windows (.NET 10) | 1.50.0, 2026-10-03 | very active (1.2k ★) |
| klogg | GUI | GPLv3 | Windows, Linux, macOS | 22.06, 2022-06 (continuous builds to 2024-11-26) | unmaintained (3.5k ★) |
| glogg | GUI | GPLv3 | Windows, Linux, macOS | no release or commit since 2021-05 | abandoned |
| SnakeTail | GUI | GPLv3 | Windows (.NET 2.0) | 1.9.8, 2024-02-29 | quiet |
| LogViewPlus | GUI | $45 / $95 per user | Windows | 3.2.9, 2026-05 | active |
| LogFusion | GUI | free personal / Pro $19–$899 | Windows | 7.2, 2026-07-21 | active |
| lnav | TUI | BSD-2 | Linux, macOS, BSD | 0.14.1, 2026-09-05; 0.15.0-beta1, 2026-09-28 | very active (10.7k ★) |
| tailspin | CLI highlighter | MIT (Rust) | all | 7.0.0, 2026-07-14 | active (8k ★) |
| hl | CLI formatter | MIT (Rust) | all | 0.36.3, 2026-06-12 | active (3.3k ★; only dependency updates since) |
| gonzo | TUI dashboard | MIT (Go) | all | 0.4.3, 2026-07-15 | active (2.8k ★) |
| nerdlog | TUI, multi-host SSH | BSD-2 | Linux, macOS | 1.13.0, 2026-10-03 | active (weekly releases) |
| toolong | TUI | MIT (Python) | all | 1.4.0, 2024-03 | dormant (4k ★) |
| Seq | server + web UI | proprietary, free single user | Windows, Docker | 2026.1 (no 2026.2 found) | active |
| Grafana Loki / Drilldown | server + web UI | AGPL-3.0 | server | 3.7.8, 2026-09-17 | active |

Also watched: Chipmunk (Rust/Electron, DLT / PCAP, 4.3.0 2026-09-18 and 4.4.0 2026-09-29), LogFX, LogoRRR, LogMX,
LogSquirl (a klogg fork with a table view, beta), angle-grinder, logdy, lazyjournal,
otel-tui, logana (a young Rust TUI close to FastTail's TUI plan).

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
  FastTail shipped in 0.13.0;
- **named searches**: several searches stay highlighted at once, each with its own colour,
  `n` / `N` across all of them or `.` / `,` to focus one, a row per search in the timeline,
  saved in the session;
- CSV / TSV as **tabular formats** with separator and header detection (quoted multi-line
  cells joined), and a `metrics_log` format whose numeric columns can be drawn as
  **sparklines over the timeline** (up to four);
- per-column statistics in the details overlay (min..max, p50 / p90 / p99, distinct count);
- `lnav file split`, cutting a large log into pieces by lines, size or time;
- arrow keys moving by field rather than half a screen, and indexing of several files in
  parallel.

**tailspin** — zero-configuration highlighting of dates, levels, URLs, IPs, UUIDs, paths and
key=value pairs; pages through `less` ([GitHub](https://github.com/bensadeh/tailspin)).
8k stars for highlighting alone.

**hl** — JSON / logfmt formatter with field filters (`-f`, `-q` boolean queries),
`--since -3h`, time-zone conversion, time-ordered merge at ~2 GiB/s
([GitHub](https://github.com/pamburus/hl)).

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
  Chipmunk, LogoRRR); FastTail's `merged-timeline-view` is planned for 0.21.0.
- **Context lines** became standard in 2026: lnav 0.15 (`:filter-context`) followed
  LogFusion and FastTail 0.13.0.
- **Tabular and metric data**: lnav 0.15 reads CSV / TSV as a format and draws numeric
  columns as sparklines; logs are no longer the only text people open in a log viewer.
- **Malformed input is shown, not dropped**: nerdlog 1.12 keeps out-of-order and
  unparseable records visible and marks them.
- **Sources beyond local files**: journald, Docker / Kubernetes, SSH / SFTP, TCP / syslog.
- **Patterns and summaries**: grouping similar lines that are not adjacent (Drain), top-N
  values, charts. Drain is now requested in LogExpert too (#654).
- **AI and MCP**: Seq, gonzo, logana and LogViewPlus added AI prompts or MCP servers in 2026.
- **Basics still win**: tailspin's growth shows zero-configuration highlighting sells; the
  top requests of older viewers (wrap, dark theme, all-files search, ANSI) are table stakes.

## 5. What FastTail already covers

Items often requested elsewhere that FastTail has: word wrap, themes (dark and light),
search across all open files, ANSI colours, quick colour labels, collapse of repeated lines
and stack traces, bookmarks with notes and rule-driven bookmarks, time range filter with a
histogram, show in context, global filter, compressed files and tar / zip entries, stdin,
sessions, external tools with triggers, the `[N]` new-lines badge on background tabs, HEX
and Markdown views, level detection and filtering.

Added since the previous scan (0.13.0 to 0.20.0): context lines around matches, headless
`--print`, selection highlight and rule walk, rule-set import / export, time zones and
epoch dates, automatic highlighting, search in a range, the bookmark report, the command
palette, 7z, relative time windows, partial-line selection, the scratchpad, filter as a new
tab (saved in the workspace and sessions since 0.16.2), compare lines, the Windows tray
icon, structured fields with a column view, **JSON rows as a foldable tree** (0.16.2), and
the **terminal interface** `fasttail-tui` at parity with the window (0.20.0), with a Linux
terminal-only archive for servers without graphical libraries. FastTail is now the only
tool in section 1 that offers the same viewer as a native GUI and as a TUI on Windows,
Linux and macOS.

## 6. Gaps, ranked

Value for FastTail's users against effort in this code base (S < 1 week, M 1–3 weeks,
L > 3 weeks).

The numbers are stable: other sections and the OpenSpec changes refer to them. The status
column says where each gap stands after 0.20.0.

| # | Gap | Who has it | Value | Effort | Status after 0.20.0 |
|---|---|---|---|---|---|
| 1 | **Structured logs**: parse JSON / logfmt / regex with named groups into fields, a column view, hide / show fields | LogExpert, LogViewPlus, lnav, hl, Seq, Loki | High | L | partly closed: `structured-fields` (0.14.0), JSON tree (0.16.2); field terms in 0.21.0, user formats in 0.23.0 |
| 2 | **Merged timeline** of several files by timestamp | lnav, Tailviewer, hl, LogViewPlus, Chipmunk | High | L | 0.21.0 |
| 3 | **Context lines** (±N) around filter matches, `grep -C` style | LogFusion, lnav 0.15 | High | S–M | closed, 0.13.0 |
| 4 | **Field-scoped and boolean filters** (`status>=500`, OR / NOT) | klogg, hl, Seq, Loki, LogViewPlus | High | M | 0.21.0 (field terms), 0.23.0 (boolean) |
| 5 | **Remote and system sources**: journald, Docker / Kubernetes, SSH / SFTP | lnav, LogExpert, LogViewPlus, nerdlog, lazyjournal, gonzo | Med–High | M–L | 0.22.0 |
| 6 | **Pattern grouping** of similar lines anywhere in the file (Drain) | gonzo, Loki, Kibana; requested in LogExpert (#654) | Medium | M | 0.23.0 |
| 7 | **Headless mode**: `fasttail --print` filters / highlights to stdout | hl, tailspin, lnav, Chipmunk CLI | Medium | S | closed, 0.13.0 |
| 8 | **Time zones**: show timestamps in UTC / local / a chosen zone; epoch as dates | hl, LogViewPlus | Medium | S–M | closed, 0.13.0 |
| 9 | **Automatic highlighting** of IPs, UUIDs, URLs, durations, paths | tailspin, lnav | Medium | S | closed, 0.13.0 |
| 10 | **Selection helpers**: highlight every occurrence of the selected text; jump to the next line of a given rule | LogViewPlus, SnakeTail, LogExpert | Medium | S | closed, 0.13.0 |
| 11 | **Rule sets** import / export (share highlight rules) | LogFusion, LogViewPlus | Medium | S | closed, 0.13.0 |
| 12 | **Windows Event Log** source | SnakeTail, LogFusion, LogViewPlus | Medium | M | 0.22.0 |
| 13 | **TCP / UDP / syslog listener** | LogViewPlus, Chipmunk | Medium | M | 0.22.0 |
| 14 | **Remappable shortcuts** | klogg, LogViewPlus, lnav | Medium | M | 0.22.0 |
| 15 | **Top-N and statistics** of a field over the view | angle-grinder, goaccess, lnav (per-column stats in 0.15), LogViewPlus | Medium | M | 0.23.0 |
| 16 | **Markdown report** of bookmarks and notes (incident write-up) | logana, lnav | Medium | S | closed, 0.13.0 |
| 17 | MCP server / AI assistance | Seq, gonzo, logana, LogViewPlus | Medium | M | 0.22.0 |
| 18 | CSV / HTML export | LogFusion, LogViewPlus | Low | S | 0.23.0 |
| 19 | Partial-line selection | requested (SnakeTail #60, Tailviewer #231) | Low–Med | M | closed, 0.14.0 |
| 20 | 7z archives, cross-line regex, plugin API, tray icon, OutputDebugString | various | Low | S–L | 7z closed (0.13.0), tray icon on Windows (0.14.0); Linux tray 0.21.0, OutputDebugString and plugin API 0.22.0, cross-line regex 0.23.0 |
| 21 | **Terminal interface** for servers and SSH sessions | lnav, gonzo, nerdlog, toolong | High | L | closed, 0.20.0 (`fasttail-tui`, Linux terminal-only archive) |

### New candidates for 0.24.0

Found in the 2026-10-05 scan and assigned to 0.24.0 by the maintainer the same evening, each
now an OpenSpec change (section 8): `open-at-line`, `hide-lines`, `tabular-files`,
`field-sparkline`, `split-log`, `out-of-order-lines`, `tui-find-all`.

| Candidate | Who has it | Value | Effort |
|---|---|---|---|
| **Open at a line** from the command line (`fasttail app.log:1204` or `--line N`), for editors and IDEs that link to log lines | LogExpert 1.50 (#58), Chipmunk "Open with" | Medium | S |
| **Hide selected lines** by hand, without writing an exclude term; listed and restorable, saved with the stream | LogExpert 1.50 (#338) | Medium | S |
| **Tabular files**: CSV / TSV opened as columns with header and separator detection, quoted multi-line cells joined into one row (builds on the `structured-fields` column view) | lnav 0.15, LogExpert CSV columnizer | Medium | M |
| **Numeric field over time**: a sparkline of a numeric field (latency, memory) drawn over the timeline histogram (builds on `field-statistics`) | lnav 0.15 metrics, gonzo charts | Medium | M |
| **Split a large log** by lines, size or time into new files (today: a time range plus **Export visible lines**, one piece at a time) | lnav 0.15 `file split` | Low | S |
| **Out-of-order lines marked**: a line whose timestamp is earlier than the previous one is flagged in the gutter, so the time filter and the histogram do not hide a clock jump | nerdlog 1.12 | Low–Med | S |
| **Search all streams in the terminal** (the window's `Ctrl+Shift+F` Find results), left out of 0.20.0 by the `terminal-interface` spec; already an open question for the maintainer | FastTail window; Chipmunk "find in search results" | Medium | M |

Checked and already covered: lnav's named searches (several patterns highlighted at once,
walked one by one) match FastTail's highlight rules, quick labels and rule walk; LogExpert's
overview marker bar matches the overview strip; Chipmunk's pinned presets and CLI presets
match the filter presets and `--print`.

## 7. Lessons for the terminal interface (0.20.0)

- **Keys**: less / vim by default (`j`/`k`, `g`/`G`, `/`, `?`, `n`/`N`, `Space`/`b`, `q`,
  `F` follow, counts), lnav's `e`/`E` and `w`/`W` for errors and warnings, FastTail's F-keys
  as aliases. toolong was criticised for keys that surprised vim users.
- **Discoverability**: a key-hint bar that stays, `?` / `F1` help even with no file open, a
  command palette (`:`).
- **Layout**: status on top (file, level counts, time span), rows, an optional histogram
  strip; FastTail's overview strip as a gutter column. Avoid dashboards that hide the log.
- **Pipes and headless use**: `cmd | fasttail-tui -`, and the headless print mode (gap 7).
- **Avoid**: CPU-burning polling (toolong #17), losing the terminal's copy (`y` + OSC 52 and
  a mouse-off mode — already in the prototype).
- **Terminal support**: mouse, the Kitty keyboard protocol, colours that degrade from
  truecolor to 16, rebindable keys.

These points are folded into the `tui-interface` proposal (PR #132, design decision 7 and
the Terminal Key Conventions requirement). It shipped in 0.20.0 (archived as the
`terminal-interface` spec; guide in `docs/tui.md`); remappable keys stay with
`remappable-shortcuts` (0.22.0).

## 8. Release plan: closing the gap

The maintainer asked for an OpenSpec change for every feature the competitors have and
FastTail lacks, spread over three releases (four since 0.13.0 and 0.14.0 shipped early, each
moving its unfinished changes to the next) so the whole gap is closed; 0.20.0, the
terminal interface (PR #132), comes right after 0.14.0 and the gap resumes with 0.21.0. Each change lives in
`openspec/changes/<name>/` with proposal, design, tasks and specs; efforts use the scale of
section 6. The order inside a release follows the dependencies.

### 0.13.0 — basics and quick wins

Closed on 2026-09-29 with the seven changes below; `relative-time-windows`,
`filter-to-tab`, `compare-lines`, `partial-line-selection`, `scratchpad` and `tray-icon`
moved to 0.14.0 (first in its table), so the release was not held back.

| Change | Gaps | Effort |
|---|---|---|
| `context-lines` — ±N lines around filter matches | 3 | S–M |
| `quick-wins-0-13` — selection highlight, next / previous line of a rule, rule-set import / export, epoch and time zones, automatic highlighting | 8, 9, 10, 11 | M (five S items) |
| `headless-print` — `fasttail --print` to stdout | 7 | S |
| `search-in-range` — find limited to a line / time range or the selection | klogg | S |
| `bookmark-report` — Markdown report of bookmarks and notes | 16 | S |
| `command-palette` — CTRL + SHIFT + P over every action; creates the action registry | lnav, editors | M |
| `archive-7z` — 7z entries like zip / tar | 20 | S |

### 0.14.0 — structured logs and analysis

Closed on 2026-09-29 with the changes below; the rest of the planned 0.14.0 moved to
0.21.0, after the terminal interface, so the release was not held back.

| Change | Gaps | Effort |
|---|---|---|
| `relative-time-windows` — "last 15 min / 1 h / 24 h", `--since -3h` | hl | S |
| `partial-line-selection` — select part of a line | 19 | M |
| `scratchpad` — notes tab lines can be sent to | klogg | S |
| `filter-to-tab` — the filtered view as a new following tab (saving the tabs: 0.16.2) | LogExpert | M |
| `compare-lines` — diff of two lines or two regions | requested | M |
| `tray-icon` — minimise to tray, tray menu, Windows (Linux: `linux-tray-icon`, 0.21.0) | SnakeTail | S–M |
| `structured-fields` — JSON / logfmt / regex parsers with detection, column view | 1 | L |

### 0.15.0 — terminal interface preview, then nightly patches

Released on 2026-09-30 at the maintainer's request: the Commander theme and `fasttail-tui`
as a preview in every archive (dock, dialogs, themes, mouse, file browser; editors, lock,
hand-off and translations still missing). From then on a patch release (`0.15.x`) is cut
every evening after 18:00 when something was merged (AGENTS.md, release process).

| Change | Gaps | Effort |
|---|---|---|
| `release-packages` — `.deb` / `.rpm` / AppImage (Linux x86_64 and ARM64), `.msi` (Windows), universal `.dmg` and Intel archive (macOS), each install-tested in CI | RustDesk-style platform coverage (maintainer request) | M |

### 0.16.0 — terminal windows

Released on 2026-09-30 in the evening, a minor at the maintainer's request after testing
the preview build:
- floating terminal windows that move, resize and overlap;
- `[x]` to close a window;
- the empty workspace;
- a help that shows every key;
- the bookmark report export fix;
- the downloads grid on the release page.

Nightly patches continue as `0.16.x`; `release-packages` moves with them.

### 0.20.0 — terminal interface

Released on 2026-10-05; `tui-interface` is archived.


After 0.14.0 the maintainer moved the terminal interface forward: 0.20.0 holds only
`tui-interface` (PR #132, the TUI at parity with the window, HEX view included, Markdown
excluded), completed through the nightly patches (0.15.x, then 0.16.x). The gap changes below resume after it.

### 0.21.0 — structured logs, sources from folders, binary views

Re-planned by the maintainer on 2026-10-05: the disassembly view moves here from 0.23.0;
user-defined formats, boolean filters, cross-line regex, export formats and the statistics
group move to 0.23.0.

| Change | Gaps | Effort |
|---|---|---|
| `structured-field-terms` — field filter terms, level and time from fields, cell spans, copy as shown | 1, 4 | M |
| `merged-timeline-view` — several files merged by timestamp | 2 | L |
| `folder-source` — several patterns, subfolders, "open all" | Tailviewer, LogFusion | M |
| `disassembly-view` — ASM view of a stream: x86 16 / 32 / 64-bit, PE and ELF entry point and sections (a maintainer request; hex editors with a disassembly pane such as Hiew and 010 Editor have it; it extends the HEX view in the window and in the terminal) — **shipped early in 0.20.1** | requested | M |
| `linux-tray-icon` — the Linux tray backend left open by `tray-icon` | — | S |

### 0.22.0 — sources and integrations

| Change | Gaps | Effort |
|---|---|---|
| `system-sources` — journald, Docker / Kubernetes | 5 | M–L |
| `ssh-sources` — SSH / SFTP files | 5 | M |
| `windows-event-log` — Event Log source | 12 | M |
| `network-listener` — TCP / UDP / syslog | 13 | M |
| `otlp-receiver` — OTLP logs on top of the listener | gonzo | S |
| `debug-output-capture` — Windows OutputDebugString | 20 | S |
| `rule-notifications` — desktop notification when a rule matches | LogExpert | S |
| `query-language` — SQL-like queries over fields | LogViewPlus, lnav | L |
| `remappable-shortcuts` — key bindings on the 0.13.0 action registry | 14 | M |
| `mcp-server` — read-only MCP server for AI assistants, off by default | 17 | M |
| `web-ui` — read-only local web view, off by default | Seq, Loki | M |
| `plugin-api` — out-of-process source plugins, bundled formats | 20 | L |

### 0.23.0 — formats, filters and statistics (postponed from 0.21.0)

| Change | Gaps | Effort |
|---|---|---|
| `custom-log-formats` — user-defined formats (named groups, timestamp, multiline start) | 1 | M |
| `boolean-filter-expressions` — AND / OR / NOT, field operands | 4 | M |
| `cross-line-regex` — regex across lines (plus single-line regex search) | 20 | M |
| `export-formats` — CSV and HTML export | 18 | S |
| `field-statistics` — top-N and statistics of a field | 15 | M |
| `pattern-grouping` — Drain patterns, CTRL + SHIFT + G | 6 | M |
| `spike-explanation` — what is different in a histogram spike | Loki, Kibana | M |
| `operation-timeline` — Gantt of operations by an id field | lnav | M |

### 0.24.0 — scan candidates of 2026-10-05

The seven candidates of section 6, assigned by the maintainer on 2026-10-05.
`field-sparkline` comes after `field-statistics` (0.23.0).

| Change | Who has it | Effort |
|---|---|---|
| `open-at-line` — `fasttail app.log:1204`, `--line N`, also for `--print` | LogExpert, Chipmunk | S |
| `hide-lines` — hide selected lines by hand, listed, restorable, saved per stream | LogExpert | S |
| `tabular-files` — CSV / TSV as columns, header and separator detection, quoted multi-line cells | lnav, LogExpert | M |
| `field-sparkline` — a numeric field plotted over the timeline histogram | lnav, gonzo | M |
| `split-log` — split a file by lines, size or time | lnav | S |
| `out-of-order-lines` — backward clock jumps marked, counted and walked | nerdlog | S |
| `tui-find-all` — Search all streams in the terminal interface | FastTail window, Chipmunk | M |

Open questions are listed in each change's design.

## 9. Scan history

| Date | FastTail | Outcome |
|---|---|---|
| 2026-09-24 | 0.9.x | first scan; led to `timestamp-range-filter` and `merged-timeline-view` |
| 2026-09-27 | 0.11.0 | led to show in context, bookmark notes and triggers, more archive formats, collapse repeated lines (all shipped in 0.12.0) |
| 2026-09-28 | 0.12.0 | this document; 36 changes over 0.13.0–0.15.0 (section 8) |
| 2026-10-05 | 0.20.0 | after the terminal interface. New releases: LogExpert 1.50.0 (open at a line, hide lines, overview marker bar), lnav 0.15.0-beta1 (filter context, named searches, CSV / metrics with timeline sparklines, column statistics, `file split`), nerdlog 1.12–1.13 (out-of-order and malformed records shown), Chipmunk 4.3–4.4; no release from BareTail, Tailviewer, klogg, glogg, SnakeTail, toolong, hl, tailspin, gonzo, LogViewPlus, LogFusion or Seq since the previous scan. Gaps 3, 7–11, 16, 19 and the terminal interface (new row 21) marked closed; seven candidates for 0.24.0 or later listed in section 6; section 8 unchanged |
