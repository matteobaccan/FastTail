## Why

FastTail only runs as a desktop window. On an SSH session, in a container, on a server
without a display, or for a user who lives in a terminal, there is nothing to start:
the user falls back to `tail -f | grep` and loses the line index, the filters, the
search, collapse, bookmarks and compressed files. FastTail had an experimental terminal
front end until 0.7.1; it was removed because it duplicated engine logic and drifted.

The prototype on `feat/tui-prototype` (draft PR #131, `docs/tui-feasibility.md`) showed
that a terminal front end can now sit on the engine without touching it: bordered
windows, dialogs, mouse, split view, search, filters, collapse and compressed files in
about 2,800 lines, a full redraw in about 1 ms on a 10-million-line file, 0.26 % of one
core when idle, and a 3.5 MB Windows executable against 21.7 MB for the GUI. It also
showed what is missing for daily use and what in the library still ties every build to
egui, wgpu and winit.

This change turns the prototype into a full second interface. A user picks the
graphical or the terminal version and expects the same things from either: the same
workspace, the same settings, the same rules, presets and tools, the same PIN lock,
saved to the same `fasttail.ini`.

Target release: **0.20.0**, released only when the terminal interface reaches parity
with the GUI for everything in scope below.

## What Changes

- **Engine decoupling first.** A neutral RGBA colour type replaces `egui::Color32` in
  engine-facing types (`HighlightStyle`, the theme palette), the Markdown cache moves out
  of `TailEngine` into the GUI's per-tab state, `external_tools::Shortcut` uses a
  neutral key type, and the "open any file / pattern / archive / standard input"
  dispatch, the workspace save, the go-to and time-range parsers, the PIN attempt
  counter and the settings validation move out of `ui` into shared, UI-agnostic
  modules. The GUI code goes behind a `gui` Cargo feature, the terminal code behind a
  `tui` feature, both on by default; `cargo build --no-default-features --features tui`
  compiles without eframe, egui, wgpu, winit, egui_dock, egui_commonmark and rfd.
- **Interface choice, separate from the renderer.** Settings (in both interfaces) gains
  "Interface: Graphical / Terminal", saved as `interface=gui|tui` in `[general]` of
  `fasttail.ini` (**new key**, default `gui`). The command line gains `--tui`; `--gui`,
  accepted and ignored since 0.7.1, now selects the graphical interface.
- **Hand-off in both directions.** `fasttail --tui` or `interface=tui` starts the
  terminal interface; `fasttail-tui --gui`, or "Switch now" after changing the interface
  in Settings, starts the graphical one. On Windows each executable starts the other
  from its own directory (`fasttail.exe` is GUI subsystem, `fasttail-tui.exe` console
  subsystem, both in the release zip); if the other one is missing the user is warned
  and the current interface keeps running, or exits with a clear message when it was
  started only to hand off. On Linux and macOS both interfaces are in the one `fasttail`
  binary; the terminal-only archive answers `--gui` with a message that the GUI is not
  in that build.
- **The terminal interface (`terminal-interface`, new)**, for 0.20.0:
  - bordered stream windows with a focused border, dialogs, mouse (off with
    `--no-mouse`), `--ascii`, a two-way split, follow, search, include / exclude
    filters, minimum level, collapse, compressed files and archives;
  - a keyboard cursor row with bookmarks (toggle, next, previous), notes, show in
    context and go to line or time;
  - a **HEX view** per stream (`h`): offset, bytes and an ASCII column sized to the
    window width, search hits highlighted, served by the engine's HEX APIs;
  - ANSI colours rendered; the archive entry picker and a time-range dialog;
  - a **Settings dialog** for everything that applies to a terminal (theme, language,
    refresh and performance values, defaults for new streams, `auto_bookmark_max`,
    sound, PIN lock and idle timeout, the interface choice);
  - the **highlight-rule editor** (fg / bg / bold / italic, auto-bookmark, reorder),
    **filter presets**, **global filter editing**, and **external tools** (editor with
    placeholders and shortcuts, runs on the cursor row, rule-bound runs);
  - the **PIN lock**: `Ctrl+L`, idle lock after the same timeout as the GUI, a
    full-screen PIN dialog hiding every log line, the same attempts and cooldown;
  - every string through `i18n::t(lang, key)` in all 16 languages.
- **Shared configuration and workspace, read and written.** The terminal interface
  loads and saves `fasttail.ini` like the GUI: open files, per-stream state, bookmarks
  and notes, recent files and sessions, search history, theme, language and every
  setting it edits, when the GUI would save them (every 2 s when changed, on explicit
  actions, on exit) with the same write-if-changed rule. With several instances on one
  file the last writer wins, and every instance, GUI included, writes only when its own
  state changed.
- **Release pipeline.** Every target builds the terminal interface. The Windows zip
  adds `fasttail-tui.exe` (symbols zip: `fasttail-tui.pdb`); Linux x86_64 and ARM64
  add a terminal-only archive `fasttail-tui-linux-<arch>-<version>.tar.gz` built
  without the `gui` feature; the `test` job adds a GUI-free `cargo check`. Size budget
  in the release-pipeline delta.

## Capabilities

### New Capabilities

- `terminal-interface`: the terminal front end: launch and hand-off, layout, keys and
  mouse, cursor row, bookmarks and context, HEX view, ANSI colours, dialogs, Settings,
  rules, presets, global filter, external tools, the lock screen, shared configuration
  and workspace, sessions, i18n, terminal compatibility, performance, scope limits.

### Modified Capabilities

- `command-line`: `--tui`, `--gui`, the terminal options, the `fasttail-tui`
  executable and its `--gui` hand-off.
- `rendering-backend`: the Settings dialog gains the interface choice next to the
  renderer; the renderer setting does not apply to the terminal interface.
- `release-pipeline`: `fasttail-tui.exe` and its PDB, the Linux terminal-only archives,
  the GUI-free check, the size budget.
- `cyber-themes`: the four themes as a neutral palette used by both interfaces, theme
  changes saved from either.
- `window-lock`: the lock armed and shown in the terminal interface too.

## Impact

- `Cargo.toml`: features `gui` and `tui` (default `["gui", "tui"]`), optional GUI
  crates under `gui`; `ratatui`, `crossterm`, `arboard` under `tui`; `[[bin]]` entries
  with `required-features`.
- `src/lib.rs`: `ui` and `renderer` behind `gui`; new `color`, `workspace`, `lock`,
  `settings_model` and `tui` modules; `src/bin/fasttail-tui.rs`.
- `src/theme.rs`, `src/tail_engine.rs`, `src/external_tools.rs`, `src/config.rs`,
  `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/time_range.rs`: decoupling; the "write only on own
  change" fix in `config.rs` / `ui/app.rs` ships earlier as a separate 0.13.0 bugfix.
- `src/main.rs`, `src/cli.rs`: `--tui`, `--gui`, hand-offs, Unix terminal checks.
- `src/i18n.rs`: terminal strings and the Settings entry in all 16 languages.
- `.github/workflows/build.yml`: GUI-free check, packaging, Linux terminal-only build.
- README, `docs/tui.md`, `docs/ui-design.md`, CHANGELOG, tests.

## Non-goals

- The Markdown view: rendered Markdown has no meaning in a terminal; `.md` files open
  as text.
- The Matrix screensaver: the terminal interface goes straight to the lock screen.
- Renderer, GPU, frame-rate, zoom, font, always-on-top, borderless and window settings
  in the terminal interface: they concern the GUI only and are kept as they are in
  `fasttail.ini`.
- A timeline histogram with graphics or brushing, the overview strip, line wrap and
  free resizable tiling in 0.20.0 (later releases).
- Find results across streams in 0.20.0 (0.21.0).
- Dock layout parity: the terminal shows at most two windows at a time.
- Merging concurrent edits of `fasttail.ini` from several instances.
