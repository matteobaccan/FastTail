<div align="center">
  <img src="assets/logo.svg" alt="FastTail Logo" width="700" />

  <p><strong>Next-generation ultra-fast multi-stream log monitor & tail viewer with Cyberpunk UI aesthetic.</strong></p>

  <p>
    <a href="https://github.com/matteobaccan/FastTail/actions/workflows/build.yml"><img src="https://github.com/matteobaccan/FastTail/actions/workflows/build.yml/badge.svg" alt="Build Status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="MIT License" /></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/language-Rust-orange.svg?style=flat-square" alt="Rust 2021" /></a>
    <a href="https://github.com/matteobaccan/FastTail/releases"><img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platforms" /></a>
  </p>

  <p>
    <img src="assets/screenshots/fasttail_main_view.png" alt="FastTail Main Interface" width="850" />
  </p>
</div>

---

## ⚡ Overview

**FastTail** is a modern, high-performance cross-platform log tailing application written in Rust. Designed as the successor to legacy tools like BareTail, it combines a **zero-lag 64-bit streaming engine** with a **Cyberpunk UI**, rich log intelligence, audio alert presets and a modular docking workspace.

Whether you are monitoring multi-gigabyte production logs, inspecting raw binary streams in Hex mode, reading Markdown or HTML reports, or isolating errors with priority-ordered highlight rules, FastTail stays instantly responsive with a minimal memory footprint.

---

## 🚀 Feature Highlights

- **Zero-Lag Streaming Engine**: never holds the file in memory. Rows are read on demand through a 4 MB block cache per stream; the only per-file state is the line index (8 bytes per line). Appends are indexed incrementally, log rotation, truncation and in-place rewrites are detected without locking the file for the writer, and files above 16 MB run filters and search on a worker thread with progress shown in the stream bar (above 256 MB the initial index too). Ten 50 MB logs growing continuously cost about 60 MB of RAM and a few milliseconds per frame.
- **Three View Modes per Stream**:
  - **TXT**: virtualized text view with highlight rules, inline JSON pretty-printing and stack-trace grouping.
  - **HEX**: live hexadecimal + ASCII dump with byte columns in multiples of 8 (16, 24, 32...).
  - **MD**: rendered Markdown. `.md` files open in this mode automatically and HTML documents are converted to Markdown on the fly.
- **Powerful Search**: a search box per stream, `F3` / `Shift+F3` navigation scoped to the focused window, match counter, wrap-around beep, last 10 queries history, matches refreshed live as the file grows. In HEX mode the query is matched at byte level (text or `0A 0D` patterns), and a marker column (`▶` current hit, `●` other hits) plus full-row highlight is shown in every view.
- **Live Include / Exclude Filters**: plain text or regex, case-sensitive or not, applied as you type. Filtered rows are virtualized so the viewport is always full.
- **Multi-Rule Highlighting**: foreground, background, **bold**, *italic*, top-down priority (reorder with ⬆ / ⬇), and a sound alert preset per rule (Beep, Chime, Warning, Critical).
- **Capture-Group Highlighting and Quick Labels**: a regex rule can paint only its capture groups (`req=(\d+)` colours the request id, not the row) with "Captures only"; the first rule wins per byte, top-down, and at most 64 spans are painted per row. `Ctrl+Shift+1..9` turns the current search text into a quick colour label with preset colour 1..9, painted in every stream and listed in a strip above the rows with a remove button; labels rank below the rules and are not saved across restarts.
- **Cyberpunk Themes**: **Tron** (obsidian & neon cyan), **Matrix** (phosphor green), **Blade** (charcoal & amber/magenta) and a clean **Light** theme.
- **Modular Docking Workspace** (egui_dock): dock, split, float or tab any number of streams. Layout, floating window positions, dialog positions and window geometry are persisted and restored.
- **Multi-Encoding Support**: automatic detection and manual override for ASCII, ANSI (Windows-1252), UTF-8 (with/without BOM), UTF-16 LE and UTF-16 BE.
- **Log Intelligence**: inline `[+] JSON` detection with pretty-printing, multiline stack-trace continuation kept together with its parent line.
- **Log Level Detection**: the level of every line (`FATAL`, `ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`, plus syslog `<n>` priorities) is detected from the common layouts without configuration. Rows are coloured by level when no highlight rule matches them (switchable in Settings), a `≥ level` selector above the buffer filters by minimum level together with the include / exclude filters, and the stream bar counts the lines per level live.
- **Directory Wildcard Tail**: open `C:\logspp-*.log` (type it in the `📂*` prompt, drop a folder on the window, or pass it on the command line) and the stream follows the newest file matching the pattern, switching by itself when the logger rotates to a new day or hour. Filters, highlight rules, search and wrap survive the switch; the stream bar shows the pattern, the current file and a "switched to" notice. The pattern, not the resolved file, is what the workspace and the recent list remember.
- **Line Wrap**: a per-stream `↩ Wrap` toggle (`Alt+W`) soft-wraps long lines (JSON payloads, stack traces, URLs) at the window width instead of scrolling horizontally. Wrapped rows keep their line number, marker and colours; search, bookmarks, go-to and paging still navigate by line. Only the rows in view are laid out, so wrapping stays cheap on huge files; the scroll bar thumb is approximate in wrap mode. The toggle is saved per file.
- **Telemetry & FX**: CPU and memory in the title bar, per-stream throughput, optional borderless window, Matrix digital rain screensaver after a configurable idle time (10 minutes by default).
- **BareTail Migration Bridge**: one-click import of recent files and highlight colors from BareTail / BareTailPro on Windows.
- **Multilingual UI**: English, Italian, French, Spanish and Chinese, with CJK font fallback.
- **Crash Logger**: an unexpected panic writes `fasttail_crash.log` with version, commit and build timestamp so it can be reported.

---

## 📊 Comparison with Other Tail Tools

| Feature | FastTail | BareTail (Free/Pro) | Tailviewer | SnakeTail | `tail -f` / CLI |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Engine / Architecture** | **Rust** | Win32 C++ (2006) | .NET / C# | C# / WPF | POSIX C |
| **Binary Size** | **~10 MB (single binary)** | ~220 KB | ~45 MB | ~1.5 MB | ~50 KB |
| **Runtime Dependencies** | **Zero (standalone native)** | Zero (Win32 native) | .NET Runtime required | .NET Framework / WPF | POSIX coreutils |
| **Large Files (>50 GB)** | **Instant** | Good | Slow / High RAM | Moderate | Fast |
| **Cross-Platform** | **Windows, Linux, macOS** | Windows only | Windows only | Windows only | Linux/macOS |
| **User Interface** | **Cyberpunk UI (GPU)** | Win32 Classic | Modern Windows | Classic Windows | Terminal CLI |
| **Multi-Tab / Docking** | **Full modular docking** | Tabs only | Tabs & Panels | Tabs & Splits | Multiple terms |
| **Binary Hex View** | **Yes, with byte-level search** | No | Plugin required | No | No (`xxd`) |
| **Markdown / HTML View** | **Yes** | No | No | No | No |
| **Line Wrap** | **Per stream (`Alt+W`), navigation by line kept** | Yes | No | No | Terminal wrap |
| **Directory Wildcard Tail** | **`app-*.log` follows the newest match, switch keeps filters** | No | No | Yes | `tail -F` one file |
| **Include / Exclude Filters** | **Live, text or regex** | Pro version only | Yes | Yes | `grep` pipe |
| **Log Level Detection** | **Built-in: colouring, `≥ level` filter, counters** | No | Yes | No | N/A |
| **Highlighting Styles** | **FG, BG, Bold, Italic** | FG, BG | FG, BG | FG, BG | ANSI codes |
| **Capture-Group Highlight / Quick Labels** | **Captures-only rules, `Ctrl+Shift+1..9` labels** | No | No | No | No |
| **Rule Priority Reordering**| **Yes (⬆ / ⬇ top-down)** | Limited | Yes | Yes | N/A |
| **Sound Alerts** | **Presets (Beep/Chime/Crit)** | No | Plugins | Limited | Bell (`\a`) |
| **Encoding Support** | **ASCII, ANSI, UTF-8, UTF-16 LE/BE** | ANSI, UTF-8, Unicode | UTF-8, ANSI | UTF-8, ANSI | Terminal enc |
| **JSON Formatter** | **Inline pretty-print** | No | Plugin required | No | `jq` pipe |
| **Screensaver Mode** | **Matrix digital rain** | No | No | No | No |
| **Open Source & License** | **MIT License** | Proprietary | MIT | GPL | Open Source |

---

## ⌨️ Keyboard Shortcuts Reference

Search and navigation shortcuts act on the stream in the **focused dock panel** (click a panel to focus it).

| Shortcut | Description |
| :--- | :--- |
| `Space` | Toggle Follow mode (auto-scroll to the latest line) |
| `Ctrl + F` | Focus the search box of the focused stream |
| `F3` / `Shift + F3` | Next / previous search match in the focused stream |
| `Enter` / `Shift + Enter` | Next / previous match while typing in the search box |
| `↑` / `↓` / `←` / `→` | Scroll by one line / column (`Ctrl` + `←` / `→` scrolls 5x faster) |
| `PgUp` / `PgDown` | Scroll by one page |
| `Home` / `End` | Scroll horizontally to the far left / right |
| `Ctrl + Home` | Jump to the top of the file and pause follow |
| `Ctrl + End` | Jump to the latest line and resume follow |
| `Click` / `Shift + Click` / `Ctrl + Click` | Select a row / extend the selection over the visible rows / toggle a row |
| `Ctrl + A` | Select every visible row of the focused stream |
| `Ctrl + C` | Copy the selected rows (or the current search hit) as plain text |
| `Ctrl + G` | Go to line N, or `+N` / `-N` from the current line (hidden lines resolve to the next visible one) |
| `Ctrl + Shift + T` | Toggle always-on-top (also the 📌 pin in the title bar and a Settings checkbox) |
| `Ctrl + F2` / `F2` / `Shift + F2` | Bookmark the current row (`★` in the marker column) / jump to the next / previous bookmark, wrapping around; bookmarks are saved per file |
| `Alt + W` | Toggle line wrap for the focused stream (also the `↩ Wrap` button in the stream bar); saved per file |
| `Ctrl + Shift + 1..9` | Create, recolour or remove the quick colour label (preset 1..9) for the current search text of the focused stream; labels apply to every stream and are listed above the rows |
| `Alt + 1..9` | Switch to stream tab #1 through #9 |
| `Ctrl +` / `Ctrl =` | Zoom in (increase font size) |
| `Ctrl -` | Zoom out (decrease font size) |
| `Ctrl 0` | Reset font size to default (13 pt) |
| `Ctrl + MouseWheel` | Dynamically scale font size |
| `F1` | Open the Help & Keyboard Shortcuts dialog |
| `Esc` | Close the active dialog, or leave the search box |

Files can also be opened by **drag & drop** onto the window or from the command line:

```text
fasttail [OPTIONS] [PATH...]

  PATH...            log files to open in addition to the restored workspace
  --fresh            start with an empty workspace instead of the saved one
  --filter <TEXT>    include filter for the files opened from the command line
  --exclude <TEXT>   exclude filter for those files
  --follow / --no-follow
                     follow mode for those files
  --renderer <NAME>  auto (default), glow or wgpu
  --config <FILE>    configuration file to use (same as FASTTAIL_CONFIG)
  -V, --version      print the version and exit
  -h, --help         print the usage and exit
```

Example: `fasttail --fresh --filter ERROR app.log err.log`.

---

## ⚙️ Configuration

Settings are stored in a single `fasttail.ini` file, looked up in this order:

1. the path in the `FASTTAIL_CONFIG` environment variable, if set;
2. `fasttail.ini` in the current working directory (portable layout);
3. `fasttail.ini` next to the executable;
4. the per-user directory: `%APPDATA%\FastTail` on Windows, `$XDG_CONFIG_HOME/FastTail` or `~/.config/FastTail` elsewhere.

New installs write next to the executable and fall back to the per-user directory when that folder is read-only (for example `Program Files`). A legacy `fasttail.toml` from older versions is migrated automatically on first start.

### Rendering backend
FastTail starts on `wgpu` (Direct3D 12 or Vulkan on Windows, Vulkan on Linux, Metal on macOS) and, if that backend cannot be created, retries automatically with OpenGL. wgpu is the default because it is the cheapest per frame: on an NVIDIA Windows machine a continuous repaint costs about 12% of one core on wgpu against a full core on OpenGL, whose driver busy-waits for the vertical blank; for that reason the OpenGL path runs without vsync. The status bar shows which backend is active: `WGPU`, `GL`, or `GL fallback` when the retry happened; hover it, or open About, for the adapter details.

| Setting | Values | Where |
|---|---|---|
| `renderer` in `fasttail.ini` | `auto` (default), `glow`, `wgpu` | Settings dialog, applies at the next start |
| `FASTTAIL_RENDERER` | same values, overrides the config | environment, useful for support: `FASTTAIL_RENDERER=wgpu fasttail` |

When the first backend fails, the error is printed to stderr together with `renderer: falling back to OpenGL`.

---

## 🛠️ Building & Installation

### Prerequisites
- [Rust](https://rustup.rs/) 1.87 or newer
- On Linux: `libasound2-dev libudev-dev pkg-config libx11-dev libxcb1-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libwayland-dev`

### Build from Source
```bash
git clone https://github.com/matteobaccan/FastTail.git
cd FastTail

# Run development build (optionally with files to open)
cargo run -- app.log other.log

# Run the test suite
cargo test

# Build optimized release binary
cargo build --release
```

The standalone executable is written to `target/release/fasttail` (`target/release/fasttail.exe` on Windows).

### Benchmarks
`cargo bench` runs `benches/filter_bench.rs`, which times the engine hot paths (indexing, include/exclude and regex filters, search, highlight scanning) on a synthetic 200 MB log generated in a temporary directory. It is built only on demand, never by `cargo test`.

| Variable | Effect |
|---|---|
| `FASTTAIL_BENCH_LOG=<file>` | benchmark an existing log instead of generating one |
| `FASTTAIL_BENCH_BYTES=<n>` | size of the generated log (default 200000000; 1100000000 was used for the LTO decision) |
| `FASTTAIL_BENCH_ROUNDS=<n>` | rounds per phase, best time reported (default 3) |

### Prebuilt binaries
Tagged versions are published on the [Releases](https://github.com/matteobaccan/FastTail/releases) page as one archive per platform:

| Platform | Asset | Contents |
|---|---|---|
| Windows x86_64 | `fasttail-windows-x86_64.zip` | `fasttail.exe` + `fasttail.pdb` |
| Linux x86_64 | `fasttail-linux-x86_64.tar.gz` | `fasttail` |
| Linux ARM64 | `fasttail-linux-arm64.tar.gz` | `fasttail` |
| macOS Apple Silicon | `fasttail-macos-arm64.tar.gz` | `fasttail` |

```bash
# Linux / macOS
tar -xzf fasttail-linux-x86_64.tar.gz && ./fasttail app.log

# Windows (PowerShell)
Expand-Archive fasttail-windows-x86_64.zip -DestinationPath fasttail; .\fasttail\fasttail.exe app.log
```

Keep `fasttail.pdb` next to `fasttail.exe`: it lets a crash report (`fasttail_crash.log`) show function names. Every push and pull request runs the test suite in [GitHub Actions](https://github.com/matteobaccan/FastTail/actions); release binaries are built only for `v*` tags and manual workflow runs.

---

## 📐 Specifications

Behaviour is documented as [OpenSpec](https://github.com/Fission-AI/OpenSpec) specifications under [`openspec/specs`](openspec/specs): stream engine, search and navigation, filters and highlighting, docking UI, themes, localization, log intelligence, screensaver, telemetry, BareTail migration, crash reporting, the release pipeline, the rendering backend, the command line, selection and export.

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2026 **Matteo Baccan**
