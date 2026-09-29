# Competitor analysis

How FastTail compares with the log viewers people actually use, what they have that
FastTail does not, and what that suggests for the next releases. It is refreshed after
every release (the post-release scan); each refresh replaces the tables and adds a line to
the history at the end.

**Last scan:** 2026-09-28, against FastTail **0.12.0**. Release dates and star counts come
from the GitHub API on that day; every other claim links its source.

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
| **FastTail** | GUI (GPU), TUI planned | MIT | Windows, Linux, macOS | 0.12.0, 2026-09-28 | active |
| BareTail / BareTailPro | GUI | freeware / $35 | Windows | 3.50a (2008) / 2.50a (2006) | abandoned |
| Tailviewer | GUI | MIT | Windows (.NET 4.7.1) | 1.0.0-rc1, 2022-09 | dormant |
| LogExpert (LogExperts fork) | GUI | MIT | Windows (.NET 10) | 1.43.0, 2026-08-28 | very active |
| klogg / glogg | GUI | GPLv3 | Windows, Linux, macOS | 22.06, 2022-06 | unmaintained |
| SnakeTail | GUI | GPLv3 | Windows (.NET 2.0) | 1.9.8, 2024-02 | quiet |
| LogViewPlus | GUI | $45 / $95 per user | Windows | 3.2.9, 2026-05 | active |
| LogFusion | GUI | free personal / Pro $19–$899 | Windows | 7.2, 2026-07 | active |
| lnav | TUI | BSD-2 | Linux, macOS, BSD | 0.14.1, 2026-09-05 | very active (10.7k ★) |
| tailspin | CLI highlighter | MIT (Rust) | all | 7.0.0, 2026-07 | active (8k ★) |
| hl | CLI formatter | MIT (Rust) | all | 0.36.3, 2026-06 | active |
| gonzo | TUI dashboard | MIT (Go) | all | 0.4.3, 2026-07 | fast-growing (2.8k ★ in 13 months) |
| nerdlog | TUI, multi-host SSH | BSD-2 | Linux, macOS | 1.12.0, 2026-09-26 | active |
| toolong | TUI | MIT (Python) | all | 1.4.0, 2024-03 | dormant |
| Seq | server + web UI | proprietary, free single user | Windows, Docker | 2026.1 | active |
| Grafana Loki / Drilldown | server + web UI | AGPL-3.0 | server | 3.7.8, 2026-09-17 | active |

Also watched: Chipmunk (Rust/Electron, DLT / PCAP, 4.4.0 2026-09), LogFX, LogoRRR, LogMX,
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
readable Unix timestamps (#139).

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
error jumps, Kitty keyboard protocol.

**tailspin** — zero-configuration highlighting of dates, levels, URLs, IPs, UUIDs, paths and
key=value pairs; pages through `less` ([GitHub](https://github.com/bensadeh/tailspin)).
8k stars for highlighting alone.

**hl** — JSON / logfmt formatter with field filters (`-f`, `-q` boolean queries),
`--since -3h`, time-zone conversion, time-ordered merge at ~2 GiB/s
([GitHub](https://github.com/pamburus/hl)).

**gonzo** — k9s-style dashboard: severity and time charts, heatmap, **Drain pattern
clustering**, optional AI analysis, stdin / files / Kubernetes / OTLP inputs, vim keys and
mouse ([GitHub](https://github.com/control-theory/gonzo)).

**nerdlog** — tails many hosts over SSH with no server, histogram on top; on its Show HN the
most requested feature was journald ([HN](https://news.ycombinator.com/item?id=43750765)).

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
  Chipmunk, LogoRRR); FastTail's `merged-timeline-view` proposal is still pending.
- **Sources beyond local files**: journald, Docker / Kubernetes, SSH / SFTP, TCP / syslog.
- **Patterns and summaries**: grouping similar lines that are not adjacent (Drain), top-N
  values, charts.
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

## 6. Gaps, ranked

Value for FastTail's users against effort in this code base (S < 1 week, M 1–3 weeks,
L > 3 weeks).

| # | Gap | Who has it | Value | Effort |
|---|---|---|---|---|
| 1 | **Structured logs**: parse JSON / logfmt / regex with named groups into fields, a column view, hide / show fields | LogExpert, LogViewPlus, lnav, hl, Seq, Loki | High | L |
| 2 | **Merged timeline** of several files by timestamp | lnav, Tailviewer, hl, LogViewPlus, Chipmunk | High | L (proposal exists) |
| 3 | **Context lines** (±N) around filter matches, `grep -C` style | LogFusion | High | S–M |
| 4 | **Field-scoped and boolean filters** (`status>=500`, OR / NOT) | klogg, hl, Seq, Loki, LogViewPlus | High | M |
| 5 | **Remote and system sources**: journald, Docker / Kubernetes, SSH / SFTP | lnav, LogExpert, LogViewPlus, nerdlog, lazyjournal | Med–High | M–L |
| 6 | **Pattern grouping** of similar lines anywhere in the file (Drain) | gonzo, Loki, Kibana | Medium | M |
| 7 | **Headless mode**: `fasttail --print` filters / highlights to stdout | hl, tailspin, lnav | Medium | S |
| 8 | **Time zones**: show timestamps in UTC / local / a chosen zone; epoch as dates | hl, LogViewPlus | Medium | S–M |
| 9 | **Automatic highlighting** of IPs, UUIDs, URLs, durations, paths | tailspin, lnav | Medium | S |
| 10 | **Selection helpers**: highlight every occurrence of the selected text; jump to the next line of a given rule | LogViewPlus, SnakeTail | Medium | S |
| 11 | **Rule sets** import / export (share highlight rules) | LogFusion, LogViewPlus | Medium | S |
| 12 | **Windows Event Log** source | SnakeTail, LogFusion, LogViewPlus | Medium | M |
| 13 | **TCP / UDP / syslog listener** | LogViewPlus, Chipmunk | Medium | M |
| 14 | **Remappable shortcuts** | klogg, LogViewPlus, lnav | Medium | M |
| 15 | **Top-N and statistics** of a field over the view | angle-grinder, goaccess, lnav, LogViewPlus | Medium | M |
| 16 | **Markdown report** of bookmarks and notes (incident write-up) | logana, lnav | Medium | S |
| 17 | MCP server / AI assistance | Seq, gonzo, logana, LogViewPlus | Medium | M |
| 18 | CSV / HTML export | LogFusion, LogViewPlus | Low | S |
| 19 | Partial-line selection | requested (SnakeTail #60, Tailviewer #231) | Low–Med | M |
| 20 | 7z archives, cross-line regex, plugin API, tray icon, OutputDebugString | various | Low | S–L |

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

These points are to be folded into the `tui-interface` proposal (PR #132) before it is merged.

## 8. Release plan: closing the gap

The maintainer asked for an OpenSpec change for every feature the competitors have and
FastTail lacks, spread over three releases so the whole gap is closed; 0.20.0 stays
reserved for the terminal interface (PR #132). Each change lives in
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

| Change | Gaps | Effort |
|---|---|---|
| `relative-time-windows` — "last 15 min / 1 h / 24 h", `--since -3h` | hl | S |
| `partial-line-selection` — select part of a line | 19 | M |
| `scratchpad` — notes tab lines can be sent to | klogg | S |
| `filter-to-tab` — the filtered view as a new following tab | LogExpert | M |
| `compare-lines` — diff of two lines or two regions | requested | M |
| `tray-icon` — minimise to tray, tray menu | SnakeTail | S–M |
| `structured-fields` — JSON / logfmt / regex fields, column view, field filters | 1, 4 | L |
| `custom-log-formats` — user-defined formats (named groups, timestamp, multiline start) | 1 | M |
| `boolean-filter-expressions` — AND / OR / NOT, field operands | 4 | M |
| `merged-timeline-view` — several files merged by timestamp | 2 | L |
| `folder-source` — several patterns, subfolders, "open all" | Tailviewer, LogFusion | M |
| `pattern-grouping` — Drain patterns, CTRL + SHIFT + G | 6 | M |
| `field-statistics` — top-N and statistics of a field | 15 | M |
| `spike-explanation` — what is different in a histogram spike | Loki, Kibana | M |
| `operation-timeline` — Gantt of operations by an id field | lnav | M |
| `cross-line-regex` — regex across lines (plus single-line regex search) | 20 | M |
| `export-formats` — CSV and HTML export | 18 | S |

### 0.15.0 — sources and integrations

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

0.14.0 is the heaviest (two L changes, plus the six carried over from 0.13.0); if it runs long, `operation-timeline` and
`cross-line-regex` are the first to move to 0.15.0. Open questions are listed in each
change's design.

## 9. Scan history

| Date | FastTail | Outcome |
|---|---|---|
| 2026-09-24 | 0.9.x | first scan; led to `timestamp-range-filter` and `merged-timeline-view` |
| 2026-09-27 | 0.11.0 | led to show in context, bookmark notes and triggers, more archive formats, collapse repeated lines (all shipped in 0.12.0) |
| 2026-09-28 | 0.12.0 | this document; 36 changes over 0.13.0–0.15.0 (section 8) |
