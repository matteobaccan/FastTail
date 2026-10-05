## ADDED Requirements

### Requirement: Tabular Files
The column view SHALL offer a delimited parser for CSV and TSV files, detected from the extension and from the first lines (the separator among comma, semicolon, tab and vertical bar that splits them into the same number of cells) or chosen in the field parser selector. A first row of distinct non-numeric names SHALL be used as the column names, which the user can turn off; otherwise the columns SHALL be numbered. Cells SHALL follow RFC 4180 quoting: a quoted cell may hold the separator and doubled quotes, and a quoted cell that spans several lines SHALL be shown as one row with a mark where each newline was, up to 1,000 lines per record. The file SHALL NOT be loaded into memory to do so. Column choice, order and widths, field filter terms, copy as shown and export SHALL work on the cells. The separator and the header choice SHALL be saved per stream; older builds ignore them.

#### Scenario: A CSV export with a header
- **WHEN** the user opens `orders.csv` whose first line is `id,customer,total` followed by data rows
- **THEN** the stream shows the columns `id`, `customer` and `total` with one row per record.

#### Scenario: A quoted newline
- **WHEN** a record's `note` cell is `"line one` on one line and `line two"` on the next
- **THEN** the record is one row whose `note` cell reads `line one⏎line two`.

#### Scenario: Semicolons and no header
- **WHEN** the user opens a file of numeric rows separated by `;`
- **THEN** the separator is detected and the columns are named `1`, `2`, `3`.
