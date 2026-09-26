## MODIFIED Requirements

### Requirement: Keyboard Navigation & Hotkeys
The log stream viewport SHALL support comprehensive keyboard navigation. Stream-level shortcuts act on the stream in the focused dock panel only (see the Search and Navigation specification):
- Home / End: Scroll horizontally to the far left (Home) or far right (End).
- Arrow keys: Scroll by one line or column; Ctrl + Left / Right scrolls horizontally 5x faster.
- PgUp / PgDown: Scroll up or down by one viewport page.
- Ctrl + Home: Jump directly to line 0 (top of buffer) and pause follow mode.
- Ctrl + End: Jump directly to the latest line (bottom of buffer) and enable follow mode.
- Ctrl + F: Focus the search input box of the focused stream.
- Ctrl + Shift + F: Open or focus the Find results tab to search every open stream, prefilled with the focused stream's query (see the Search and Navigation specification). It SHALL NOT also trigger Ctrl + F.
- F3 / Shift + F3: Jump to next / previous search match of the focused stream.
- Ctrl + G: Open the go-to popup of the focused stream (line number, `+N` / `-N`, or a time such as `14:02`; see the Search and Navigation specification).
- Alt + 1..9: Activate stream tab #1 through #9.
- Space: Toggle follow mode.
- Esc: Close the active dialog, or leave the search box.
- Ctrl + / Ctrl =: Increase the interface zoom (see Interface Zoom).
- Ctrl -: Decrease the interface zoom.
- Ctrl 0: Reset the interface zoom to 100%.
- Ctrl + MouseWheel: Adjust the interface zoom.
- F1: Open the Keyboard Shortcuts & Help modal.

#### Scenario: Shortcuts act on the focused stream only
- **WHEN** two streams are docked side by side and the user presses Ctrl + End
- **THEN** only the stream in the focused panel jumps to its last line and enables follow mode; the other panel keeps its scroll position.

#### Scenario: Searching every stream from the keyboard
- **WHEN** the focused stream's search box holds `req-7f3a` and the user presses Ctrl + Shift + F
- **THEN** the Find results tab opens with `req-7f3a` in its query box, and the focused stream's search box does not take keyboard focus.
