## ADDED Requirements

### Requirement: Search Scope
Each stream's search SHALL have a scope, Whole view by default, shown as a chip in the search box. The user SHALL be able to set it to the lines from the first to the last selected line ("Search in selection"), to a typed line range (`first-last`, `first-` to the end of the file, or `-last`), to a time range typed with the formats and rules of the time range popup, or, from the row menu, from the clicked line to the end ("Search from here") or from the start to the clicked line ("Search up to here"). While a scope is set, only visible lines inside it SHALL be hits: the match marker, the tint, the counter (which SHALL say the hits are in range), `F3` / `SHIFT + F3` with their wrap-around, the search results pane and the histogram's search lane SHALL ignore hits outside it, and lines outside it SHALL stay visible. The overview strip SHALL shade the scope. A scope with an open end or a time scope SHALL include matching appended lines; a closed line range SHALL NOT. A time scope SHALL be held while the stream is timed in the background and SHALL be unavailable on a stream without usable timestamps. A truncation, rotation or reload SHALL return the scope to Whole view with a note in the search box. The chip's `✖` SHALL return to Whole view. The scope SHALL NOT be persisted.

#### Scenario: Searching inside a line range
- **WHEN** the user sets the scope to `1200000-1250000` and searches `timeout`, which occurs 40 times in the file and 3 times in that range
- **THEN** the counter reads `[1 / 3]` with the in-range note, `F3` from the third hit wraps to the first hit of the range, and the lines outside the range are still shown.

#### Scenario: Search in selection
- **WHEN** rows 500 to 620 are selected and the user picks "Search in selection" and searches `retry`
- **THEN** only the `retry` lines between lines 500 and 620 are hits.

#### Scenario: Search from here while following
- **WHEN** the user picks "Search from here" on line 90,000 of a growing log, searches `ERROR`, and the writer appends an ERROR line
- **THEN** the appended line is a hit and no ERROR line before line 90,000 is.

#### Scenario: Time scope
- **WHEN** the user sets the scope to the time range `14:00` to `14:10` and searches `503`
- **THEN** only lines stamped from 14:00:00.000 to 14:10:59.999 containing `503` are hits, a stack-trace line inheriting a timestamp in the range included.

### Requirement: Find Results Time Scope
The Find results tab SHALL offer optional from and to fields, accepting the formats of the time range popup, that limit the search of every stream to the lines whose timestamp lies in that range, each stream being timed first when needed. A stream without usable timestamps SHALL be reported as skipped for the time scope. Empty fields SHALL search the whole of each stream, as before.

#### Scenario: Ten minutes across all logs
- **WHEN** `gateway.log` and `payment.log` are open and the user searches all streams for `req-7f3a` with from `14:00` and to `14:10`
- **THEN** the results list only the lines of each stream stamped in that range.
