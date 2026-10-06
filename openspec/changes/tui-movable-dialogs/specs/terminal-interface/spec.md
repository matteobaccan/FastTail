## ADDED Requirements

### Requirement: Movable Terminal Settings and Keys Dialogs
The Settings dialog and the Keys (help) dialog SHALL show `[x]` right-aligned in their top border, before the corner, as the floating windows do; a click on it SHALL close the dialog as `Esc` does (Settings brings back the values it opened with). Dragging the title row (the top border except its two corner cells and `[x]`) SHALL move the dialog; dragging the left or right border, the bottom row or a corner SHALL resize it, with the same hit zones as a floating window. The dialog SHALL stay inside the screen and SHALL be at least 40 columns by 8 rows (Settings) or 30 columns by 6 rows (Keys), or the screen size when smaller. A double click on the title row SHALL bring back the centred default size. The content SHALL follow the dialog's size: Settings SHALL show as many fields as its rows allow and keep the focused field in view, and Keys SHALL lay out 1 to 3 columns (each at least 44 cells) for the dialog's inner width, scrolling when the entries do not fit. Each dialog SHALL remember its position and size while the interface runs, so that reopening it finds it where it was left, and SHALL be moved and shrunk back inside the screen when the terminal is resized. The geometry SHALL NOT be written to `fasttail.ini` or session files.

#### Scenario: Moving Settings off the stream
- **WHEN** Settings is open centred on an 120x40 terminal and the user drags its title row 30 columns to the right and 10 rows down
- **THEN** the dialog is drawn 30 columns to the right and 10 rows lower with the same size, the stream lines it uncovered are visible, and Settings keeps the keyboard.

#### Scenario: Resizing the Keys dialog
- **WHEN** the Keys dialog shows 3 columns on a 200x50 terminal and the user drags its right border left until the dialog is 90 columns wide
- **THEN** the entries are laid out in 2 columns of at least 44 cells, no text crosses the border, and the selected entry stays in view.

#### Scenario: Closing with [x]
- **WHEN** the user changes the theme in Settings and clicks `[x]`
- **THEN** the dialog closes, the theme Settings opened with is back, and `fasttail.ini` is not written.

#### Scenario: Position kept for the run
- **WHEN** the user moves the Keys dialog to the top-left corner, closes it with `Esc` and presses `?` again
- **THEN** the Keys dialog opens in the top-left corner at the size it had.

#### Scenario: Terminal shrinks
- **WHEN** Settings was moved to the bottom-right corner of a 160x50 terminal and the terminal is resized to 80x24
- **THEN** Settings is drawn entirely inside the 80x24 screen, at most 80 columns by 24 rows.

#### Scenario: Default geometry back
- **WHEN** the user double-clicks the title row of a moved and resized Settings dialog
- **THEN** the dialog is centred again at its default size.

## MODIFIED Requirements

### Requirement: Terminal Mouse
Mouse capture SHALL be on by default and off with `--no-mouse`. The wheel SHALL scroll the window under the pointer by 3 rows and pause its follow; a left click SHALL focus a window and put the cursor on the clicked row; `Shift`+click or a drag SHALL select a range; a double click SHALL toggle a bookmark; clicks on the stream strip and on a window's top border SHALL show that stream; clicks on dialog buttons, check boxes and list items SHALL act on them, and a click outside an open dialog SHALL close it like `Esc`. A drag that starts on a dialog's title row or border SHALL move or resize the dialog where the dialog allows it, and its release SHALL NOT close the dialog wherever the pointer ends. The help dialog SHALL say that `Shift`+drag (or `Option` on macOS terminals) selects text natively while the mouse is captured.

#### Scenario: Clicking the second window
- **WHEN** two windows are side by side and the user clicks the fifth visible row of the right one
- **THEN** the right window gets the focused border and its cursor is on that row.

#### Scenario: No mouse
- **WHEN** the user runs `fasttail-tui --no-mouse app.log` in conhost with QuickEdit on
- **THEN** clicks select console text natively and the application receives no mouse events.

#### Scenario: Drag released outside a dialog
- **WHEN** the user drags the Settings title row and releases the button with the pointer outside the dialog
- **THEN** Settings stays open at its new position and no stream receives the click.
