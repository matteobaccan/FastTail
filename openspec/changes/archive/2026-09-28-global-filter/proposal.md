## Why

With several logs open, hiding the same noise everywhere (health checks, one chatty
component) or keeping only one request id across every file means typing the same terms
into each stream bar, or saving a preset and pressing "all" again after every change. A
stream opened later starts unfiltered, and a change on one stream does not reach the
others. The maintainer asked for a filter that applies to all streams at once, like the
search across streams (asked 2026-09-27, after testing the 0.11.0 preview).

## What Changes

- A **global filter**: up to 8 include terms and 8 exclude terms, with their own
  case-sensitive and regex toggles, edited in a global filter bar shown under the menu
  bar (toggled from the toolbar and with `CTRL + SHIFT + H`).
- It applies **on top of each stream's own filter**: a line is visible when it passes the
  stream's terms, level and time filters *and* contains every global include term and no
  global exclude term. A stack-trace continuation line follows its entry as it does today.
- It applies to **every open stream and to streams opened later**, standard input and
  compressed streams included; HEX view is unaffected, as for stream filters.
- An **on / off switch** suspends it without losing its terms; while it is on, every stream
  bar shows a `🌐` badge whose tooltip lists the global terms, so a hidden condition is
  never invisible.
- The **Find results** tab searches the lines each stream shows, so it honours the global
  filter with no change of its own; the line counters, overview strip and search pane
  follow the visible lines as today.
- Persisted in `fasttail.ini` as a new `[global_filter]` section: `enabled`,
  `case_sensitive`, `regex`, `include.1`…`include.8`, `exclude.1`…`exclude.8` (values
  written with `filter_preset::ini_value`, so quotes and edge spaces survive).

Target release: **0.12.0**.

### Non-goals

- Per-stream opt-out ("ignore the global filter on this stream"): possible later, not in
  this change.
- Linked stream filters (editing one stream's terms changing the others).
- Global minimum level or global time range: the global filter is text only.
- Saving the global filter in session files or in filter presets: it is a preference of
  the installation, like the search lane.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `filters-and-highlighting`: adds the Global Filter requirement (terms, combination with
  each stream's filter, on/off switch, badge, persistence, streams opened later).

## Impact

- `src/scan_job.rs`: `FilterSpec` gains an optional global term set with its own case and
  regex flags, evaluated in `matches` / `visible_in_sequence`, so the synchronous path,
  the background filter and search jobs and the Find results jobs all see it.
- `src/tail_engine.rs`: the engine holds the current global set and recomputes its
  filter when it changes (same path as `set_filter_terms`); `is_filter_active` accounts
  for it.
- `src/ui/app.rs`: global filter bar, toolbar toggle, `CTRL + SHIFT + H` consumed before the
  dock (as `CTRL + SHIFT + F` is), propagation to every engine, persistence.
- `src/ui/dock.rs`: `🌐` badge in the stream bar.
- `src/config.rs`: `[global_filter]` read and write.
- `src/i18n.rs`: new strings in all 16 languages; help dialog entry.
- README, CHANGELOG, tests.
