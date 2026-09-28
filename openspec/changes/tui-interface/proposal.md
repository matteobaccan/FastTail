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
showed what is missing for daily use (a keyboard cursor row, the configuration, ANSI
colours, an archive picker) and what in the library still ties every build to egui,
wgpu and winit.

This change turns the prototype into a supported second interface, labelled
experimental, sharing the engine, the configuration and the workspace with the GUI.

Target release: **0.20.0**.

## What Changes

- **Engine decoupling first.** A neutral RGBA colour type replaces `egui::Color32` in
  engine-facing types (`HighlightStyle`, the theme's level palette), the Markdown cache
  moves out of `TailEngine` into the GUI's per-tab state, `external_tools::Shortcut`
  uses a neutral key type, and the "open any file / pattern / archive / standard input"
  dispatch and the go-to and time-range text parsing move out of `ui` into a shared,
  UI-agnostic module. The GUI code goes behind a `gui` Cargo feature, the terminal code
  behind a `tui` feature, both on by default; `cargo build --no-default-features
  --features tui` compiles without eframe, egui, wgpu, winit, egui_dock, egui_commonmark
  and rfd.
- **Interface choice, separate from the renderer.** Settings gains "Interface:
  Graphical / Terminal (experimental)" next to the renderer choice, saved as
  `interface=gui|tui` in `[general]` of `fasttail.ini` (**new key**, default `gui`).
  The command line gains `--tui`; `--gui`, accepted and ignored since 0.7.1, now
  overrides `interface=tui` for one start.
- **Windows: two executables.** The release zip holds `fasttail.exe` (GUI subsystem)
  and `fasttail-tui.exe` (console subsystem). `fasttail.exe` asked for the terminal
  interface starts `fasttail-tui.exe` from its own directory in a new console window,
  passes the arguments along, and exits; if `fasttail-tui.exe` is missing it shows an
  error and opens the GUI. From a terminal the user runs `fasttail-tui.exe` directly
  (a GUI-subsystem process does not make cmd or PowerShell wait, so both would read the
  same console).
- **Linux and macOS: one binary.** `fasttail --tui` or `interface=tui` starts the
  terminal interface when standard output is a terminal and a terminal is available for
  input; otherwise `interface=tui` opens the GUI and `--tui` exits with an error.
- **The terminal interface (`terminal-interface`, new):** bordered stream windows with
  a focused border, dialogs, mouse (off with `--no-mouse`), `--ascii` borders, a
  two-way split, follow, search, include / exclude filters, minimum level, collapse,
  compressed files and archives, plus for 0.20.0:
  - a keyboard cursor row with bookmarks (toggle, next, previous), bookmark notes, show
    in context and go to line or time;
  - a **HEX view** per stream (`h`): offset, bytes and an ASCII column sized to the
    window width, search hits highlighted, served by the engine's HEX APIs;
  - ANSI colours in the log rendered, following the stream's ANSI mode;
  - the archive entry picker as a dialog;
  - a text time-range dialog (from / to fields, no calendar);
  - highlight rules, quick labels, the theme's level palette and the global filter from
    `fasttail.ini`;
  - every string through `i18n::t(lang, key)` in all 16 languages.
- **Shared configuration and workspace, read-only.** The terminal interface reads
  `fasttail.ini` (open files, per-stream state, bookmarks with notes, theme, language,
  highlight rules, global filter, sessions) and never writes it. It loads a session
  with `--session` or from a dialog, and saves the current state only by an explicit
  "Save session as" to a `*.fasttail-session.ini` file.
- **Release pipeline.** Every target builds the terminal interface; the Windows zip
  adds `fasttail-tui.exe` and the symbols zip `fasttail-tui.pdb`; the `test` job adds a
  GUI-free `cargo check`. Size budget: `fasttail-tui.exe` at most 6 MB, the Windows zip
  at most 3 MB larger than 0.19.x, the Linux / macOS `fasttail` executable at most 3 MB
  larger uncompressed.

## Capabilities

### New Capabilities

- `terminal-interface`: the terminal front end: start conditions, layout, keys and
  mouse, cursor row, bookmarks and context, HEX view, ANSI colours, dialogs, shared
  read-only configuration and workspace, session save, i18n, terminal compatibility,
  scope limits.

### Modified Capabilities

- `command-line`: `--tui`, `--gui` as an override, the `fasttail-tui` executable's
  options (`--ascii`, `--no-mouse`, `--theme`), the Windows hand-off and the
  terminal checks on Linux and macOS.
- `rendering-backend`: the Settings dialog gains the interface choice next to the
  renderer, and the renderer setting does not apply to the terminal interface.
- `release-pipeline`: packaging of `fasttail-tui.exe` and its PDB, the GUI-free check
  in the `test` job, the size budget.
- `cyber-themes`: the four themes as a neutral palette usable by the terminal
  interface.

## Impact

- `Cargo.toml`: features `gui` and `tui` (default `["gui", "tui"]`), optional
  `eframe`, `egui`, `egui_dock`, `egui_commonmark`, `rfd` under `gui`; `ratatui`,
  `crossterm`, `arboard` under `tui`; `[[bin]]` entries with `required-features`.
- `src/lib.rs`: `ui` and `renderer` behind `gui`; new `workspace` (open dispatch,
  restore) and `tui` modules; `src/bin/fasttail-tui/` from the prototype.
- `src/theme.rs`, `src/tail_engine.rs`, `src/external_tools.rs`, `src/config.rs`,
  `src/ui/app.rs`, `src/ui/dock.rs`, `src/ui/time_range.rs`: decoupling.
- `src/main.rs`, `src/cli.rs`: `--tui`, `--gui`, the Windows hand-off and the Unix
  terminal checks.
- `src/i18n.rs`: the terminal interface's strings and the Settings entry in all 16
  languages.
- `.github/workflows/build.yml`: GUI-free check, packaging.
- README, `docs/tui.md` (from `docs/tui-feasibility.md`), `docs/ui-design.md`,
  CHANGELOG, tests.

## Non-goals

- The Markdown view: rendered Markdown has no meaning in a terminal; `.md` files open
  as text.
- A timeline histogram with graphics or brushing; even a one-row text sparkline is
  deferred to a later release.
- External tools: the key type is decoupled now, launching them from the terminal is a
  later release.
- The Matrix screensaver and the window PIN lock: the terminal and the OS session own
  the screen.
- Renderer, GPU, zoom, font and window settings in the terminal interface.
- Writing `fasttail.ini` from the terminal interface (no settings dialog, no workspace
  save on exit).
- Find results across streams, filter presets, wrap and free resizable tiling in
  0.20.0 (see design.md for when).
- Dock layout parity: the terminal shows at most two windows at a time.
