# Cyberpunk UI & Docking Workspace Specification

## Purpose
Provides a responsive, high-contrast Cyberpunk docking UI with virtualized rendering, draggable modal windows, theme customizability, and intuitive keyboard navigation.

## Requirements

### Requirement: Cyberpunk UI Styling and Branding
The application SHALL present a clean, high-contrast Cyberpunk UI featuring customizable themes (Tron, Matrix, Blade, Light), without double slashes (//) in UI labels. Interactive buttons SHALL maintain zero expansion (`expansion = 0.0`) and fixed dimensions on hover, preventing layout shifts and footprint jitter. The main application window title SHALL be `FastTail v<version> by Matteo Baccan`.

#### Scenario: Button hover maintains stable footprint
- **WHEN** the user hovers over window controls, navigation buttons, or stream toggles
- **THEN** the button visuals highlight without expanding or altering surrounding layout geometry.

### Requirement: Titlebar Dragging and Move Cursor
When hovering over or dragging the main application titlebar to reposition the window, the cursor SHALL be set to the 4-directional move cursor.

#### Scenario: User hovers over titlebar drag region
- **WHEN** the mouse hovers over the titlebar region not occupied by buttons
- **THEN** the cursor icon switches to the move cursor and dragging moves the window smoothly.

### Requirement: Header Application Version Display
The main application window titlebar SHALL display the current crate version alongside the brand name and author.

#### Scenario: Window titlebar displays version
- **WHEN** the application opens
- **THEN** the top navigation bar displays `FastTail v<version> by Matteo Baccan`.

### Requirement: Clickable External Hyperlinks in About Dialog
The About dialog SHALL provide clickable hyperlinks to the GitHub repository and to `https://www.baccan.it`, and SHALL show the git tag and build timestamp of the running binary.

#### Scenario: User clicks website or repository link
- **WHEN** the user opens the About dialog and clicks the GitHub repository or `www.baccan.it` link
- **THEN** the default web browser opens the respective URL.

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
The docking log stream SHALL use virtualized row scrolling to render only visible lines on screen, guaranteeing 60+ FPS performance even with tens of millions of lines in the buffer. Visible rows SHALL be computed directly from the active visible line set, utilizing 100% of the viewport height without empty gaps or truncated lines. When an include/exclude filter is active, only the matching rows are laid out so the viewport is always filled. Row height SHALL scale proportionally with the selected font size (1.25x the font row height, with an 18 px readability minimum), maintaining compact and uniform interline spacing across the 8pt to 32pt zoom range.

#### Scenario: Scrolling through filtered lines
- **WHEN** an include or exclude filter reduces the number of visible lines
- **THEN** only matching lines are rendered consecutively without blank gaps, and the scrollbar bounds match the filtered line count.

#### Scenario: Small font rendering remains compact
- **WHEN** the user sets the font size below the default (e.g. 10pt or 8pt)
- **THEN** the line row height decreases with the text without extra blank gaps between lines.

### Requirement: Draggable Modal Windows
Modal dialogs (Settings, Highlights, About, Help) SHALL open centered in the viewport by default and SHALL allow the operator to freely move and drag them by their title bar.

#### Scenario: User repositions a modal dialog
- **WHEN** a modal dialog opens at the center of the screen and the user drags its title bar
- **THEN** the window moves smoothly to the dragged position without snapping back to center.

### Requirement: Dynamic Active File Status Footer
The application bottom panel SHALL display exclusively the absolute path of the currently active log tab, updating immediately whenever the user switches active tabs, without redundant status or filter counters.

#### Scenario: Switching tabs updates the footer path
- **WHEN** the user switches focus from Tab A to Tab B
- **THEN** the footer instantly displays the full absolute filesystem path of Tab B.

### Requirement: Stream View Header De-duplication
The log stream pane SHALL NOT render a redundant internal banner with the file name and path, maximizing vertical viewing area and placing stream action controls directly at the top of the tab.

#### Scenario: Tab opens with direct stream controls
- **WHEN** a log stream is displayed in a dock tab
- **THEN** the stream controls appear immediately below the tab header without an internal title banner.

### Requirement: Persisted Workspace Geometry
The dock layout, floating window positions and sizes, dialog positions and sizes, and the main window position, size and maximized state SHALL be persisted in the configuration file and restored on the next start. A saved window position that no longer falls on an attached monitor SHALL be ignored so the window is never restored off-screen.

### Requirement: Window Idle Detection
Mouse movement, mouse clicks, wheel scrolling and keyboard input SHALL all count as user activity for the screensaver idle timer.
