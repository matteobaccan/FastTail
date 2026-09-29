## ADDED Requirements

### Requirement: Navigation with Context Lines
While a stream shows context lines, the stream search SHALL cover every row shown, context rows included: the match marker, the query tint, the match counter, `F3` / `SHIFT + F3` and the search results pane SHALL include hits on context rows, and changing the number of context lines SHALL refresh the search (in the background above 16 MB) without refreshing the filter. Bookmarks, go to line and timeline jumps SHALL treat a context row as a visible row; a target line that is neither a match nor a context line SHALL behave as a line hidden by the filters. The overview strip and the scroll bar SHALL be proportional to the rows shown. "Show in context" SHALL show every line as it does without context lines, and returning SHALL restore the view with its context lines. When collapse is on, runs SHALL be formed over the rows shown, context rows included. The Find results tab SHALL keep searching the lines that pass the filters, without context lines.

#### Scenario: A hit on a context row
- **WHEN** `N` is 2, the filter is `ERROR`, and the search `retry` matches a context line just before an ERROR line
- **THEN** the context row shows the match marker and `F3` selects it.

#### Scenario: Going to a hidden line
- **WHEN** `N` is 2 and the user goes to a line 500 lines away from any match
- **THEN** the view goes to the next shown row after that line, as it does for a line hidden by the filters.

#### Scenario: Back from show in context
- **WHEN** `N` is 3 and the user enters and leaves "Show in context" on a match
- **THEN** the view shows the matches with three context lines each again, with the same top row as before.
