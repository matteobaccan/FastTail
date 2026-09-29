## ADDED Requirements

### Requirement: Selection and Export with Context Lines
While a stream shows context lines, context rows SHALL be selectable like any row, `CTRL + A` SHALL select every row shown, and copy (`CTRL + C`), "Copy as shown", "Export visible lines..." and the elapsed time of the selection SHALL use the lines shown, match and context lines alike, in file order. The separator between groups SHALL NOT be copied or exported.

#### Scenario: Exporting matches with context
- **WHEN** `N` is 1 and the filter keeps lines 10 and 50 of a file
- **THEN** "Export visible lines..." writes lines 9, 10, 11, 49, 50 and 51 in that order and nothing else.

#### Scenario: Copying across a separator
- **WHEN** the user selects the last row of one group and the first row of the next and presses `CTRL + C`
- **THEN** the clipboard holds the two lines separated by one newline.
