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
The About dialog SHALL provide clickable hyperlinks to the GitHub repository and to `https://www.baccan.it`, each showing its full URL as a hover tooltip, and SHALL show the git tag and build timestamp of the running binary. The build SHALL include the platform support that opens URLs in the operating system's default browser (the `links` feature of eframe), and a test SHALL fail if that support is dropped from the dependency declaration.

#### Scenario: User clicks website or repository link
- **WHEN** the user opens the About dialog and clicks the GitHub repository or `www.baccan.it` link
- **THEN** the default web browser opens the respective URL.

#### Scenario: Hovering a link
- **WHEN** the pointer rests on the `www.baccan.it` or the repository link
- **THEN** a tooltip shows the full `https://` URL that a click will open.

#### Scenario: Browser support removed from the build
- **WHEN** the `eframe` dependency in `Cargo.toml` no longer lists the `links` feature
- **THEN** the test suite fails with a message naming the missing feature.

### Requirement: Localized Tooltips
All interactive buttons, sliders, and controls SHALL provide informative tooltips fully localized in the currently selected user language (see the localization-i18n capability for the supported set).

#### Scenario: Tooltip follows the selected language
- **WHEN** the interface language is Italian and the user hovers over the follow-mode toggle
- **THEN** the tooltip is shown in Italian, and switching the language to French updates the tooltip text on the next hover.

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

#### Scenario: Shortcuts act on the focused stream only
- **WHEN** two streams are docked side by side and the user presses Ctrl + End
- **THEN** only the stream in the focused panel jumps to its last line and enables follow mode; the other panel keeps its scroll position.

### Requirement: Recent Files Menu (MRU)
The top navigation bar SHALL provide a Recent Files dropdown as an icon-only button (🕒) placed immediately to the right of the Open File button, whose tooltip SHALL read the localized "Recent Files" label. The dropdown SHALL list previously opened log files in Most Recently Used order, allowing instant reopening with a single click and a clear recent list option.

#### Scenario: Reopening a file from the recent list
- **WHEN** the user opens the Recent dropdown and clicks a previously opened log file
- **THEN** the file opens in a new stream tab and moves to the top of the Most Recently Used list.

#### Scenario: Icon button next to Open File
- **WHEN** the title bar is rendered in any language
- **THEN** the 🕒 button sits directly after the Open File button, before the Filter button, shows no text, and hovering it displays the "Recent Files" label of the current language.

### Requirement: Virtualized Scroll Rendering
The docking log stream SHALL use virtualized row scrolling to render only visible lines on screen, guaranteeing 60+ FPS performance even with tens of millions of lines in the buffer. Visible rows SHALL be computed directly from the active visible line set, utilizing 100% of the viewport height without empty gaps or truncated lines. When an include/exclude filter is active, only the matching rows are laid out so the viewport is always filled.
Row height SHALL scale proportionally with the selected font size (1.25x the font row height, with an 18 px readability minimum), maintaining compact and uniform interline spacing across the 8pt to 32pt zoom range.

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

#### Scenario: Modal chrome shows what it does
- **WHEN** the pointer is over the title strip of a dialog
- **THEN** the cursor becomes the move cursor over the draggable part and a pointing hand over the collapse and close buttons at its ends.

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
The dock layout, floating window positions and sizes, dialog positions and sizes, and the main window position, size and maximized state SHALL be persisted in the configuration file and restored on the next start. A saved window position that no longer falls on an attached monitor SHALL be ignored so the window is never restored off-screen. Geometry recorded for a floating dock window SHALL be discarded as soon as that window no longer exists, and looking it up SHALL never index a surface that is gone.

#### Scenario: Restart after disconnecting a monitor
- **WHEN** the window was last closed on a secondary monitor that is no longer attached
- **THEN** the next start ignores the saved position and opens the window on an attached monitor with its saved size and dock layout.

#### Scenario: Closing a floating dock window
- **WHEN** the user closes or re-docks a floating window whose position was recorded during the session
- **THEN** the application keeps running, the stale record is dropped, and the next layout save contains only windows that still exist.

### Requirement: Window Idle Detection
Mouse movement, mouse clicks, wheel scrolling and keyboard input SHALL all count as user activity for the screensaver idle timer.

#### Scenario: Wheel scrolling keeps the workspace awake
- **WHEN** the user only scrolls a stream with the mouse wheel for longer than the configured idle timeout
- **THEN** the screensaver does not start, because each wheel event resets the idle timer.

### Requirement: Always-On-Top Window
The title bar SHALL offer a pin toggle, mirrored by a Settings checkbox and by Ctrl+Shift+T, that keeps the main window above other windows. The state SHALL be persisted in `fasttail.ini` and applied at startup.

#### Scenario: Pinning the window
- **WHEN** the user clicks the pin and then focuses another application
- **THEN** the FastTail window stays visible above it, and the pin is highlighted.

#### Scenario: Restart with pin active
- **WHEN** FastTail was closed with the pin active
- **THEN** the next start opens the window already on top.

### Requirement: Background Tab Activity Badge
A stream tab that is not currently displayed SHALL show a badge with the number of lines appended since it was last displayed, capped at `999+`, coloured by the most severe highlight (or log level, when detected) among those lines. The badge SHALL clear when the tab is displayed again.

#### Scenario: Lines arrive in a hidden tab
- **WHEN** stream B is behind stream A in the same tab group and 42 lines are appended to B, one matching a rule with a Critical sound preset
- **THEN** B's tab shows `42` in the critical colour, and clicking B clears it.

### Requirement: Window Attention on Background Alerts
When enabled in Settings, a highlight rule with a sound preset matching in a stream that is not displayed while the window is unfocused SHALL request user attention from the OS (taskbar/dock flash).

#### Scenario: Alert while the window is in the background
- **WHEN** the option is on, the window is not focused, and a Critical rule matches
- **THEN** the OS attention request is sent once, and not again until the window has been focused.

### Requirement: Line Wrap Mode
Each text stream SHALL offer a wrap toggle (stream bar button and Alt+W) that soft-wraps rows at the viewport width. Wrapped rows SHALL keep their line number, marker and highlight, navigation by line index (search, bookmarks, go-to, paging) SHALL keep working, and the toggle state SHALL be persisted per stream in the workspace.

#### Scenario: Wrapping a long JSON line
- **WHEN** wrap is enabled on a stream containing a 3,000-character line
- **THEN** the line is displayed on several rows within the viewport width, with no horizontal scrollbar, and the line number is shown once.

#### Scenario: Search jump in wrap mode
- **WHEN** wrap is enabled and the user presses F3
- **THEN** the viewport scrolls so that the matching line is fully visible.

### Requirement: Visible Zoom Level
The log font size doubles as the application zoom (`Ctrl+`, `Ctrl-`, `Ctrl+0` and `Ctrl+wheel`), so the current level SHALL be shown as a percentage of the default size next to the always-on-top pin in the title bar, and beside the point size in the settings. Clicking the indicator SHALL reset the zoom to 100%.

#### Scenario: Accidental zoom is explained
- **WHEN** the user scrolls the wheel with `Ctrl` held and the text changes size
- **THEN** the title bar shows the new percentage in the accent colour, and a click on it restores 100%.

### Requirement: Stream Toolbar Affordances
The stream toolbar SHALL show the state of its toggles (follow, monitor, line numbers, wrap, TXT/HEX/MD) with a tinted fill and a border, not with the label colour alone, so the active state is readable on the light theme as well. The TXT/HEX/MD switcher SHALL keep a fixed position in the toolbar, before the controls that appear and disappear with the view mode, so it does not move under the pointer when the mode changes.

#### Scenario: Active toggle on the light theme
- **WHEN** the light theme is active and line wrap is enabled
- **THEN** the wrap button is drawn filled and outlined in the accent colour, clearly distinct from the inactive buttons next to it.

#### Scenario: Switching to HEX does not move the switcher
- **WHEN** the user switches a stream from text to HEX, and the line-number and wrap buttons disappear
- **THEN** the TXT/HEX/MD buttons stay where they were.

### Requirement: Named Sessions
The application SHALL save the workspace (open files and patterns, dock layout, floating windows, per-stream filters, search queries, wrap, encoding, bookmarks) to a named session file and load it back, replacing the current workspace after confirmation. Global preferences SHALL NOT be part of a session. The title bar SHALL show the session name and a `*` when the workspace differs from the saved session. Paths SHALL be stored absolute and, when possible, relative to the session file so moved bundles still open. Missing files SHALL be skipped with a summary. Recent sessions SHALL be listed in a menu.

#### Scenario: Switching projects
- **WHEN** the user saves the current five tabs as `incident.fasttail-session.ini`, then loads `dev.fasttail-session.ini`
- **THEN** the five tabs are closed, the dev session's tabs and layout are restored, and the title bar shows `dev`.

#### Scenario: Session moved with its logs
- **WHEN** a session saved in `bundle/` referencing `bundle/app.log` is moved together with the folder to another drive
- **THEN** loading it opens `app.log` from the new location.

