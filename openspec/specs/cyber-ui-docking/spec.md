# Cyberpunk UI & Docking Workspace Specification

## Purpose
Provides a responsive, high-contrast Cyberpunk docking UI with virtualized rendering, draggable modal windows, theme customizability, and intuitive keyboard navigation.

## Requirements

### Requirement: Cyberpunk UI Styling and Branding
The application SHALL present a clean, high-contrast Cyberpunk UI featuring customizable themes (Tron, Matrix, Blade, Light), without double slashes (//) in UI labels. The main application window title SHALL be `FastTail v<version> by Matteo Baccan`.

### Requirement: Localized Tooltips
All interactive buttons, sliders, and controls SHALL provide informative tooltips fully localized in the currently selected user language (English, Italian, French, Spanish, Chinese).

### Requirement: Keyboard Navigation & Hotkeys
The log stream viewport SHALL support comprehensive keyboard navigation. Stream-level shortcuts act on the stream in the focused dock panel only (see the Search and Navigation specification):
- Home / End: Scroll horizontally to the far left (Home) or far right (End).
- Arrow keys: Scroll by one line or column; Ctrl + Left / Right scrolls horizontally 5x faster.
- PgUp / PgDown: Scroll up or down by one viewport page.
- Ctrl + Home: Jump directly to line 0 (top of buffer) and pause follow mode.
- Ctrl + End: Jump directly to the latest line (bottom of buffer) and enable follow mode.
- Ctrl + F: Focus the search input box of the focused stream.
- F3 / Shift + F3: Jump to next / previous search match of the focused stream.
- Alt + 1..9: Activate stream tab #1 through #9.
- Space: Toggle follow mode.
- Esc: Close the active dialog, or leave the search box.
- Ctrl + / Ctrl =: Increase font size (zoom in).
- Ctrl -: Decrease font size (zoom out).
- Ctrl 0: Reset font size to default (13 pt).
- Ctrl + MouseWheel: Dynamically adjust font size.
- F1: Open the Keyboard Shortcuts & Help modal.

### Requirement: Recent Files Menu (MRU)
The top navigation bar SHALL provide a Recent Files dropdown (🕒 Recent) listing previously opened log files in Most Recently Used order, allowing instant reopening with a single click and a clear recent list option.

### Requirement: Virtualized Scroll Rendering
The docking log stream SHALL use virtualized row scrolling to render only visible lines on screen, guaranteeing 60+ FPS performance even with tens of millions of lines in the buffer. When an include/exclude filter is active, only the matching rows are laid out so the viewport is always filled.

### Requirement: Persisted Workspace Geometry
The dock layout, floating window positions and sizes, dialog positions and sizes, and the main window position, size and maximized state SHALL be persisted in the configuration file and restored on the next start. A saved window position that no longer falls on an attached monitor SHALL be ignored so the window is never restored off-screen.

### Requirement: Window Idle Detection
Mouse movement, mouse clicks, wheel scrolling and keyboard input SHALL all count as user activity for the screensaver idle timer.
