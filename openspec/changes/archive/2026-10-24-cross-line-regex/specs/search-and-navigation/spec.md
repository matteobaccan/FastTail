## ADDED Requirements

### Requirement: Search Modes
The stream search box SHALL offer three modes: Text (the default, matching plain text without regard to case, exactly as before), Regex (a regular expression matched against each visible line, case-sensitive only while the box's `Aa` toggle is on) and Multi-line regex. A pattern that does not compile SHALL tint the box and show the error, and the hits of the last valid pattern SHALL stay until it is fixed. The mode SHALL be saved per stream in the workspace and in session files as `search_mode=regex` or `search_mode=multiline`, written only when the mode is not Text, and the search history SHALL remember each query with its mode.

#### Scenario: Regex search
- **WHEN** the user picks Regex mode and searches `status=5\d\d`
- **THEN** the lines containing `status=500`, `status=503` and so on are hits and `status=404` is not.

#### Scenario: Text search unchanged
- **WHEN** a stream saved by FastTail 0.12.0 with the search `a.b` is restored
- **THEN** the search is in Text mode and matches only the literal text `a.b`.

### Requirement: Multi-Line Regex Search
In Multi-line regex mode the pattern SHALL be matched against the visible lines joined with a line feed, so that `\n` in the pattern crosses a line end and `^` and `$` match at the start and end of each line; lines hidden by the filters SHALL NOT be part of the text. A hit SHALL cover the rows from its first to its last byte and SHALL span at most 64 lines and 64 KB; longer matches SHALL NOT be reported and the search box SHALL say how many were skipped. The first row of a hit SHALL carry the match marker and tint; the rows the hit continues on SHALL carry a bracket in the marker column and a lighter tint. The match counter SHALL count hits, `F3` and `SHIFT + F3` SHALL move from hit to hit, the overview strip SHALL mark every row of a hit, and the search results pane SHALL list each hit's first line with the number of further lines it covers. The search SHALL run in the background above 16 MB with the same 1,000,000-hit cap as a text search, and lines appended to the file SHALL be searched so that a hit spanning the previous end of the file is found.

#### Scenario: Two lines in sequence
- **WHEN** the user searches `BEGIN TRANSACTION.*\n(.*\n){0,4}.*deadlock` in Multi-line regex mode
- **THEN** each place where a line containing `deadlock` follows a `BEGIN TRANSACTION` line within five lines is one hit, and `F3` goes to its first row.

#### Scenario: A hit across a filter gap
- **WHEN** the exclude term `DEBUG` hides the line between two visible lines `request /pay` and `request /refund`, and the user searches `request.*\nrequest` in Multi-line regex mode
- **THEN** the two visible rows form one hit.

#### Scenario: Too long to report
- **WHEN** a pattern with `(?s)` would match 500 lines
- **THEN** no hit is reported for it and the search box says one match was skipped for being longer than 64 lines.

#### Scenario: A hit completed by appended lines
- **WHEN** the file ends with a `BEGIN TRANSACTION` line and the writer appends a line containing `deadlock`
- **THEN** the counter grows by one hit spanning the old last line and the new line.
