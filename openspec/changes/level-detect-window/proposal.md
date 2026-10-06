## Why

The level of a line is looked for only in its first 96 bytes (`DETECT_WINDOW` in
`src/log_level.rs`). Some logs put a long prefix first (host, process, thread, class,
request id) and the level after it: on such a log every line is `unknown`, so the level
colours, the minimum-level filter, the counters and level navigation do nothing. The user
cannot change it. Reported by the maintainer on 2026-10-06.

## What Changes

- A **setting "Look for the level in the first N bytes"**, default 96 (today's behaviour),
  range 16 to 4,096, in Settings of the window and of the terminal: the default for every
  stream. New `fasttail.ini` key `level_detect_bytes`.
- A **per-stream override** in the stream's level menu (window) and level chip (terminal):
  "Level searched in the first N bytes", saved with the stream as `level_bytes` only when
  it differs from the global value. Sessions carry it too.
- Changing the value redetects the levels of the stream (cache, counters, filter, colours)
  as a background job with progress above 16 MB, as when a file is reopened.
- `fasttail --print` uses the same value (`--level-bytes N` overrides it on the command line).
- Older builds ignore both keys and keep 96.

Target release: a **nightly patch (0.20.x)**, requested by the maintainer on 2026-10-06.
Priority: **high**. Effort: **S**.

### Non-goals

- Telling where the level is by column, field or regex: `custom-log-formats` and the
  field parsers (`structured-field-terms`) cover that.
- New level words or a user list of level words.
- The timestamp scan window (64 bytes), which stays fixed.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `log-intelligence`: Log Level Detection (configurable scan window, global and per stream).

## Impact

- `src/log_level.rs`: `detect_level` takes the window (the constant becomes the default);
  callers in `tail_engine.rs`, `scan_job.rs`, `print_mode.rs`.
- `src/tail_engine.rs`: the stream's window, cache reset and recount on change.
- `src/config.rs`: `level_detect_bytes` and the stream key `level_bytes` (load, save,
  round-trip); `src/command_line.rs` (or equivalent): `--level-bytes`.
- `src/ui/` Settings and the level menu; `src/tui/settings.rs` and the level chip.
- i18n (`src/i18n.rs`, `src/i18n_tui.rs`) in every language, README, `docs/ui-design.md`,
  `docs/tui.md`, CHANGELOG, tests, a benchmark at 96 and 1,024 bytes.
