## Purpose
Provides a responsive, high-contrast Cyberpunk docking UI with draggable window controls, header version information, clickable links, and seamless virtual scroll rendering.

## Requirements

### Requirement: Titlebar Dragging and Move Cursor
When hovering over or dragging the main application titlebar to reposition the window, the cursor SHALL be set to the 4-directional move cursor (`CursorIcon::Move`).

#### Scenario: User hovers over titlebar drag region
- **WHEN** the mouse hovers over the titlebar region not occupied by buttons
- **THEN** the cursor icon switches to `Move` and dragging moves the window smoothly.

### Requirement: Header Application Version Display
The main application window titlebar SHALL display the current application version alongside the brand name and author.

#### Scenario: Window titlebar displays version
- **WHEN** the application opens
- **THEN** the top navigation bar displays `FastTail v0.1.0 by Matteo Baccan` (or current crate version).

### Requirement: Clickable External Hyperlinks in About Dialog
The About dialog SHALL provide clickable hyperlinks to the GitHub repository and Matteo Baccan's official website.

#### Scenario: User clicks website or repository link
- **WHEN** the user opens the About dialog and clicks on the GitHub repository or `www.baccan.it` link
- **THEN** the default web browser opens the respective URL.

### Requirement: Gap-Free Virtualized Scroll Rendering
Virtual scroll rendering SHALL compute visible rows directly from the active visible line set, utilizing 100% of the viewport height without empty gaps or truncated lines.

#### Scenario: Scrolling through filtered lines
- **WHEN** an include or exclude filter reduces the number of visible lines
- **THEN** only matching lines are rendered consecutively without blank gaps, and the scrollbar bounds match the filtered line count.
