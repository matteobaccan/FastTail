## Why

BareTail has been the gold standard for real-time log file monitoring on Windows for over two decades thanks to its instant startup, tiny footprint, and reliable out-of-core tailing. However, modern developers and sysadmins face logs that are massive, distributed, and structured (JSON, stack traces), running across Windows, Linux, and macOS. Meanwhile, sci-fi HUD interfaces like eDEX-UI proved that terminal tools can be visually stunning, but eDEX-UI was crippled by Electron bloat.

FastTail bridges these worlds: a pure Rust, zero-dependency, single-binary log tailer that combines the speed and muscle memory of BareTail with a modern, high-performance eDEX-UI sci-fi aesthetic, native BareTail settings migration, and intelligent structured log handling.

## What Changes

- Introduce FastTail as a standalone, zero-external-dependency Rust desktop application.
- Implement an out-of-core, memory-mapped streaming tail engine capable of monitoring arbitrarily large log files (>50GB) and detecting log rotation.
- Create an eDEX-UI inspired cyberpunk HUD interface using `egui` and `egui_dock`, where every panel can be resized, docked, minimized, or closed.
- Provide a selectable suite of iconic sci-fi themes: **Tron** (cyan/electric blue), **Matrix** (phosphor green/black), and **Blade** (amber noir/magenta).
- Implement an idle Matrix digital rain screensaver, enabled by default at 10 minutes with customizable timeout and toggle options.
- Add full internationalization (i18n) supporting Italian, English, French, Spanish, and Chinese, with automatic fallback to English.
- Provide a Windows Registry migration wizard that detects existing BareTail / BareTailPro configurations on first boot and prompts to import open files and highlight rules.
- Add real-time log intelligence: inline pretty-printing and tree view for JSON logs, and multiline stack trace grouping.
- Implement BareTailPro-grade live include/exclude filtering and multi-pattern color highlighting.
- Add an optional, minimizable telemetry HUD (throughput, RAM, CPU, lines) and optional cyber-terminal audio feedback (disabled by default).

## Capabilities

### New Capabilities
- `stream-engine`: Out-of-core file tailing engine with memory-mapped I/O (`memmap2`), 64-bit offsets, ring buffering, and log rotation/truncate detection.
- `cyber-ui-docking`: Sci-fi HUD aesthetic built with `egui` and `egui_dock`, featuring high-contrast neon styling, embedded monospace fonts, and a fully modular workspace where all panels can be resized, collapsed, or closed.
- `cyber-themes`: Built-in sci-fi color themes including **Tron** (cyan/electric blue), **Matrix** (phosphor green), and **Blade** (Blade Runner amber noir), switchable on-the-fly and persisted across sessions.
- `screensaver-matrix`: Interactive Matrix-style digital glyph rain screensaver triggering after user inactivity (default: active at 10 minutes), with customizable timeout or disable toggle, instantly dismissible on user input.
- `localization-i18n`: Multi-language interface supporting English (`en`), Italian (`it`), French (`fr`), Spanish (`es`), and Chinese (`zh`), with automatic language detection and graceful fallback to English.
- `baretail-migration`: Automatic detection of Windows Registry entries for BareTail/BareTailPro (`HKCU\Software\Bare Metal Software\BareTail`), interactive first-run prompt, and seamless import of recent files and color highlight rules into a cross-platform configuration file.
- `log-intelligence`: Smart log parsing featuring automatic JSON payload detection, inline expand/collapse formatted tree view, and multiline stack trace preservation.
- `filters-and-highlighting`: High-performance regex and literal search, live include/exclude filter trays (BareTailPro Filter Tail parity), and customizable line/token highlight rule sets.
- `telemetry-audio-fx`: Collapsible telemetry monitoring (CPU %, memory, stream KB/s, line counters) and retro-futuristic sound effects for notifications/errors (disabled by default).

### Modified Capabilities
<!-- Existing capabilities whose REQUIREMENTS are changing. Leave empty since this is a greenfield project. -->

## Impact

- **Language & Runtime**: 100% Rust, compiling to a single self-contained executable on Windows, Linux, and macOS.
- **Dependencies**: Native crates only (`egui`, `eframe`, `egui_dock`, `memmap2`, `notify`, `winreg` on Windows, `serde`, `serde_json`, optional lightweight audio crate). Zero external system runtimes required (no WebView2, no .NET, no Node/Electron).
- **Fonts & Assets**: Embedded monospace font supporting glyphs for Matrix rain and Chinese CJK characters.
- **Migration Path**: Frictionless transition for legacy BareTail users on Windows via Registry discovery.
