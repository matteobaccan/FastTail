## Why

The line-number column (`# 123`) and the time delta column (`Δt`) are toggled from each
stream bar, but the switch is one global value: turning `Δt` on to study a timed service
log also turns it on in the access log beside it, and hiding line numbers in one pane hides
them everywhere. The maintainer reported it: "the time delta and the line numbers are
applied globally, while they should be applied per window". Every other view switch of the
stream bar (wrap, timeline, ANSI mode, collapse) already belongs to its stream.

## What Changes

- Each stream has its own line-number and time delta switches. The `# 123` and `Δt`
  buttons of a stream bar change only that stream; row rendering in the normal and wrap
  layouts, the column widths and the collapse badge layout follow the stream's own
  switches.
- A newly opened stream (file, pattern, standard input, compressed file, archive entry)
  starts from the `[general]` values `show_line_numbers` and `show_time_delta`, which stay
  in `fasttail.ini` and become the **defaults for new streams**. Settings labels them that
  way. Changing a default does not change the streams already open.
- The switches are saved with the stream in the workspace and in session files as
  `line_numbers=true|false` and `time_delta=true|false` in the `[stream_N]` section,
  always written, so a saved stream never depends on the defaults. Old files without the
  keys take the defaults.
- The Δt gap threshold (`time_delta_gap_ms`) stays one global preference.
- No new key in `[general]`.

Target release: **0.12.0**.

### Non-goals

- A per-stream gap threshold.
- Changing what copy and export write (they never include line numbers).
- A "apply to all streams" command for the switches.
