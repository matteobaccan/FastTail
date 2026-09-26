## ADDED Requirements

### Requirement: Show In Context
While a stream in Text view has an active filter (include or exclude terms, minimum level, time range or the global filter), the row context menu SHALL offer "Show in context", and `CTRL + K` SHALL do the same on the selected row. It SHALL switch that stream to a context view showing every line of the file, unfiltered, with the chosen line centred, selected and marked, and follow mode paused; a banner above the rows SHALL say that the filters are suspended and offer "Back to filtered view". The banner button, `Esc` while the rows have the keyboard, and `CTRL + K` again SHALL return to the filtered view with the same top row, selection and follow state as before entering, and neither entering nor returning SHALL recompute the filter or the search: the filter settings SHALL stay unchanged and the filtered lines SHALL keep being maintained, appended lines included, while the context view is shown. Entering and returning SHALL complete within one frame on a file of any size once its line index is built. Any change of the stream's filter SHALL end the context view and apply the new filter with the chosen line centred when it is still visible; a reload, truncation or rotation, or switching to HEX or Markdown view, SHALL end it without restoring. In the context view, bookmarks, selection, copy, export and the overview strip SHALL work on the lines shown, and `F3` / `SHIFT + F3` SHALL walk the existing search hits. The action SHALL be unavailable in HEX and Markdown views, when the stream has no active filter, and while the line index is being built. The context view SHALL NOT be persisted.

#### Scenario: Reading around an error
- **WHEN** a stream filtered by the include term `ERROR` shows line 48,211 and the user picks "Show in context" on it
- **THEN** the stream shows every line of the file with line 48,211 centred, selected and marked, lines 48,200 to 48,210 are visible above it, and the banner offers "Back to filtered view".

#### Scenario: Back exactly where the user was
- **WHEN** in that context view the user scrolls 5,000 lines away, then presses `Esc`
- **THEN** the stream shows the `ERROR` lines again with the same top row and selection as before entering, and the include field still holds `ERROR`.

#### Scenario: Large file without recomputation
- **WHEN** a 4 GB stream filtered to 30,000,000 lines enters and leaves the context view
- **THEN** no background filter or search job is started and both transitions complete within one frame.

#### Scenario: Growing file
- **WHEN** a followed filtered stream enters the context view and 200 lines are appended, 3 of which match the filter
- **THEN** follow stays paused, the 200 lines appear at the end of the context view, and after returning the 3 matching lines are visible and follow is on again.

#### Scenario: Editing the filter ends the context view
- **WHEN** in the context view the user adds the exclude term `DEBUG`
- **THEN** the banner disappears and the stream shows the lines matching the new filter.

#### Scenario: Not offered without a filter
- **WHEN** a stream has no active filter, or is in HEX view
- **THEN** the row context menu has no "Show in context" item and `CTRL + K` does nothing.

### Requirement: Show In Context from Find Results
A result in the Find results tab SHALL offer "Show in context" in its context menu, and `CTRL + K` SHALL do the same on the selected result. It SHALL bring that stream's tab to the front and open its context view on the result's line, shown exactly even when the stream's current filter hides it, while the Find results list keeps the keyboard. It SHALL do nothing for a result of a stale group, and SHALL behave as a plain jump when the stream has no active filter.

#### Scenario: A hidden result in context
- **WHEN** the Find results list a match on line 88,120 of `payment.log`, whose filter now hides that line, and the user picks "Show in context"
- **THEN** the `payment.log` tab comes to the front in context view with line 88,120 centred and marked, and the banner offers "Back to filtered view".
