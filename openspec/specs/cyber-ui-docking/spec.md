# Cyberpunk UI & Docking Workspace Specification

## Purpose
Provides a responsive, high-contrast Cyberpunk docking UI with virtualized rendering, draggable modal windows, theme customizability, and intuitive keyboard navigation.

## Requirements

### Requirement: Cyberpunk UI Styling and Branding
The application SHALL present a clean, high-contrast Cyberpunk UI featuring customizable sci-fi themes (Tron, Matrix, Blade), without double slashes (//) in UI labels. The main application window title SHALL be FastTail by Matteo Baccan.

### Requirement: Localized Tooltips
All interactive buttons, sliders, and controls SHALL provide informative tooltips fully localized in the currently selected user language (English, Italian, French, Spanish, Chinese).

### Requirement: Keyboard Navigation & Hotkeys
The log stream viewport SHALL support comprehensive keyboard navigation:
- Home / End: Scroll horizontally to the far left (Home) or far right (End).
- PgUp / PgDown: Scroll up or down by one viewport page.
- Ctrl + Home: Jump directly to line 0 (top of buffer) and pause follow mode.
- Ctrl + End: Jump directly to the latest line (bottom of buffer) and enable follow mode.
- Ctrl + F: Focus the on-the-fly search input box in the active stream bar.
- F3 / Shift + F3: Jump to next / previous search match.
- Ctrl + / Ctrl =: Increase font size (zoom in).
- Ctrl -: Decrease font size (zoom out).
- Ctrl 0: Reset font size to default (13 pt).
- Ctrl + MouseWheel: Dynamically adjust font size.
- F1: Open the Keyboard Shortcuts & Help modal.

### Requirement: Recent Files Menu (MRU)
The top navigation bar SHALL provide a Recent Files dropdown (🕒 Recent) listing previously opened log files in Most Recently Used order, allowing instant reopening with a single click and a clear recent list option.

### Requirement: Virtualized Scroll Rendering
The docking log stream SHALL use virtualized row scrolling to render only visible lines on screen, guaranteeing 60+ FPS performance even with tens of millions of lines in the buffer.
