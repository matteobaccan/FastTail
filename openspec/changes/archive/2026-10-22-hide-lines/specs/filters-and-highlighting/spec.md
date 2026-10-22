## ADDED Requirements

### Requirement: Hidden Lines
The user SHALL be able to hide the selected rows of a stream, or the cursor row, from the stream's context menu, the command palette and `Ctrl+H` (`H` in the terminal interface). Hidden lines SHALL be kept as ranges of line numbers and SHALL be removed from the view like lines excluded by a filter: search, match counters, the overview strip, navigation, copy and export SHALL skip them, and Show in context SHALL show them with the other filtered lines. While any line is hidden the stream bar SHALL show the number of hidden lines; a click SHALL list the hidden ranges with the text of their first line, each with a Show action, and a Show all action. The ranges SHALL be saved per stream as `hidden=` in `fasttail.ini` and session files, which older builds ignore. A truncation, rotation or rewrite of the file SHALL clear them with a notice; appended lines SHALL keep them.

#### Scenario: Hide a burst of lines
- **WHEN** the user selects lines 120 to 180 and chooses Hide lines
- **THEN** those lines disappear from the view, the stream bar shows `61 hidden`, and a search no longer counts hits in them.

#### Scenario: Show them again
- **WHEN** the user opens the hidden list and chooses Show on the range 120-180
- **THEN** the lines are visible again and the chip disappears when no range is left.

#### Scenario: Restored with the stream
- **WHEN** lines are hidden, FastTail is closed and started again on the same unchanged file
- **THEN** the same lines are hidden.

#### Scenario: The file was rewritten
- **WHEN** a stream with hidden lines is truncated and rewritten
- **THEN** the hidden ranges are cleared and a notice says so.
