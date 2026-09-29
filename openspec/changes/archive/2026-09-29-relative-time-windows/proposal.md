## Why

The time range popup (0.12.0) takes absolute times, and its "Last hour" shortcut means the
last hour *of the log*. When tailing a live service the question is usually "what
happened in the last 15 minutes", relative to now, and it should keep meaning that while
the user watches: an absolute window typed at 14:00 is stale by 14:20. Grafana, Kibana and
Seq default to relative ranges that slide; hl takes `--since -3h` on the command line.
FastTail's `headless-print` change already specifies relative `--since` for print mode;
the window has no equivalent.

## What Changes

- The time range fields accept **relative times**: `-15m`, `-90s`, `-3h`, `-2d`, `-1w`,
  combined forms such as `-1h30m`, and `now`, counted back from the current time on the
  log's clock. `from -15m` with an empty "to" is "the last 15 minutes".
- A window with a relative side is **live**: its bounds are re-evaluated every 5 seconds
  while it is set, lines that fall out of it disappear and appended lines inside it
  appear, without the user touching the popup.
- The popup gets a **Relative to now** row of shortcuts: 5 min, 15 min, 1 h, 6 h, 24 h,
  7 d (filling `from -Nm/h/d`, "to" empty). The existing **Last hour** shortcut keeps its
  behaviour and is renamed **Last hour of the log** so the two are not confused.
- The time span control shows `⟳` before the span while the window is live, and its
  tooltip gives the relative text and the bounds currently applied. When a live window
  holds no line because the log's last timestamp is older, the control says so and gives
  the time of the last line.
- "Now" is the local clock, or, once `quick-wins-0-13` ships, the current time in the
  stream's **source zone**, so a log written in UTC is compared with UTC now.
- **Command line**: `--since <TIME>` and `--until <TIME>` also work **without `--print`**,
  setting the time window of the streams opened from the command line (standard input
  included) as if typed in the popup; relative values give a live window. The syntax is
  shared with `headless-print`, which currently makes these options a usage error without
  `--print`; whichever change lands second aligns the two.
- Filter presets that save the time range keep the relative text, so a preset "Errors,
  last hour" stays relative. No new key in `fasttail.ini`.

Target release: **0.14.0** (planned for 0.13.0, moved when 0.13.0 shipped early), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **S**.

### Non-goals

- Relative times anchored to the log's end (`end-15m`) other than the existing shortcut;
  see the design's open questions.
- Named time zones (as `quick-wins-0-13`: local, UTC, fixed offsets).
- A global time window shared by every stream.
- Persisting the time window in the workspace or session files (it is not persisted
  today either).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filters-and-highlighting`: adds Relative Time Windows and Time Window Sliding Cost;
  modifies Time range shortcuts (relative row, renamed "Last hour of the log").
- `command-line`: adds Time Window Options (`--since`, `--until` in window mode).

## Impact

- `src/timestamp.rs`: `parse_relative(text, now) -> Option<i64>` used by
  `parse_user_time` (a leading `-` or `now`); shared with `headless-print`.
- `src/tail_engine.rs`: a window with a relative side (`PendingWindow::Texts` keeps the
  text); re-evaluation every 5 s; when the stream's timestamps are non-decreasing, moving
  bounds trim or extend `filtered_lines` at the ends instead of refiltering; otherwise a
  refilter (in the background above 16 MB) at most once a minute.
- `src/ui/time_range.rs`: relative shortcut row, renamed shortcut, `⟳` label, tooltip
  and empty-window hint; `src/ui/app.rs`: a repaint every 5 s while a live window is set.
- `src/cli.rs`: `--since`, `--until`; `src/main.rs` / `src/ui/app.rs`: applied to the
  streams opened from the command line.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
- **TUI (0.20.0, PR #132)**: the parser and the live window are engine code; the TUI's
  time range prompt accepts the same text, and `--since` / `--until` apply to it too.
