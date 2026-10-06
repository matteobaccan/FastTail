## 1. Engine

- [ ] 1.1 `src/log_level.rs`: `detect_level(line, window)`, `DEFAULT_WINDOW = 96`, range constants; unit tests at the window edge (token inside, token cut, token past it)
- [ ] 1.2 `tail_engine.rs`, `scan_job.rs`, `print_mode.rs` pass the stream's window; changing it clears the level cache and counters and refills them (UI thread under 16 MB, `Levels` job above)
- [ ] 1.3 Benchmark: 96 vs 1,024 bytes on lines without a level token

## 2. Settings and state

- [ ] 2.1 `src/config.rs`: `level_detect_bytes` (default 96, 16..=4096) and the stream key `level_bytes` (written only when it differs); load, save, round-trip, older-build test
- [ ] 2.2 Sessions carry `level_bytes`; `--print --level-bytes N`

## 3. Interfaces

- [ ] 3.1 Window: Settings field and the per-stream override in the level menu
- [ ] 3.2 Terminal: Settings field (`src/tui/settings.rs`) and the override from the level chip

## 4. Texts, docs, wrap-up

- [ ] 4.1 New texts in every language (`src/i18n.rs`, `src/i18n_tui.rs`)
- [ ] 4.2 Integration test: a log with the level at byte 150 is `unknown` at 96 and detected at 256; filter and counters follow the change
- [ ] 4.3 README, `docs/ui-design.md`, `docs/tui.md`, CHANGELOG `[Unreleased]`
- [ ] 4.4 `cargo fmt`, clippy (GUI and TUI builds), focused tests; PR with Linux and Windows CI green
- [ ] 4.5 Archive the change after the release that ships it
