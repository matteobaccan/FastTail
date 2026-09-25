# Selection and Export Specification

## Purpose
Defines row selection in the stream view, copying selected rows to the clipboard, and exporting the visible rows or the search matches to a text file.
## Requirements
### Requirement: Row Selection in the Stream View
Each stream SHALL keep its own set of selected rows. A click selects a row, Shift+click extends the selection over the visible rows between the anchor and the clicked row, Ctrl+click toggles one row, and Ctrl+A selects every visible row of the focused stream. Selected rows SHALL be tinted distinctly from search and highlight tints, and the selection SHALL be cleared when the file is truncated or reopened.

#### Scenario: Range selection under an active filter
- **WHEN** an include filter shows rows 10, 42 and 97 and the user clicks row 10 then Shift+clicks row 97
- **THEN** rows 10, 42 and 97 are selected and the hidden rows in between are not.

#### Scenario: Truncation clears the selection
- **WHEN** rows are selected and the writer truncates the file
- **THEN** the selection is empty and no stale index remains.

### Requirement: Copy Selected Rows to the Clipboard
Ctrl+C in the focused stream SHALL copy the selected rows as plain text in file order, one line per row, without line numbers or markers. With no selection it SHALL copy the row under the cursor of the current search match, if any.

#### Scenario: Copying three rows
- **WHEN** rows 3, 1 and 2 were selected in that order and the user presses Ctrl+C
- **THEN** the clipboard holds the text of rows 1, 2 and 3 separated by newlines.

### Requirement: Export Visible Lines and Search Matches
The stream menu SHALL offer "Export visible lines..." and "Export search matches...". Each SHALL open the native save dialog and write the corresponding rows as plain text, streaming to disk without holding the whole output in memory. The exported content SHALL be the line text regardless of the active view mode, without its ANSI escape sequences when the stream's ANSI mode is render or strip (see the ansi-escape-codes capability) and as stored otherwise. "Export search matches..." SHALL write the stored hits, that is at most the first 1,000,000 matching lines when the search counted more.

#### Scenario: Exporting a filtered stream
- **WHEN** an exclude filter hides half of a 2 million line file and the user exports visible lines
- **THEN** the file contains exactly the rows that pass the filter, in order.

#### Scenario: Exporting from HEX view
- **WHEN** the stream is in HEX view and the user exports search matches
- **THEN** the file contains the text of the matching lines, not a hex dump.

#### Scenario: Exporting a coloured stream
- **WHEN** a stream in ANSI render mode shows `ESC[31mERRORESC[0m payment failed` and the user exports visible lines
- **THEN** the exported line reads `ERROR payment failed` without any escape byte.

#### Scenario: Exporting more matches than are stored
- **WHEN** a search counts 3,412,009 matching lines and the user exports search matches
- **THEN** the file contains the first 1,000,000 matching lines, in order.

