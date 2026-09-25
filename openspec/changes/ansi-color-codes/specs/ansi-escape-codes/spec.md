## ADDED Requirements

### Requirement: ANSI Escape Sequence Modes
Each text stream SHALL have an ANSI mode among auto, render, strip and raw, selectable from the stream toolbar and persisted per stream in the workspace and in sessions. In render mode, ANSI CSI and OSC escape sequences SHALL be hidden and SGR attributes (the 16 base and bright colours mapped to a palette of the active theme, 256-colour and 24-bit colours, bold, dim, italic, underline, inverse, and their resets) SHALL be painted as styled spans of the row. In strip mode the sequences SHALL be hidden and no ANSI style painted. In raw mode the line SHALL be shown as stored, with the escape character drawn as `␛`. Auto SHALL resolve to render once an SGR sequence is found in the first 64 KB of the file or, later, in appended data, and SHALL behave as raw until then. Escape sequences other than SGR SHALL be removed in render and strip modes without being interpreted. HEX view SHALL always show the file bytes.

#### Scenario: Coloured container log
- **WHEN** the user opens a `docker logs` capture whose lines contain `ESC[32mINFO ESC[0m` and `ESC[31mERRORESC[0m`
- **THEN** the stream resolves to render mode, INFO and ERROR are painted in the theme's green and red, and no `[32m` fragment is visible.

#### Scenario: Plain log is unaffected
- **WHEN** a log without any escape byte is opened
- **THEN** the mode stays auto (acting as raw) and rows, filters and search behave exactly as without this feature.

#### Scenario: Colours appear after the banner
- **WHEN** a stream in auto mode has shown 10,000 plain lines and the writer then appends coloured lines
- **THEN** the stream switches once to render mode, announces the switch in the stream bar, and restarts its filter and search scans.

#### Scenario: Raw mode for debugging the codes
- **WHEN** the user selects raw mode on a coloured stream
- **THEN** rows show `␛[31mERROR␛[0m` and an include regex `\x1b\[31m` matches those rows.

### Requirement: Text Features Operate on the Stripped Line
In render and strip modes, the text of a line for every text feature SHALL be the decoded line with its escape sequences removed: include and exclude filters (in the synchronous path and in background scans alike), search and its match navigation, highlight rules and quick labels, level detection, timestamp detection, JSON detection, copy to the clipboard, export, and the line placeholder of external tools. Offsets inside a line produced by these features SHALL be offsets in the stripped text; where a file byte offset is needed (moving the search cursor to HEX view) the stripped offset SHALL be converted through the positions of the removed sequences. Changing the mode SHALL re-evaluate filters, search, levels and timestamps without rebuilding the line index.

#### Scenario: Level detected behind a colour code
- **WHEN** a line reads `ESC[31mERRORESC[0m payment failed` in render mode
- **THEN** its level is ERROR, the `>= WARN` level filter keeps it, and the include filter `\bERROR\b` matches it.

#### Scenario: Copy without control bytes
- **WHEN** the user selects two coloured rows in render mode and presses Ctrl+C
- **THEN** the clipboard holds their text without any escape byte.

#### Scenario: Same result in background scans
- **WHEN** the include filter `ERROR payment` is applied to a coloured 400 MB stream in render mode, scanned on a worker thread
- **THEN** the visible lines are the same as those of the synchronous path on the same content.

#### Scenario: Search cursor carried to HEX view
- **WHEN** the current search match is the word `payment` after a colour code in render mode and the user switches to HEX view
- **THEN** the HEX view scrolls to the file bytes of `payment`, not to a position shifted by the length of the removed escape sequence.

### Requirement: ANSI Colour Precedence
ANSI colour spans SHALL rank below user highlight rules and quick labels and above the level-colouring fallback: a whole-row user rule SHALL style the whole row, capture-only rules and quick labels SHALL win on the bytes they cover, ANSI colours SHALL apply to the remaining bytes, and level colouring SHALL apply only to the uncoloured bytes of rows that no user rule matches.

#### Scenario: User rule over an ANSI colour
- **WHEN** a user rule colours lines containing `payment` green and a line in render mode contains `payment` and a red ANSI `ERROR`
- **THEN** the row is green.

#### Scenario: Quick label inside a coloured row
- **WHEN** a quick label marks `sess-8f3a` and a coloured row contains it
- **THEN** `sess-8f3a` carries the label colour and the rest of the row keeps its ANSI colours.
