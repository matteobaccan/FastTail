<div align="center">
  <img src="assets/logo.svg" alt="FastTail Logo" width="700" />

  <p><strong>Next-generation ultra-fast multi-stream log monitor & tail viewer with Cyberpunk UI aesthetic.</strong></p>

  <p>
    <a href="https://github.com/matteobaccan/FastTail/actions"><img src="https://img.shields.io/badge/build-passing-brightgreen?style=flat-square" alt="Build Status" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square" alt="MIT License" /></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/language-Rust-orange.svg?style=flat-square" alt="Rust 2021" /></a>
    <a href="https://github.com/matteobaccan/FastTail"><img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey?style=flat-square" alt="Platforms" /></a>
  </p>
</div>

---

## ⚡ Overview

**FastTail** is a modern, high-performance cross-platform log tailing application written in Rust. Designed as the ultimate successor to legacy tools like BareTail, it combines **zero-lag 64-bit memory-mapped file I/O** (memmap2) with a stunning **Cyberpunk UI** aesthetic, rich log intelligence, audio alert presets, and modular docking layouts.

Whether you are monitoring multi-gigabyte production server logs, analyzing raw binary firmware streams in Hex mode, or isolating errors using priority-ordered regex filters, FastTail delivers instant responsiveness with minimal memory footprint.

---

## 🚀 Feature Highlights

- **Zero-Lag Memory-Mapped Engine (memmap2)**: Instantly open multi-gigabyte log files (>50 GB) in milliseconds using less than 50 MB of RAM.
- **Cyberpunk UI Themes**: Switch seamlessly between **Tron** (Obsidian & Neon Cyan), **Matrix** (Phosphor Green & Jet Black), and **Blade** (Charcoal & Amber/Magenta).
- **Multi-Mode Log Viewing**:
  - **TXT (Text Mode)**: Full-featured text streaming with virtualized scrolling.
  - **HEX (Binary Mode)**: Real-time hexadecimal dump with configurable byte columns in multiples of 8 (16, 24, 32...).
  - **FILTERED Mode**: Dedicated view showing exclusively lines matching your active filters.
- **Multi-Encoding Support**: Automatic detection and manual selection for **ASCII**, **ANSI (Windows-1252)**, **UTF-8** (with/without BOM), **Unicode LE (UTF-16 LE)**, and **Unicode BE (UTF-16 BE)**.
- **Multi-Rule Color Highlighting & Styles**: Configure color rules with **Bold**, **Italic**, and top-down evaluation priority (reorder with ⬆/⬇).
- **Sound Alert Presets**: Assign audio notifications (Beep, Chime, Warning, Critical) to highlight triggers.
- **Modular Docking Workspace (gui_dock)**: Dock, split horizontally/vertically, float, or tabulate multiple simultaneous log streams.
- **Log Intelligence**: Automatic inline [+] JSON detection with interactive pretty-printing and multiline stack trace preservation.
- **Matrix Digital Rain Screensaver**: Built-in 2D falling glyph animation when idle for an operator-configured timeout.
- **BareTail Migration Bridge**: One-click import of registry configurations, highlight colors, and recent files from BareTail and BareTailPro on Windows.
- **Multilingual Support (i18n)**: English, Italian, French, Spanish, and Chinese with native CJK font fallback.

---

## 📊 Comparison with Other Tail Tools

| Feature | FastTail | BareTail (Free/Pro) | Tailviewer | SnakeTail | 	ail -f / CLI |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Engine / Architecture** | **Rust (memmap2)** | Win32 C++ (2006) | .NET / C# | C# / WPF | POSIX C |
| **Large Files (>50 GB)** | **Instant (<100ms)** | Good | Slow / High RAM | Moderate | Fast |
| **Cross-Platform** | **Windows, Linux, macOS** | Windows only | Windows only | Windows only | Linux/macOS |
| **User Interface** | **Cyberpunk UI (GPU)** | Win32 Classic | Modern Windows | Classic Windows | Terminal CLI |
| **Multi-Tab / Docking** | **Full Modular Docking** | Tabs only | Tabs & Panels | Tabs & Splits | Multiple terms |
| **Binary Hex View** | **Yes (Multiples of 8)** | No | Plugin required | No | No (xxd) |
| **Filtered View Mode** | **Yes (Direct Stream)** | Pro version only | Yes | Yes | grep pipe |
| **Highlighting Styles** | **FG, BG, Bold, Italic** | FG, BG | FG, BG | FG, BG | ANSI codes |
| **Rule Priority Reordering**| **Yes (⬆ / ⬇ Top-Down)** | Limited | Yes | Yes | N/A |
| **Sound Alerts** | **Presets (Beep/Chime/Crit)** | No | Plugins | Limited | Bell (\a) |
| **Encoding Support** | **ASCII, ANSI, UTF-8, UTF-16 LE/BE** | ANSI, UTF-8, Unicode | UTF-8, ANSI | UTF-8, ANSI | Terminal enc |
| **JSON Formatter** | **Inline Pretty-Print** | No | Plugin required | No | jq pipe |
| **Screensaver Mode** | **Matrix Digital Rain** | No | No | No | No |
| **Open Source & License** | **MIT License** | Proprietary | MIT | GPL | Open Source |

---

## ⌨️ Keyboard Shortcuts Reference

| Shortcut | Description |
| :--- | :--- |
| `Space` | Toggle Follow mode (Auto-scroll to latest line) |
| `Ctrl + F` | Focus search bar in active log stream |
| `F3` / `Shift + F3` | Navigate to Next / Previous search match |
| `Home` | Scroll horizontally to the far left |
| `End` | Scroll horizontally to the far right |
| `PgUp` / `PgDown` | Scroll viewport up / down by one page |
| `Ctrl + Home` | Jump to line 0 (top of file) and pause follow |
| `Ctrl + End` | Jump to latest line (bottom of file) and resume follow |
| `Ctrl +` / `Ctrl =` | Zoom in (Increase font size) |
| `Ctrl -` | Zoom out (Decrease font size) |
| `Ctrl 0` | Reset font size to default (13 pt) |
| `Ctrl + MouseWheel` | Dynamically scale font size |
| `F1` | Open Help & Keyboard Shortcuts dialog |
| `Esc` | Close active dialog or popup |

---

## 🛠️ Building & Installation

### Prerequisites
- [Rust](https://rustup.rs/) (version 1.80+ recommended)

### Build from Source
```bash
git clone https://github.com/matteobaccan/FastTail.git
cd FastTail

# Run development build
cargo run

# Build optimized release binary
cargo build --release
```

The compiled standalone executable will be located at `target/release/fasttail` (or `target/release/fasttail.exe` on Windows).

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.

Copyright (c) 2026 **Matteo Baccan**
