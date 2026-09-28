## Context

`FastTailConfig::show_line_numbers` and `show_time_delta` (`[general]`) reach every tab
through `DockContext` (`show_line_numbers: &mut bool`, `time_delta: &mut TimeDeltaPrefs`),
and the stream bar toggles flip that shared value. The per-stream view flags that already
exist (`wrap_lines`, `timeline_open`, `ansi_mode`, collapse mode) live on `TailEngine`,
carry a dirty flag that the app drains after the dock is drawn, and are saved in
`StreamEntry` (`src/session.rs`), which is both the workspace's `[stream_N]` section of
`fasttail.ini` and the stream section of a session file.

## Goals / Non-Goals

**Goals:**
- The two switches belong to the stream, exactly like `timeline`.
- The `[general]` values keep working as the defaults of new streams, so a user who never
  touches the stream bar sees no change.
- Old workspaces and session files open as before.

**Non-Goals:**
- A per-stream gap threshold, a bulk "apply to all" command.

## Decisions

- **Engine fields.** `TailEngine` gets `show_line_numbers`, `show_time_delta` and
  `view_columns_dirty`, with `set_show_line_numbers` / `set_show_time_delta` that mark the
  stream dirty only on a real change. The engine constructor uses the shipped defaults
  (numbers on, Δt off); the app applies the `[general]` values at every stream creation
  (`apply_view_defaults`: restored tabs, `open_log_file` for files, patterns, compressed
  files and archive entries, standard input), then `apply_stream_state` applies what the
  stream saved.
- **Rendering.** `render_log_stream` no longer receives the global switches: the toggles
  call the engine setters, `render_rows` gets `engine.show_line_numbers`, and
  `time_delta_column` reads `engine.show_time_delta` with the global `gap_ms`. The HEX view
  never showed either column and is unchanged; copy and export never include line numbers.
- **Persistence.** `StreamEntry` gets `line_numbers: Option<bool>` and
  `time_delta: Option<bool>`. `stream_entry_of` always records both values, and the writer
  emits `line_numbers=` / `time_delta=` for every stream, so a saved stream never depends
  on the defaults: a session saved with a column off still opens with it off after the
  default is turned on. Missing or unreadable keys, found only in files from older
  versions, read as `None` and follow the defaults.
- **Changing a default.** Open streams keep their switches and nothing in the saved
  entries depends on the defaults, so a change in Settings neither rewrites the entries
  nor marks an open named session as modified. `fasttail.ini` is saved at once.
- **`TimeDeltaPrefs`.** Kept: `show` is now the default for new streams (Settings), `gap_ms`
  the shared threshold. `DockContext` keeps both fields for the Settings tab.

## Risks / Trade-offs

- Every stream section gains two keys. An older build ignores them.
- A session file from an older version opened for the first time shows `*` once the app
  compares it with the live workspace, since the live entries now carry the keys.

## Threads, memory, large files

Nothing runs on a worker that did not before: turning `Δt` on for one stream asks that
stream alone to be timed (in the background above 16 MB, as today). Memory: two booleans
per stream. Growing, rotated and truncated files are unaffected.
