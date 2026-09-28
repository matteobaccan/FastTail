## ADDED Requirements

### Requirement: Patterns Tab
The stream menu SHALL offer "Patterns…", and `CTRL + SHIFT + G` SHALL do the same on the focused stream, opening a Patterns dock tab for that stream that is not saved in the dock layout. The tab SHALL list the patterns of the entries the stream shows under its include / exclude, level, time range and global filters, where an entry is a visible line that is not a stack-trace continuation line together with its visible continuation lines. For each pattern the tab SHALL show the number of entries, their share of the entries learnt, a sparkline of 64 buckets of its volume over the stream's time span (over line numbers when the stream has no usable timestamps), the first and last line where it occurs with their timestamps when known, and the template with its variable parts shown as `<*>`. The list SHALL be sortable by each column, sorted by count descending when opened, SHALL be narrowed by a text box to the templates containing its text, and SHALL be virtualized. The tab SHALL be unavailable in HEX and Markdown views.

#### Scenario: A log summarised by its patterns
- **WHEN** a stream shows 10,000 lines `payment <id> failed after <n> ms` with varying ids and durations, 500 lines `user <name> logged in` with 40 different user names, and 20 other distinct lines
- **THEN** the Patterns tab lists `payment <*> failed after <*> ms` with count 10,000 first and `user <*> logged in` with count 500 second.

#### Scenario: First and last seen
- **WHEN** a pattern occurs first on line 1,204 stamped 14:02:05 and last on line 88,410 stamped 16:30:12
- **THEN** its row shows line 1,204 at 14:02:05 as first seen and line 88,410 at 16:30:12 as last seen.

#### Scenario: Narrowing the list
- **WHEN** the user types `timeout` in the tab's text box
- **THEN** only the patterns whose template contains `timeout` are listed.

### Requirement: Pattern Learning
A line SHALL be assigned to a pattern from the first line of its entry, after removing the leading timestamp and trailing whitespace and masking UUIDs, `0x` hexadecimal numbers, hexadecimal words of 8 or more characters containing a digit, IPv4 and IPv6 addresses and runs of decimal digits, considering at most the first 4,096 bytes and 128 whitespace-separated tokens. Two entries SHALL share a pattern only when they have the same number of tokens and at least half of their tokens are equal; the tokens that differ SHALL become `<*>` in the template. A stream SHALL hold at most 2,000 patterns, and the entries that would create more SHALL be counted in one "other patterns" row. Learning SHALL run on a worker thread for a stream of any size, with its progress shown in the tab, while the interface stays responsive; appended lines SHALL be added to the patterns incrementally; a change of any filter other than the pattern filter SHALL relearn from the lines then visible, and a truncation, rotation or rewrite SHALL relearn from scratch. Patterns SHALL NOT be persisted.

#### Scenario: Numbers do not split patterns
- **WHEN** the lines `retry 1 of 5 for order 8812` and `retry 4 of 5 for order 9310` are learnt
- **THEN** both belong to the pattern `retry <*> of <*> for order <*>`.

#### Scenario: Different words split patterns
- **WHEN** the lines `cache hit for key a` and `connection refused by upstream b` are learnt
- **THEN** they belong to two different patterns.

#### Scenario: Large file
- **WHEN** the user opens the Patterns tab on a 3 GB stream
- **THEN** the interface stays responsive, the tab shows the learning progress and the patterns found so far, and the counts are final when the progress reaches 100%.

#### Scenario: Growing file
- **WHEN** the Patterns tab is open and the writer appends 50 lines of a known pattern and 1 line of a new one
- **THEN** within one second the known pattern's count grows by 50 and the new pattern is listed with count 1.

#### Scenario: Pattern cap
- **WHEN** a stream would produce more than 2,000 patterns
- **THEN** the tab lists 2,000 patterns and an "other patterns" row counting the remaining entries.

### Requirement: Pattern Filter
Clicking a pattern in the Patterns tab SHALL filter its stream to the entries of that pattern, `CTRL + click` SHALL add the pattern to or remove it from the selected patterns (an entry is shown when it belongs to any of them), and the row menu SHALL offer "Hide this pattern", which hides the entries of that pattern. The pattern filter SHALL combine with every other filter of the stream (a line must pass all of them), SHALL show exactly the entries counted for the selected patterns, continuation lines included, and SHALL NOT cause the patterns to be relearnt. While it is set, the stream bar SHALL show a chip naming the number of selected and hidden patterns, with the templates in its tooltip and a button that clears it. When the patterns are relearnt, the pattern filter SHALL be cleared and the chip tooltip SHALL say why. The pattern filter SHALL NOT be persisted.

#### Scenario: Filtering to one pattern
- **WHEN** the user clicks the pattern `payment <*> failed after <*> ms` listed with count 10,000
- **THEN** the stream shows exactly 10,000 entries, each with its stack-trace lines, and the stream bar shows the pattern chip.

#### Scenario: Hiding a noisy pattern
- **WHEN** the user chooses "Hide this pattern" on `healthcheck ok in <*> ms`
- **THEN** no entry of that pattern is visible, every other entry still is, and the Patterns tab keeps listing the same patterns.

#### Scenario: Clearing the filter
- **WHEN** the pattern filter is set and the user clicks the chip's clear button
- **THEN** the stream shows the lines it showed before the pattern filter was set.
