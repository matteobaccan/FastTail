## 1. Project Initialization & Dependencies

- [x] 1.1 Initialize Rust workspace (`Cargo.toml`) with cross-platform targets
- [x] 1.2 Add core dependencies: `eframe`, `egui`, `egui_dock`, `memmap2`, `notify`, `regex`, `serde`, `toml`, `serde_json`
- [x] 1.3 Add platform-conditional dependencies: `winreg` on Windows, lightweight audio crate behind an optional feature
- [x] 1.4 Embed cyber monospace font and CJK fallback font into the binary

## 2. Stream Engine & Out-of-Core File I/O

- [x] 2.1 Implement `TailEngine` using `memmap2` with 64-bit offsets for large files (>50GB)
- [x] 2.2 Implement background file watcher with `notify` and fallback polling for network drives
- [x] 2.3 Implement line offset indexing with sparse window cache to minimize RAM
- [x] 2.4 Add log rotation and truncation auto-recovery (file size decrease / inode change)
- [x] 2.5 Implement follow-tail toggle mechanism with auto-pause on scroll up

## 3. Cyberpunk HUD & Sci-Fi Themes

- [x] 3.1 Implement `CyberTheme` token system with Tron, Matrix, and Blade color palettes
- [x] 3.2 Build Tron theme styling (deep obsidian, neon cyan borders, electric blue accents)
- [x] 3.3 Build Matrix theme styling (pure black, phosphor green borders, olive highlights)
- [x] 3.4 Build Blade theme styling (dark industrial charcoal, warm amber/orange, neon magenta)
- [x] 3.5 Add dynamic theme switcher dropdown with settings persistence

## 4. Modular Docking Workspace

- [x] 4.1 Implement modular docking layout with `egui_dock` allowing panels to be moved, split, collapsed, or closed
- [x] 4.2 Implement virtual scrolling in the log view to render only lines currently visible in viewport at 60+ FPS
- [x] 4.3 Provide workspace layout presets: Full Cyber HUD vs Zen BareTail Mode
- [x] 4.4 Add tab management for monitoring multiple log files simultaneously

## 5. BareTail Windows Registry Migration & Config

- [x] 5.1 Implement Windows Registry reader for `HKCU\Software\Bare Metal Software\BareTail` and `BareTailPro`
- [x] 5.2 Build first-run interactive HUD dialog prompting the operator to import recent files and highlight rules
- [x] 5.3 Map BareTail Win32 RGB color definitions to FastTail color palettes
- [x] 5.4 Implement `fasttail.toml` configuration loader and serializer for persistent cross-platform settings

## 6. Filters, Highlighting & Search

- [x] 6.1 Implement Live Filter Tray with simultaneous Include and Exclude pattern inputs (regex and literal)
- [x] 6.2 Build multi-pattern highlight rule manager with user-customizable text/background colors and priority ordering
- [x] 6.3 Implement incremental search bar with `F3` / `Shift+F3` navigation between matches
- [x] 6.4 Render match heat indicators along the vertical scrollbar

## 7. Log Intelligence & Structured Viewing

- [x] 7.1 Implement fast inline JSON detection on log lines during tokenization
- [x] 7.2 Build interactive inline `[▼ JSON]` toggle with formatted tree / pretty-print display
- [x] 7.3 Implement multiline exception stack trace aggregator preserving trace blocks during filtering

## 8. Matrix Digital Rain Screensaver

- [x] 8.1 Implement user inactivity detection tracker (monitoring mouse and keyboard events)
- [x] 8.2 Build 2D Matrix digital rain simulation in `egui::Painter` with vertical falling glyph streams and fading tails
- [x] 8.3 Configure default activation at 10 minutes of idle time
- [x] 8.4 Add settings controls to toggle screensaver on/off and adjust idle timeout duration
- [x] 8.5 Ensure instantaneous dismissal upon any user input without visual lag

## 9. Localization (i18n) Engine

- [x] 9.1 Build lightweight string catalog translation engine supporting `en`, `it`, `fr`, `es`, and `zh`
- [x] 9.2 Implement automatic host OS locale detection on first launch
- [x] 9.3 Implement reliable fallback to English for any missing translation key
- [x] 9.4 Add UI language selector in top bar/settings with dynamic re-rendering
- [x] 9.5 Ensure proper CJK glyph rendering for Chinese characters

## 10. Telemetry & Audio FX

- [x] 10.1 Implement collapsible telemetry HUD widget monitoring CPU, memory, stream throughput (KB/s), and line count
- [x] 10.2 Implement cyber-terminal audio engine with embedded SFX (disabled by default in settings)
- [x] 10.3 Add audio triggers for error detection and file attachment when sound is enabled by the user

## 11. Build, Optimization & Packaging

- [x] 11.1 Configure Cargo release profile with LTO and binary size optimizations for standalone distribution
- [x] 11.2 Verify standalone execution on Windows, Linux, and macOS without external dependencies
