## MODIFIED Requirements

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
