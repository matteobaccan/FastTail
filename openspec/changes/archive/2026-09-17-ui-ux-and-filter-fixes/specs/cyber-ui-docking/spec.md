## MODIFIED Requirements

### Requirement: Cyberpunk UI Styling and Branding
The application SHALL present a clean, high-contrast Cyberpunk UI featuring customizable sci-fi themes (Tron, Matrix, Blade), without double slashes (//) in UI labels. Interactive buttons SHALL maintain zero expansion (`expansion = 0.0`) and fixed dimensions on hover, preventing layout shifts and footprint jitter. The main application window title SHALL be FastTail by Matteo Baccan.

#### Scenario: Button hover maintains stable footprint
- **WHEN** the user hovers over window controls, navigation buttons, or stream toggles
- **THEN** the button visuals highlight without expanding or altering surrounding layout geometry.

### Requirement: Virtualized Scroll Rendering
The docking log stream SHALL use virtualized row scrolling to render only visible lines on screen, guaranteeing 60+ FPS performance even with tens of millions of lines in the buffer. Row height SHALL scale proportionally with the selected font size without a restrictive minimum floor, maintaining compact and uniform interline spacing from 8pt to 32pt.

#### Scenario: Small font rendering remains compact
- **WHEN** the user sets font size below 16pt (e.g. 10pt or 8pt)
- **THEN** the line row height decreases proportionally with the text without extra blank gaps between lines.

## ADDED Requirements

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
