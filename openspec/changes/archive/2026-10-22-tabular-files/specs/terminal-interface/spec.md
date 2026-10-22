## ADDED Requirements

### Requirement: Terminal Column View
A terminal stream window whose stream has an active field parser (JSON, logfmt, regex and its presets, CSV / TSV) SHALL offer a column view, off by default and toggled per stream from a stream bar chip and from the command palette. In the column view the window SHALL show a header row with the column names and one row per record, each shown field in its own column, followed by the message column, as the window's column view does. A columns dialog SHALL list every field known for the stream (at most 256) with a check box to show or hide it and keys to move it up or down; `←` `→` SHALL scroll the columns sideways. Widths SHALL fit the widest cell among the rows drawn, up to 40 cells, and be measured in terminal cells. A line that does not parse SHALL be drawn across the field columns, dimmed. The view state SHALL be read from and saved to the same `fields_view`, `fields_columns` and `fields_width.<field>` keys as the window, so a stream shows the same columns in both interfaces. Only the rows drawn SHALL be parsed.

#### Scenario: CSV with a header in the terminal
- **WHEN** the user opens `export.csv` whose first line is `level,proto,timestamp,message` and turns the column view on
- **THEN** the window shows a header row `level proto timestamp message` and each record's cells under it.

#### Scenario: Hiding columns
- **WHEN** the user opens the columns dialog and unticks `proto` and `timestamp`
- **THEN** the window shows only the `level` and `message` columns, and the GUI opened afterwards shows the same two columns for that stream.

#### Scenario: A line that does not parse
- **WHEN** a JSON stream holds a plain-text line between JSON lines
- **THEN** that line is drawn dimmed across the field columns and the other rows keep their columns.
