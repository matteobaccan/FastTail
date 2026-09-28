## TUI prototype: `fasttail-tui` (feasibility spike)

A second binary, `fasttail-tui`, runs the unchanged FastTail engine in a terminal, drawn with ratatui and crossterm. It sits behind a new `tui` Cargo feature that is off by default. The GUI, `cargo build` / `cargo test` and the release workflow do not change. The full report, with measurements, Windows console notes, what would need decoupling and an effort estimate, is in `docs/tui-feasibility.md`.

```sh
cargo build --release --features tui --bin fasttail-tui
fasttail-tui                                    # opens the GUI's workspace from fasttail.ini
fasttail-tui app.log other.log.gz               # just these files, with their saved state
fasttail-tui --session incident.fasttail-session.ini
fasttail-tui --config D:\other\fasttail.ini     # or FASTTAIL_CONFIG
```

### New in this update: it starts with the GUI's settings

- **Config lookup.** `fasttail.ini` is found exactly as the GUI finds it (`FastTailConfig::config_path()`). `--config FILE` and `FASTTAIL_CONFIG` work as in the GUI's command line.
- **Workspace restore.** With no FILE arguments, the TUI opens the GUI's `open_files` in tab order. Each stream gets its saved state, applied in the GUI's order: include and exclude terms, search query, encoding, ANSI mode, collapse mode, and bookmarks with notes. Pattern entries (`*.log`) open on their newest match. Missing files are skipped and named in the status bar.
- **FILE arguments.** Only those files open, but they still get the state the ini keeps for their paths.
- **`--session FILE`** opens a named `*.fasttail-session.ini`, as the GUI does.
- **Global settings.** From the ini the TUI takes:
  - the theme (`--theme` overrides it) and `level_colors`;
  - the highlight rules, through the engine's `set_highlight_rules`. The first enabled rule that matches colours the row with its fg/bg, and `bookmark=` rules create automatic bookmarks;
  - the global filter, when it is enabled;
  - `poll_interval_ms`, which sets how long the event loop waits when idle;
  - the size-check interval, the automatic-bookmark cap and the spool settings.

  The language is not used, because the TUI's few texts are not in i18n yet. Minimum level and time range are not persisted per stream by the GUI, so there is nothing to restore.
- **Read-only.** The TUI never writes `fasttail.ini`, so it cannot conflict with a running GUI. It deliberately bypasses `FastTailConfig::load()`, which can save the ini while migrating an old `fasttail.toml`. A test checks that the file is byte-for-byte unchanged. `docs/tui-feasibility.md` discusses saving TUI state later.

### What the prototype does

- **Layout.** Every stream is a bordered window. The focused one has a double border in the theme accent colour. The top border shows the name and follow state. The bottom border shows line counts, job progress, filters and the search position. The status bar is bordered too. Prompts and help open as centred dialogs with `[ OK ]` / `[ Cancel ]` buttons. `--ascii` (automatic on a legacy console) draws `+-|` borders instead.
- **Streams and navigation.**
  - Several files plus stdin; `Tab` / `Alt+1..9` switch between them. `s` gives a two-window split.
  - Follow with `Space` and `End`/`G`; scroll with arrows, PgUp/PgDn and Home/End, and sideways with Left/Right.
- **Viewing.**
  - Level colours come from the theme, mapped to RGB with a fallback to 16 colours.
  - `/` searches, `n` / `N` step through hits, and hits are highlighted.
  - `i` / `x` edit the include / exclude filter, `l` cycles the minimum level, `c` cycles the collapse mode.
- **Mouse.** The wheel scrolls the window under the pointer. A click focuses a window and selects a row. Shift+click or dragging selects a range, and a double click adds a bookmark. File titles and dialog buttons can be clicked. `y` / `Ctrl+C` copy the selection through `arboard` or, as a fallback, OSC 52. `--no-mouse` turns mouse capture off.
- **Files.** gzip, bzip2, xz and zstd files and archive entries open through the engine's own decompression path.
- **Terminal handling.** A panic hook restores the terminal. The screen is redrawn only when something changes, and only the visible rows are read.

### Measurements (1.04 GB, 10M lines, Windows 11)

- A frame renders in about 1 ms; in a real console, full-screen paging takes 0.6–0.8 ms at the median.
- An idle process uses 0.26 % of one core. It was measured with a 100 ms poll; the idle wait is now `poll_interval_ms`, 250 ms by default.
- The binary is 3.5 MB, against 21.7 MB for the GUI.

### Tests

- 28 unit tests, run with `cargo test --features tui --bin fasttail-tui`. They cover:
  - colour mapping, key handling, the layout of the visible rows and mouse hit testing;
  - `TestBackend` renders of borders, the split view, dialogs and clicks;
  - a workspace built from a temporary ini: two streams, a saved filter and bookmark, a row coloured by a highlight rule, a missing file named in the status bar, and the ini left unchanged;
  - a named session.
- The default `cargo test` is unaffected. `cargo clippy --all-targets --features tui` reports no warnings in the new code.

This is a draft, to judge feasibility, not to merge as is.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
