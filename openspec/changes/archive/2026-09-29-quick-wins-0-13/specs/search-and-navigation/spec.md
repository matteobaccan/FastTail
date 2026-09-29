## ADDED Requirements

### Requirement: Rule Navigation
The row context menu SHALL offer "Next line of rule" with the enabled highlight rules whose pattern matches that row, regardless of their priority; picking one SHALL make it the stream's navigation rule and go to the next shown line that the rule's pattern matches. `F4` SHALL go to the next and `SHIFT + F4` to the previous shown line matching the navigation rule, starting from the selected row or, without a selection, from the top row of the view, and wrapping around once at the end or the start with the same beep as the search. Without a navigation rule, `F4` SHALL use the first rule in priority order that matches the selected row, and when there is none the stream bar SHALL say so. The walk SHALL never block a frame for more than 4 ms: when the next line is not found within that time the stream bar SHALL show that the rule is being sought and the walk SHALL go on in the following frames until it finds a line, `Esc` is pressed, or the whole stream has been walked, in which case the stream bar SHALL say that no line matches. The jump SHALL select the line, centre it, pause follow mode and expand a collapsed group that hides it. The navigation rule SHALL be forgotten when the rules change and SHALL NOT be persisted.

#### Scenario: Next slow query
- **WHEN** the rule `duration_ms=\d{4,}` paints slow queries yellow, the user picks "Next line of rule" on a yellow row, then presses `F4`
- **THEN** the view goes to the next shown line that the rule matches, and `SHIFT + F4` goes back to the previous one.

#### Scenario: Seeking through a large file
- **WHEN** the navigation rule's next match is 3 GB further down a stream
- **THEN** the interface stays responsive, the stream bar says the rule is being sought, and the view goes to that line once it is found.

#### Scenario: No rule on the row
- **WHEN** no navigation rule is set and no rule matches the selected row
- **THEN** `F4` does not move the view and the stream bar says that no rule matches the row.
