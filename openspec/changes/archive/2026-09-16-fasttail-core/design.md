## Context

FastTail is a next-generation log tailing tool implemented in Rust. It aims to replace BareTail / BareTailPro by offering instant out-of-core performance, combined with an eDEX-UI inspired sci-fi HUD aesthetic, full docking customizability, JSON log formatting, and frictionless migration from existing BareTail Windows Registry settings.

## Goals / Non-Goals

**Goals:**
- **Zero External Dependencies**: Single portable executable for Windows, Linux, and macOS without requiring webview, .NET, or Electron runtimes.
- **Extreme Performance**: Low memory footprint (<50MB) and instant startup (<100ms), capable of tailing files >50GB via memory-mapped I/O.
- **Cyberpunk HUD Usability**: High-contrast dark neon aesthetic with modular docking (`egui_dock`) where all panels can be resized, minimized, or closed.
- **Sci-Fi Theme Suites**: Curated visual themes (**Tron**, **Matrix**, **Blade**) switchable at runtime.
- **Matrix Digital Rain Screensaver**: Idle animation activated by default at 10 minutes, configurable and instantly dismissible.
- **Internationalization (i18n)**: Native UI support for Italian, English, French, Spanish, and Chinese, with automatic fallback to English.
- **BareTail Muscle Memory**: Familiar shortcuts (`Space` for follow tail, `Ctrl+F` for live search, highlight prioritization) and automatic Windows Registry import on first boot.
- **Log Intelligence**: Inline tree expansion and pretty-printing of single-line JSON logs, plus multiline stack trace grouping.
- **Customizable FX**: Optional telemetry HUD (CPU/RAM/throughput) and optional cyber-terminal audio chirps, both disabled by default.

**Non-Goals:**
- Centralized log aggregation server (this is a client-side local/remote tail utility, not a replacement for Elastic/Loki/Datadog).
- Heavy 3D WebGL animations that waste CPU or battery (unlike the original Electron eDEX-UI).

## Decisions

### Decision 1: GUI Framework — `egui` + `eframe` + `egui_dock`
- **Choice**: `egui` with `eframe` rendering backend (`glow` or `wgpu`) and `egui_dock` for the windowing system.
- **Rationale**: Immediate mode UI in pure Rust compiles statically into a single small binary (~15 MB). It allows pixel-perfect custom cyber HUD styling (neon borders, scanlines, monospace rendering) and native docking with zero external C++ dependencies.
- **Alternatives considered**:
  - *Slint*: Rejected due to dual-licensing complexity and rigid window docking.
  - *Tauri*: Rejected because it requires WebView2 on Windows and WebKitGTK on Linux, violating the single standalone binary requirement.
  - *Iced*: Solid architecture, but dynamic docking and custom sci-fi HUD widget customization are less mature.

### Decision 2: File I/O — Memory Mapped Streaming (`memmap2`)
- **Choice**: Use memory-mapped files via `memmap2` paired with `notify` for filesystem change events and adaptive polling for network/UNC drives.
- **Rationale**: Memory mapping allows the OS page cache to manage memory paging. Files larger than 50 GB can be opened instantly with 64-bit offsets, indexing only line break byte offsets (`\n`) for the visible viewport window.
- **Alternatives considered**:
  - *Standard File::read to memory*: Would exhaust RAM on multi-gigabyte logs.
  - *Chunked seek/read on every frame*: Slower than OS page cache-backed `mmap`.

### Decision 3: BareTail Migration via Windows Registry (`winreg`)
- **Choice**: On Windows builds (`#[cfg(target_os = "windows")]`), inspect `HKCU\Software\Bare Metal Software\BareTail` and `BareTailPro` using the `winreg` crate.
- **Rationale**: Detects existing open files, recent files, and highlight color rules (RGB pairs) on first run. An interactive cyber HUD prompt lets the operator import everything in one click, persisting to a portable `fasttail.toml` configuration file.

### Decision 4: Modular Docking & Minimal Zen Mode
- **Choice**: Every UI element is an `egui_dock` tab or dockable container.
- **Rationale**: Users who want the full sci-fi eDEX-UI experience can keep telemetry, file trees, and highlight panels visible. Users who want pure BareTail simplicity can close or collapse telemetry and sidebar panels into a clean, distraction-free log viewer.

### Decision 5: Inline JSON and Multiline Parsing
- **Choice**: Lightweight heuristic scanning during line tokenization.
- **Rationale**: If a line contains `{` and `}`, a lazy JSON validator checks validity without blocking the main stream. An inline `[▼ JSON]` toggle is displayed, and full AST formatting occurs only on demand when clicked.

### Decision 6: Sci-Fi Visual Themes (Tron, Matrix, Blade)
- **Choice**: Define a `CyberTheme` token system supplying background, border, surface, text, and accent colors.
  - **Tron**: Neon cyan (`#00E5FF`), electric blue (`#0066FF`), dark obsidian (`#0A0E17`).
  - **Matrix**: Phosphor green (`#00FF41`), dark olive (`#003B00`), pure black (`#050505`).
  - **Blade**: Blade Runner amber (`#FF8C00`), neon magenta (`#FF0055`), dark charcoal (`#121014`).
- **Rationale**: High contrast for readability under any lighting, switchable at runtime without UI rebuild.

### Decision 7: Matrix Digital Rain Screensaver & Idle Timer
- **Choice**: Implement an idle detection tracker monitoring `egui::InputState` (last mouse pos, click time, keypress time). When `now - last_input_time > timeout` (default: 600s), an `egui::Painter` layer paints a 2D matrix rain simulation consisting of column buffers with falling unicode/katakana glyph heads and fading tails. Any mouse move, scroll, or keypress immediately dismisses the screensaver.
- **Rationale**: Extremely low CPU overhead in pure 2D immediate painting, while delivering the authentic sci-fi terminal vibe requested.

### Decision 8: Localization (i18n) Engine with English Fallback
- **Choice**: Compile-time embedded translation catalogs (`rosetta` / `rust-i18n` or static string maps) for `en`, `it`, `fr`, `es`, `zh`. Provide an `i18n::t!(key, lang)` macro that resolves to the selected language, automatically falling back to `en` if a key is missing.
- **Font Support**: Bundle a CJK-compatible font (such as Noto Sans SC / Source Han Code or WenQuanYi micro) as a secondary font fallback in `egui::FontDefinitions` to render Chinese characters properly.

## Risks / Trade-offs

- **[Risk] High-frequency log floods freezing the UI** → **Mitigation**: Decouple file reading onto a dedicated background worker thread communicating with the UI via crossbeam or bounded ring channels, batching UI redraw updates.
- **[Risk] Huge files with millions of line offsets consuming RAM** → **Mitigation**: Sparse line indexing (index every Nth line or dynamically map index blocks as the user scrolls).
- **[Risk] CJK font increasing binary size** → **Mitigation**: Use subsetted or compressed font binary data (e.g. WOFF2 decompressed in-memory) to keep the executable footprint under 15-20 MB.
- **[Risk] Network shares (SMB/CIFS) not emitting OS notify events** → **Mitigation**: Combine OS filesystem notifications with a 500ms heartbeat polling fallback for files hosted on network shares.
- **[Risk] Audio latency or crash on machines without audio devices** → **Mitigation**: Audio engine is completely optional, disabled by default, and fails silently without interrupting log streaming.
