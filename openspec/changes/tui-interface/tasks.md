## 0. Release gate

- [ ] 0.1 0.20.0 is tagged only when every task of groups 1 to 7 is done and the parity checklist (7.1) passes: every item marked 0.20.0 in the design's scope table works in the terminal interface as in the GUI. No partial terminal interface ships, and no "experimental" label is used anywhere (Settings, help, README, CHANGELOG)

## 1. Engine decoupling (first, own PR)

- [x] 1.1 `src/color.rs` with `Rgba`; `HighlightStyle`, rule colours, `CyberTheme` palette functions and every engine-facing colour use it; `From<Rgba> for egui::Color32` and `CyberTheme::apply(ctx)` under `gui`
- [x] 1.2 Move `markdown_cache` from `TailEngine` to the GUI's per-tab state in `ui/dock.rs`
- [x] 1.3 `external_tools::Shortcut` with a neutral `KeyName` and `Mods`; GUI mapping at the edge; `tool.N` ini round-trip test unchanged
- [x] 1.4 `src/workspace.rs`: `open_target`, `restore(config)`, `snapshot(engines)` (open files, `stream_N` state, bookmarks and notes, taken out of `save_dock_layout`), the go-to parser and the time-range text parser; the GUI calls them
- [x] 1.5 `src/lock.rs`: `LockAttempts`, `LOCK_MAX_FAILURES`, `LOCK_COOLDOWN`, `pin_matches` and an idle clock with the arming rule, out of `ui/app.rs` and `config.rs`; the GUI lock uses it
- [x] 1.6 `src/settings_model.rs`: ranges, defaults and validation of every setting both interfaces edit, highlight-rule, preset, global-filter and tool validation; the GUI Settings and `FastTailConfig::load` use it
- [x] 1.7 Search wrap-around and rule sound alerts returned as engine events instead of calling `audio` from the engine; the GUI plays them
- [x] 1.8 `Cargo.toml`: features `gui` and `tui`, `default = ["gui", "tui"]`; optional GUI crates under `gui`; `ui` and `renderer` behind `#[cfg(feature = "gui")]`; `[[bin]]` entries with `required-features`
- [x] 1.9 Unit tests for each moved piece; `cargo check --no-default-features --features tui`; `cargo tree` without GUI crates

## 2. Configuration shared and written by both interfaces

- [ ] 2.1 Own-change saves: the GUI part shipped in 0.13.0 (#144); here the terminal applies the same rule (keeps the bytes it last loaded or wrote, writes only when its serialised state differs); test that a GUI and a terminal with different state do not rewrite the file while idle
- [x] 2.2 GUI restore reconciles a dock layout whose tabs differ from `open_files` (drop closed files, add new ones to the main area); test with a layout written before the terminal changed the open files
- [ ] 2.3 Terminal load and save through `FastTailConfig` with the user-directory fallback; 2 s save when changed, on dialog confirmation, on session operations, on exit; standard-input streams not saved; GUI-only keys carried through unchanged (test: dock layout, `font_size`, `max_fps`, `renderer`, `zoom_factor` byte-identical after a terminal save)
- [x] 2.4 `interface=gui|tui` in `[general]` (load, save, default `gui`)

## 3. Terminal interface: viewing

- [ ] 3.1 Move the prototype into the library module `tui` with `tui::run(args, config)`; `src/bin/fasttail-tui.rs` as a thin `main`; keep `--bench`, `--capture`, `--stats` hidden for tests and measurements
- [ ] 3.2 Open everything through `workspace::open_target`; workspace restore, `--fresh`, `--session`, command-line paths as in the GUI
- [ ] 3.3 Keyboard cursor row, `Shift+↑` / `Shift+↓` selection, row actions on selection or cursor; cell-width horizontal scroll
- [ ] 3.4 Bookmarks (`b`, `Ctrl+F2`, `]` `[`, `F2` `Shift+F2`), note dialog (`m`), note in the status bar, `Ctrl+K` context view with banner, go-to dialog (`Ctrl+G`, `:`)
- [ ] 3.5 HEX view (`h`): width-driven bytes per row, visible rows only, byte hits painted, `n` / `N`, place kept both ways, offset go-to, follow
- [ ] 3.6 ANSI rendering by mode (`a`), raw mode with `^[`, no escape sequence written from log text; precedence search over rule over ANSI
- [ ] 3.7 Viewing dialogs on a shared form toolkit (text field, number field, check box, radio list, reorderable list, colour field): archive entry picker, time range (text fields), open file, open and save session (recent sessions updated, `fasttail.ini` refused), help listing every key
- [ ] 3.8 Colour depth detection with the 256-colour cube; ASCII fallback; alternate screen; key-press filtering on Windows; terminal restore on exit and panic; mintty message and exit code 1
- [ ] 3.9 256 KiB buffered output; redraw only on change; poll 100 ms / 50 ms

## 4. Terminal interface: settings, editors, tools, lock

- [ ] 4.1 Settings dialog (`,`): interface, appearance, performance and refresh, new-stream defaults, `auto_bookmark_max`, `size_unit`, sound, PIN lock (set / change / remove, `lock_enabled` (arms the idle lock), idle minutes, deterrent statement), validation from `settings_model`, `[ OK ]` applies and saves
- [ ] 4.2 Highlight-rule editor (`r`): all rule fields, add / edit / delete / reorder, colour swatches and `#RRGGBB` with depth preview, quick labels listed and removable
- [ ] 4.3 Filter presets (`p`): apply, save current as, rename, delete
- [ ] 4.4 Global filter editor (`F`) and on / off (`f`), 300 ms debounce
- [ ] 4.5 External tools: editor page in Settings, `!` menu on the cursor row or selection, shortcuts, rule-bound runs with the 1 per second and 10 children limits, null standard handles for every child
- [ ] 4.5b Key conventions: `e`/`E`, `w`/`W` level jumps, counts, `?` with no file, `:` palette over `src/actions.rs` (line jump), bindings from the registry, Kitty keyboard protocol on / off
- [ ] 4.6 Lock screen: `Ctrl+L`, idle lock from the shared idle clock, full-screen bordered PIN dialog with nothing else drawn, key and mouse filtering (`q`, `Esc`, `Ctrl+C` dropped), shared attempts and cooldown, maintenance phrase, tailing continues, exact restore
- [ ] 4.7 Theme cycle (`T`) and close stream (`w`)

## 5. Interface selection and hand-offs

- [ ] 5.1 `cli.rs`: `--tui`, `--gui`, both a usage error, terminal options accepted everywhere and ignored by the GUI
- [ ] 5.2 Windows `fasttail.exe` to terminal: `CreateProcessW` + `CREATE_NEW_CONSOLE` + `--handoff`; missing or failing executable opens the GUI with the error dialog; refusal with standard input; error-exit key wait after a hand-off
- [ ] 5.3 Windows `fasttail-tui.exe --gui`: detached start of `fasttail.exe`, exit 0; missing: stderr message and exit 1
- [ ] 5.4 Linux and macOS: terminal checks (stdout, `TERM`, stdin or `/dev/tty`), GUI fallback with the stderr line for `interface=tui`, exit 1 for `--tui`; terminal-only build answers `--gui` with exit 2 and the archive name
- [ ] 5.5 Settings interface switch in both interfaces: "Switch now" / "At next start", workspace saved before switching; missing executable, no display, or no terminal reported while the current interface keeps running; Graphical disabled in the terminal-only build; Windows warning when the other executable is missing
- [ ] 5.6 Tests: resolution order, argument forwarding, executable lookup paths, the Unix terminal decision table, the display check

## 6. Release pipeline

- [ ] 6.1 `test` job: `cargo check --no-default-features --features tui --bin fasttail-tui` on Linux and Windows
- [ ] 6.2 Windows build: `cargo build --release --bins`; zip with `fasttail.exe` and `fasttail-tui.exe`; symbols zip with both PDBs
- [ ] 6.3 Linux x86_64 and ARM64: second build `--no-default-features --features tui --bin fasttail-tui`, `cargo tree` check for GUI crates, `fasttail-tui-linux-<arch>-<version>.tar.gz` with `LICENSE` and `README.md`; the release job uploads both
- [ ] 6.4 Every build job prints the byte sizes of its executables and archives; compare with the previous release against the budget table in design.md

## 7. Parity, texts, documentation

- [ ] 7.1 Parity checklist, run by hand on Windows (Windows Terminal, conhost, double click with `interface=tui`) and Linux (terminal and SSH): every 0.20.0 item of the scope table; a setting, a rule, a preset, a tool, a global filter term, a bookmark with a note and a session changed in one interface are found unchanged in the other after a restart; both hand-offs with and without the other executable
- [ ] 7.2 `TestBackend` capture tests: each view (text, HEX at 80 and 240 columns, split, context banner), each dialog and editor, the lock screen (no line of the open files in the frame), ASCII mode, a CJK language; mouse hit tests; frame-time test on a generated file (p95 at most 5 ms at 200 x 60, ignored by default like the benchmarks)
- [ ] 7.3 i18n keys for every terminal string, the Settings entry and the hand-off messages in all 16 languages; add them to the exhaustive i18n test
- [ ] 7.4 `docs/tui.md` (user guide from `docs/tui-feasibility.md`: start, keys, mouse, Settings and editors, lock, Windows consoles, colours, limits, the feasibility measurements as an appendix)
- [ ] 7.5 README: "Terminal interface" section, the two Windows executables and why to run `fasttail-tui.exe` directly from a terminal, the Linux terminal-only archive, the `interface` key, `--tui` / `--gui`, several instances on one `fasttail.ini` (last writer wins), troubleshooting (mintty, legacy console, OSC 52)
- [ ] 7.6 `docs/ui-design.md`: the Interface setting; CHANGELOG `[Unreleased]`

## 8. Wrap-up

- [ ] 8.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PRs with Linux and Windows CI green
- [ ] 8.2 Local preview of both Windows executables for the maintainer before the 0.20.0 release
- [ ] 8.3 After the release, archive the change so `terminal-interface` is created and `command-line`, `rendering-backend`, `release-pipeline`, `cyber-themes` and `window-lock` gain their deltas
