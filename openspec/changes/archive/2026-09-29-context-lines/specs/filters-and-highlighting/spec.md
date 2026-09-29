## ADDED Requirements

### Requirement: Context Lines Around Matches
The text view of a stream SHALL offer a context lines setting `N` from 0 to 100 (default 0), set from a `±N` control in the stream bar. While `N` is greater than 0 and the stream has an active filter (include or exclude terms, minimum level, time range or the global filter), the view SHALL show every line that passes the filters (a match) together with the `N` file lines before it and the `N` file lines after it, in file order, each line at most once. Context lines SHALL be taken from the file regardless of every filter, and a line that is itself a match SHALL be shown as a match. Context rows SHALL be drawn with the theme's dim text colour and their rule, label, ANSI and level colours at reduced opacity; match rows SHALL keep their normal style. Between two shown lines that are not consecutive in the file, a separator rule SHALL be drawn between their rows, not as a row of its own, with a tooltip giving the number of hidden lines. Changing `N` SHALL NOT recompute the filter, and scrolling SHALL NOT recompute anything. With `N` equal to 0, or without an active filter, the view SHALL behave exactly as without this feature. The setting SHALL NOT affect the HEX and rendered Markdown views. It SHALL be saved with the stream in the workspace and in session files as `context_lines=N`, written only when `N` is greater than 0.

#### Scenario: Three lines of context
- **WHEN** a stream filtered by the include term `payment failed` has matches on lines 1,000 and 5,000 and the user sets `N` to 3
- **THEN** the view shows lines 997 to 1,003 and 4,997 to 5,003, lines 1,000 and 5,000 in the normal style and the others dimmed, with a separator between lines 1,003 and 4,997 whose tooltip says 3,993 lines are hidden.

#### Scenario: Overlapping context
- **WHEN** `N` is 3 and the matches are on lines 100 and 104
- **THEN** lines 97 to 107 are shown once each with no separator between them, and line 104 is drawn as a match.

#### Scenario: Context ignores the filters
- **WHEN** `N` is 2, the level filter is `>= ERROR` and the two lines before an ERROR line are DEBUG lines
- **THEN** both DEBUG lines are shown, dimmed, above the ERROR line.

#### Scenario: Global filter matches get context
- **WHEN** the global include term is `req-7f3a`, a stream has no terms of its own and its `N` is 1
- **THEN** each line containing `req-7f3a` is shown with the line before and the line after it.

#### Scenario: Changing N does not refilter
- **WHEN** a 3 GB stream has finished filtering and the user changes `N` from 2 to 5
- **THEN** no background filter job is started and the new rows are shown within one frame of the rebuild.

#### Scenario: Restored with the workspace
- **WHEN** a stream has `N` set to 4 and FastTail is restarted
- **THEN** the stream reopens with `N` 4 and its workspace section holds `context_lines=4`, while a stream with `N` 0 has no `context_lines` key.

### Requirement: Context Lines on Large and Growing Files
On a stream larger than 16 MB, context SHALL appear together with the matches as the background filter scan delivers them, without a separate pass over the file. The memory kept per stream for context SHALL be at most 24 bytes per group of consecutive shown lines, and no memory per shown line SHALL be kept beyond the existing filtered line list. While the file grows, a line appended within `N` lines after the last match SHALL be shown as a context row without recomputing the filter, and a new match SHALL bring its own context; in follow mode the view SHALL stay on the last row. A truncation, rotation, rewrite or re-decode SHALL discard the context and build it again with the new filter result.

#### Scenario: Background filter with context
- **WHEN** `N` is 2 and the user types an include term on a 2 GB stream
- **THEN** the interface stays responsive, the stream bar shows the filter progress, and each match appears with its two lines before and after as the scan advances.

#### Scenario: Lines after the last match while following
- **WHEN** follow mode is on, `N` is 3, the last line is a match and the writer appends two lines that do not match
- **THEN** the two appended lines appear as dimmed context rows at the bottom and the view stays at the bottom.

#### Scenario: Truncation rebuilds the context
- **WHEN** the file is truncated and rewritten while `N` is 3
- **THEN** the view shows the matches of the new content with their context and no row of the old content.
