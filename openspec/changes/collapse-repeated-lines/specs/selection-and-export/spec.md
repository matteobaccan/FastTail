## ADDED Requirements

### Requirement: Selection and Export of Collapsed Rows
While a stream's collapse mode is on, selecting a collapsed row SHALL select every line of its group, and CTRL + A SHALL select every visible line including the hidden ones. Copy (CTRL + C), "Export visible lines..." and the elapsed time of the selection SHALL use every underlying line in file order, never the `×N` badge, so the collapse mode never changes what is copied or exported. The row context menu SHALL also offer "Copy as shown", which copies one line per selected row, a collapsed row written as its first entry followed by ` ×N`.

#### Scenario: Copying a collapsed row
- **WHEN** a collapsed row stands for 3 lines `retry 1`, `retry 2` and `retry 3` in Numbers mode and the user selects it and presses CTRL + C
- **THEN** the clipboard holds the three lines separated by newlines.

#### Scenario: Copying as shown
- **WHEN** the same row is selected and the user picks "Copy as shown"
- **THEN** the clipboard holds the single line `retry 1 ×3`.

#### Scenario: Exporting a collapsed view
- **WHEN** mode is Exact, a filter leaves 10,000 visible lines shown as 800 rows and the user exports visible lines
- **THEN** the file contains the 10,000 visible lines in order.
