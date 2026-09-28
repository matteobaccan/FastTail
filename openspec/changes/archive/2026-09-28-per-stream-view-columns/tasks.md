## 1. Engine and rendering

- [x] 1.1 `TailEngine::show_line_numbers`, `show_time_delta`, `view_columns_dirty` with `set_show_line_numbers` / `set_show_time_delta`; unit test that a switch belongs to its stream
- [x] 1.2 Stream bar `# 123` and `Δt` toggles call the engine setters; `render_rows` and `time_delta_column` read the stream's own switches, the gap threshold stays global
- [x] 1.3 `apply_view_defaults` at every stream creation (restored tabs, files, patterns, compressed files, archive entries, standard input)

## 2. Persistence

- [x] 2.1 `StreamEntry::line_numbers` / `time_delta` (`Option<bool>`), written as `line_numbers=` / `time_delta=` only when set, read back, missing keys as `None`; unit test
- [x] 2.2 `stream_entry_of` records only what differs from the defaults; `apply_stream_state` restores; the dirty flag is drained with the timeline and collapse flags
- [x] 2.3 A change of the defaults in Settings re-records every open stream's entry

## 3. Texts and documentation

- [x] 3.1 Settings labels "… in new streams" and a tooltip, new i18n keys in all 16 languages and in the exhaustive i18n test
- [x] 3.2 README, `docs/ui-design.md`, CHANGELOG `[Unreleased]` → Changed

## 4. Tests and wrap-up

- [x] 4.1 `per_stream_columns` integration tests: two streams render differently, a bar toggle changes only its stream, new streams take the defaults, workspace and session round trips, old files without the keys
- [x] 4.2 `cargo fmt`, `cargo clippy --all-targets`, `cargo test`
- [x] 4.3 Archive the change
