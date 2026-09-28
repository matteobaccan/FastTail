## 1. Engine decoupling (first, own PR)

- [ ] 1.1 `src/color.rs` with `Rgba`; `HighlightStyle`, `CyberTheme` palette functions and every engine-facing colour use it; `From<Rgba> for egui::Color32` under `gui`; `CyberTheme::apply(ctx)` moves to `ui`
- [ ] 1.2 Move `markdown_cache` from `TailEngine` to the GUI's per-tab state in `ui/dock.rs`
- [ ] 1.3 `external_tools::Shortcut` with a neutral `KeyName` and `Mods`; GUI mapping at the edge; `tool.N` ini round-trip test unchanged
- [ ] 1.4 `src/workspace.rs`: `open_target` (file, pattern, compressed, archive entry, archive needing a choice with its entry list, standard input), `restore(config)`, the go-to parser and the time-range text parser moved out of `ui/dock.rs` and `ui/time_range.rs`; `ui/app.rs` calls them; existing GUI tests stay green
- [ ] 1.5 Search wrap-around returns a flag instead of calling `audio` from the engine; the GUI plays the beep
- [ ] 1.6 `Cargo.toml`: features `gui` and `tui`, `default = ["gui", "tui"]`; optional GUI crates under `gui`; `ui` and `renderer` behind `#[cfg(feature = "gui")]`; `[[bin]]` entries with `required-features`
- [ ] 1.7 Unit tests: `Rgba` to `Color32` mapping, `KeyName` parsing, `open_target` outcomes for each kind, go-to and time-range parsers; `cargo check --no-default-features --features tui` and `cargo tree` without GUI crates

## 2. Terminal interface core (from the prototype)

- [ ] 2.1 Move the prototype (`app`, `view`, `keys`, `mouse`, `colors`, `clipboard`) into the library module `tui` with `tui::run(args, config)`; `src/bin/fasttail-tui.rs` as a thin `main`; drop the prototype-only `--bench`, `--capture`, `--stats` from the user help (keep them hidden for tests and measurements)
- [ ] 2.2 Open everything through `workspace::open_target`; workspace restore, `--fresh`, `--session`, command-line paths as in the GUI
- [ ] 2.3 Read-only configuration: theme, language, per-stream state, bookmarks and notes, highlight rules, quick labels, global filter, recent files and sessions, search history, `show_line_numbers`, `level_colors`, `sound_enabled`; one retry after 100 ms on a parse error; a test that `fasttail.ini` is byte-for-byte unchanged after a run
- [ ] 2.4 Keyboard cursor row, `Shift+↑` / `Shift+↓` selection, row actions on selection or cursor; cell-width horizontal scroll
- [ ] 2.5 Bookmarks (`b`, `Ctrl+F2`, `]` `[`, `F2` `Shift+F2`), note dialog (`m`), note in the status bar, `Ctrl+K` context view with banner, go-to dialog (`Ctrl+G`, `:`)
- [ ] 2.6 HEX view (`h`): width-driven bytes per row, visible rows only, byte hits painted, `n` / `N`, place kept both ways, offset go-to, follow
- [ ] 2.7 ANSI rendering by mode (`a`), raw mode with `^[`, no escape sequence written from log text; precedence search over rule over ANSI
- [ ] 2.8 Dialogs: shared text field and list widgets; archive entry picker, time range (text fields), open file, open session, save session as, quit confirmation; help lists every key
- [ ] 2.9 Global filter on / off (`f`), theme cycle (`T`), close stream (`w`)
- [ ] 2.10 Session save through `session`: no `[dock]`, overwrite confirmation, refuse the active `fasttail.ini`; the GUI opens such a file with its default layout (integration test)
- [ ] 2.11 Colour depth detection with the 256-colour cube; ASCII fallback; key-press filtering on Windows; terminal restore on exit and panic; mintty message and exit code 1
- [ ] 2.12 256 KiB buffered output; redraw only on change; poll 100 ms / 50 ms
- [ ] 2.13 `TestBackend` capture tests: each view (text, HEX at 80 and 240 columns, split, context banner), each dialog, ASCII mode, one language with CJK text; mouse hit tests; frame-time test on a generated file (p95 at most 5 ms at 200 x 60, ignored by default like the benchmarks)

## 3. Interface selection and launch

- [ ] 3.1 `interface=gui|tui` in `[general]` (`config.rs` load and save, default `gui`); `cli.rs`: `--tui`, `--gui` as override, both a usage error, terminal options accepted everywhere and ignored by the GUI
- [ ] 3.2 Windows: `fasttail.exe` hand-off with `CreateProcessW` + `CREATE_NEW_CONSOLE` and `--handoff`; missing or failing executable opens the GUI with the error dialog; refusal with standard input; `fasttail-tui` waits for a key on an error exit after a hand-off
- [ ] 3.3 Linux and macOS: terminal checks (stdout is a terminal, `TERM` not `dumb`, stdin or `/dev/tty` for keys), fallback to the GUI with the stderr line for `interface=tui`, exit 1 for `--tui`
- [ ] 3.4 `fasttail-tui`: `--gui` usage error naming the graphical executable, `--renderer` ignored, `--help` with the terminal options and environment variables
- [ ] 3.5 Settings: "Interface: Graphical / Terminal (experimental)" next to the renderer, next-start note, Windows warning when `fasttail-tui.exe` is missing
- [ ] 3.6 Tests: interface resolution order, argument forwarding (minus `--tui`, plus `--handoff`), the executable lookup path, the Unix terminal decision table

## 4. Release pipeline

- [ ] 4.1 `test` job: `cargo check --no-default-features --features tui --bin fasttail-tui` on Linux and Windows
- [ ] 4.2 Windows build: `cargo build --release --bins`; zip with `fasttail.exe` and `fasttail-tui.exe`; symbols zip with both PDBs
- [ ] 4.3 Every build job prints the byte sizes of its executables and archives; compare with the previous release against the budget in design.md

## 5. Texts and documentation

- [ ] 5.1 i18n keys for every terminal string and the Settings entry in all 16 languages; add them to the exhaustive i18n test
- [ ] 5.2 `docs/tui.md` (user guide built from `docs/tui-feasibility.md`: start, keys, mouse, Windows consoles, colours, limits); keep the feasibility measurements as an appendix
- [ ] 5.3 README: "Terminal interface (experimental)" section, why Windows has `fasttail-tui.exe` and why to run it directly from a terminal, the `interface` key, `--tui` / `--gui`, troubleshooting (mintty, legacy console, OSC 52)
- [ ] 5.4 `docs/ui-design.md`: the Interface setting; CHANGELOG `[Unreleased]`

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PRs with Linux and Windows CI green
- [ ] 6.2 Local preview of both Windows executables for the maintainer (double click with `interface=tui`, cmd, PowerShell, Windows Terminal, conhost) before the 0.20.0 release
- [ ] 6.3 After the release, archive the change so `terminal-interface` is created and `command-line`, `rendering-backend`, `release-pipeline` and `cyber-themes` gain their deltas
