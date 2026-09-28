# Search and Navigation Specification

## Purpose
Defines in-stream text search: per-stream queries, match navigation scoped to the focused window, live refresh of matches, and consistent match marking across the Text, Hex and Markdown views.
## Requirements
### Requirement: Per-Stream Search Query
Each open stream SHALL own its search query, match list and current match position. Typing in the search box of one stream SHALL NOT change the query or the matches of any other stream.

#### Scenario: Two streams searched independently
- **WHEN** the user searches `ERROR` in stream A and `timeout` in stream B
- **THEN** stream A keeps `ERROR` with its own matches and stream B keeps `timeout` with its own matches, even when both are visible side by side.

### Requirement: Match Navigation Scoped to the Focused Window
`F3` / `Shift+F3`, `Ctrl+F`, `Enter` / `Shift+Enter` in the search box and the keyboard navigation shortcuts SHALL act only on the stream shown in the focused dock panel. When no panel has been focused yet, the first stream of the main surface is treated as focused. A stream's search results pane is part of that stream's panel: clicking in it SHALL focus the panel and give the pane keyboard focus. While the pane has keyboard focus, the arrow keys, `PgUp` / `PgDown` and `Ctrl+Home` / `Ctrl+End` SHALL move the pane selection, `Enter` SHALL commit it and `Esc` SHALL return keyboard focus to the main view; `F3` / `Shift+F3`, `Ctrl+F`, `Ctrl+G`, the bookmark keys and the other stream shortcuts SHALL keep acting on the stream.

#### Scenario: F3 with two visible streams
- **WHEN** two streams with active searches are visible side by side and the user presses `F3`
- **THEN** only the stream in the focused panel advances to its next match; the other stream's current match does not move.

#### Scenario: F3 while the pane has focus
- **WHEN** the user has clicked a row in the results pane of stream A and presses `F3`
- **THEN** stream A advances to its next match in the main view and the pane selection moves with it; no other stream reacts.

#### Scenario: Leaving the pane
- **WHEN** the pane has keyboard focus and the user presses `Esc`, then `PgDown`
- **THEN** the main view scrolls by one page and the pane selection does not move.

### Requirement: Match Counter, Wrap-Around and History
The stream bar SHALL show the current match position and total (`[current / total]`), navigation SHALL wrap around at either end emitting a beep when sound effects are enabled, and the last 10 distinct queries SHALL be kept in a history dropdown persisted in the configuration file. Rescans while typing SHALL be debounced so that each keystroke does not block the interface. A stream SHALL store at most 1,000,000 line hits; past that the search SHALL keep counting without storing, the total SHALL be the true number of matching lines, the counter and the results pane SHALL say that only the first 1,000,000 are listed, and navigation SHALL wrap within the stored hits. Byte-level hits in HEX view SHALL be capped at 20,000.

#### Scenario: Wrapping past the last match
- **WHEN** the counter shows `[7 / 7]` and the user presses `F3`
- **THEN** the current match becomes `[1 / 7]`, a beep is played if sound effects are enabled, and the query is stored at the top of the search history.

#### Scenario: More matches than the cap
- **WHEN** a search for `INFO` matches 3,412,009 lines of a large log
- **THEN** the counter's total reads 3,412,009 with a note that the first 1,000,000 are listed, and `F3` from the 1,000,000th hit wraps to the first.

### Requirement: Live Refresh of Matches
Matches SHALL be recomputed when the file grows, is truncated or rewritten, and when the include/exclude filters change. Only lines that can be displayed under the active filters are searchable. The current match SHALL stay on the same line whenever that line still matches; otherwise it moves to the nearest following match.

#### Scenario: New matching lines appended
- **WHEN** a search for `ERROR` is active and the writer appends two more `ERROR` lines
- **THEN** the counter total grows by two and `F3` reaches the new lines, without the user editing the query.

### Requirement: Match Marker Column and Row Highlight in Every View
While a query is active, every view SHALL show a fixed-width marker column at the left of each row: `▶` on the row of the current match, `●` on rows of other matches, blank elsewhere. Matching rows SHALL be tinted across their full width, with a stronger tint on the current match. The column width SHALL not change when the current match moves.

#### Scenario: Moving the current match
- **WHEN** three rows match and the user presses `F3`
- **THEN** the `▶` marker and the stronger tint move to the next matching row, the previous row keeps `●` and the normal tint, and no row shifts horizontally.

### Requirement: Byte-Level Search in HEX View
In HEX view the query SHALL be matched against the file bytes: as ASCII text ignoring case, and additionally as a byte pattern when the query is an even-length string of hex digits (spaces and colons ignored, e.g. `0A 0D`). A hit spanning two hex rows SHALL mark both rows. `F3` / `Shift+F3` SHALL navigate between byte offsets and the counter SHALL count byte-level hits.

#### Scenario: Text spanning a row boundary
- **WHEN** the word `needle` starts at byte 15 of a stream shown with 16 bytes per row and the user searches `needle`
- **THEN** rows `00000000` and `00000010` are both marked and the counter shows one hit.

### Requirement: Search in Markdown View
In Markdown view an active query SHALL switch the stream to its source lines, marked and highlighted exactly as in Text view, with a notice that the source is being shown. Clearing the query SHALL restore the rendered Markdown document.

#### Scenario: Searching inside a rendered README
- **WHEN** a stream in MD view is showing a rendered README and the user types `install` in the search box
- **THEN** the stream shows the Markdown source lines with matches marked and a notice that the source is displayed; clearing the box returns to the rendered document.

### Requirement: Search Cursor Preserved Across Views
Switching a stream between Text, Hex and Markdown views SHALL keep the query and keep the current match position within the range of the list navigated by the new view.

#### Scenario: Switching from Text to Hex with an active search
- **WHEN** a Text view search shows `[5 / 12]` and the user switches the stream to HEX view
- **THEN** the query stays in the search box, byte-level hits are computed, and the current match index is clamped to the new hit count.

### Requirement: Go To Line
Ctrl+G SHALL open a go-to popup for the focused stream. Entering a 1-based line number and pressing Enter SHALL scroll that line to the middle of the viewport and pause follow mode. Numbers past the end SHALL clamp to the last line. Under active filters the first visible line at or after the target SHALL be used and the popup SHALL say so. The forms `+N` and `-N` SHALL jump relative to the current top line. An input containing `:` SHALL be read as a time instead, in the forms the timestamp range filter accepts (a bare `HH:MM[:SS]` belonging to the day of the stream's first timestamped line, a `YYYY-MM-DD HH:MM[:SS]`, or a timestamp copied out of a line), and SHALL jump to the first line whose timestamp is at or after it. When the stream has not been fully timed yet, the timestamp cache SHALL be built first, in the background when more than 16 MB remain to be timed: the popup SHALL show the timing progress and the jump SHALL happen when the cache is complete. Closing the popup or entering another target SHALL cancel a jump that is still waiting, without stopping the timing. The search SHALL be a binary search when the timestamps never go back in time and a linear scan otherwise. Anything else, including a bare epoch number, is a line number.

#### Scenario: Jumping to a hidden line
- **WHEN** an include filter hides line 500 and the user goes to line 500
- **THEN** the viewport centres on the first visible line after 500 and the popup reports the substitution.

#### Scenario: Number beyond the file
- **WHEN** the file has 1,000 lines and the user enters 5000
- **THEN** the viewport shows line 1,000 and follow mode is paused.

#### Scenario: Jump to a time on a freshly opened log
- **WHEN** the user opens an ordered log, presses Ctrl+G without touching the time range and enters `14:03:30`
- **THEN** the viewport centres on the first line stamped at or after 14:03:30 on the day of the log's first entry.

#### Scenario: Time past the end of the log
- **WHEN** the user enters a time later than every timestamp in the stream
- **THEN** the popup reports that the input cannot be resolved and the viewport does not move.

#### Scenario: Jump to a time on a large untimed log
- **WHEN** the user enters `09:15` in the Ctrl+G popup of a 4 GB stream that has never been timed
- **THEN** the interface stays responsive, the popup shows the timing progress, and when timing completes the viewport centres on the first line stamped at or after 09:15.

#### Scenario: Waiting jump cancelled
- **WHEN** a time jump is waiting for timing and the user presses Esc
- **THEN** the popup closes, the viewport does not move when timing completes, and the timing itself continues.

### Requirement: Line Bookmarks
Each stream SHALL keep a set of manual bookmarked line indices and a set of automatic bookmarks (see Automatic Bookmarks from Rules). `CTRL + F2` SHALL toggle a manual bookmark on the current row; on a row that carries only an automatic bookmark it SHALL dismiss that automatic bookmark instead. `F2` / `SHIFT + F2` SHALL jump to the next / previous bookmark, manual or automatic, visible under the active filters with wrap-around, and the stream menu SHALL offer "Clear bookmarks", which removes every manual bookmark with its note and dismisses every current automatic bookmark. Manual bookmarked rows SHALL show `★` in the marker column (`✏` when the bookmark has a note) and automatic-only rows `☆`, unless the row is a search match, and every bookmarked row SHALL be tinted across its width. Bookmarks, notes, automatic bookmarks and dismissals SHALL be dropped when the file is truncated or rewritten.

#### Scenario: Navigating bookmarks with a filter active
- **WHEN** rows 5, 60 and 900 are bookmarked, an include filter hides row 60, and the user presses `F2` from row 5
- **THEN** the view jumps to row 900, and pressing `F2` again wraps to row 5.

#### Scenario: Bookmark on a search match
- **WHEN** a bookmarked row is also the current search match
- **THEN** the marker column shows `▶` and the row keeps the bookmark tint.

#### Scenario: Dismissing an automatic bookmark
- **WHEN** row 42 carries only an automatic bookmark and the user presses `CTRL + F2` on it
- **THEN** row 42 shows no marker, `F2` skips it, and it is not bookmarked again until the file is reloaded or the rules change.

#### Scenario: Truncated file
- **WHEN** a stream has manual bookmarks with notes and automatic bookmarks and the file is truncated to 0 bytes
- **THEN** every bookmark, note and dismissal is dropped, and automatic bookmarks are recomputed on the lines written afterwards.

### Requirement: Bookmark Persistence per File
Manual bookmarks and their notes SHALL be saved in `fasttail.ini` keyed by absolute file path (the entry path `<archive>/<entry>` for a zip entry), at most 1,000 bookmarks per file and 50 files, and restored when the same path is reopened, provided the file still has at least as many lines as the largest saved index; a restored compressed stream applies them, with their notes, once its index covers them. In the `[bookmarks]` section the lines SHALL stay in `lines_<i>` and each note SHALL be stored as `note_<i>_<line>`; in session files each stream section SHALL keep `bookmarks` and store each note as `bookmark_note.<line>`. A note whose line is not among the saved bookmarks SHALL be ignored on load. Files without note keys SHALL load as before. Automatic bookmarks and dismissals SHALL NOT be saved and SHALL NOT count toward the 1,000 per file. Nothing of the standard-input stream SHALL be saved.

#### Scenario: Reopening a file
- **WHEN** the user bookmarks rows 10 and 200 in `app.log`, gives row 200 the note `first OOM`, closes FastTail and opens `app.log` again
- **THEN** rows 10 and 200 are bookmarked and row 200 shows the note `first OOM`.

#### Scenario: File rewritten smaller
- **WHEN** saved bookmarks reference row 200 and the reopened file has 50 lines
- **THEN** the saved bookmarks and notes for that file are discarded.

#### Scenario: Settings from an older version
- **WHEN** `fasttail.ini` has `lines_0=10,200` and no `note_0_*` keys
- **THEN** rows 10 and 200 are restored as bookmarks without notes.

#### Scenario: Notes in a session file
- **WHEN** the user saves a session whose stream has a bookmark on row 7 with the note `deploy start` and later loads that session
- **THEN** the stream section holds `bookmarks=7` and `bookmark_note.7=deploy start`, and row 7 is bookmarked with that note after loading.

### Requirement: Search Results Pane
A stream with an active query SHALL be able to show a results pane below its rows, toggled from the stream bar, listing only the matching lines in file order with their 1-based line number and text, the query tinted and the level colour applied, under a header naming the query, the total, the capped note and the progress of a running search. The pane SHALL be virtualized: only the rows on screen are read, whatever the number of hits, and hits found by a running search or on appended lines SHALL appear as they are found. Clicking a row, or selecting it with the arrow keys and pressing `Enter`, SHALL make that hit the current match, centre it in the main view and pause follow mode. The current match SHALL be marked `▶` in the pane, and the pane SHALL scroll to keep it visible when it changes by `F3`, `Shift+F3` or a refresh. The pane's open state and height SHALL be one preference shared by every stream (the pane shows in each stream with an active query), persisted in `fasttail.ini` as `search_pane` and `search_pane_height`. In HEX view the pane SHALL show a notice instead of rows.

#### Scenario: Jumping from the pane
- **WHEN** a search for `timeout` on a 2 GB log lists 340 hits in the pane and the user clicks the hit on line 1,204,881
- **THEN** the main view centres line 1,204,881, follow mode is paused, the counter shows that hit's position, and the pane row carries `▶`.

#### Scenario: Pane follows F3
- **WHEN** the pane is open and the user presses `F3` three times
- **THEN** the current match advances three hits in the main view, and the pane scrolls so that the `▶` row stays visible.

#### Scenario: Keyboard selection without moving the view
- **WHEN** the pane has keyboard focus and the user presses `↓` five times
- **THEN** the pane selection moves five rows while the main view stays where it was, and pressing `Enter` then centres the selected hit in the main view.

### Requirement: Overview Strip
The Text view of a stream SHALL show, beside its vertical scrollbar, an overview strip marking the proportional position among the visible rows of the search hits, the bookmarks, the ERROR and FATAL lines and the current match, with the viewport drawn as a box. Clicking or dragging on the strip SHALL scroll the main view to that position and pause follow mode. Hovering the strip SHALL show the line number under the pointer. Hit marks SHALL be exact for the stored hits (the first 1,000,000) and bookmark marks SHALL be exact. Error marks cover the lines whose level has already been detected; they SHALL be exact without a filter and on filtered views of up to 4,000,000 visible rows; above that they MAY be sampled, and the strip's tooltip SHALL say so. The strip SHALL be recomputed only when the rows, the hits, the bookmarks, the level cache or the filters change, not on every frame; while the stream grows it SHALL be rebuilt at most four times a second, and a change of its height or of the bookmarks SHALL rebuild it at once. The strip SHALL be hidden in HEX and rendered Markdown views and when there is nothing to mark, and SHALL be switchable off in Settings (on by default).

#### Scenario: Seeing where errors cluster
- **WHEN** a 1 GB log without filters has ERROR lines only around its middle and near its end
- **THEN** the strip shows error marks at those two heights, and clicking the lower cluster scrolls the main view there.

#### Scenario: Large filtered view
- **WHEN** an exclude filter leaves 30 million visible rows and the user hovers the strip
- **THEN** search hit and bookmark marks are exact, and the tooltip says that error marks are sampled.

### Requirement: Search All Streams
The user SHALL be able to run one query across every open stream. The match SHALL be the same case-insensitive text match as the per-stream search, over the lines each stream shows under its own include/exclude, level and time filters; streams in HEX view SHALL be skipped and reported as skipped. Each stream SHALL be searched by its own background job, independent of the stream's own filter and search scans, with at most four jobs (fewer on machines with fewer cores) running at once and the others queued. A new query, a Stop button and closing the results SHALL cancel every job; closing a stream SHALL cancel its job and remove its results. At most 100,000 hits SHALL be stored per stream, with the true total still counted. The results SHALL be a snapshot of each stream at the moment its job started, with a Refresh action to run the query again.

#### Scenario: A request id across three logs
- **WHEN** `gateway.log`, `payment.log` and `audit.log` are open and the user searches all streams for `req-7f3a`
- **THEN** each stream is searched in the background, and the results report 4 matches in `gateway.log`, 2 in `payment.log` and none in `audit.log`.

#### Scenario: Filters of each stream are honoured
- **WHEN** `payment.log` has an exclude filter `healthcheck` and one line contains both `req-7f3a` and `healthcheck`
- **THEN** that line is not among the results, as it would not be for the stream's own search.

#### Scenario: Bounded concurrency
- **WHEN** ten multi-GB streams are open and the user runs a query on an eight-core machine
- **THEN** at most four searches run at the same time, the others show as queued, and the interface stays responsive.

#### Scenario: Cancelling
- **WHEN** a search across streams is running and the user presses Stop
- **THEN** every running and queued job stops, and the results found so far stay listed.

### Requirement: Find Results Tab
The results of a search across streams SHALL be shown in a single "Find results" dock tab, opened or focused by `Ctrl+Shift+F` with the focused stream's query prefilled, and not saved in the dock layout. Results SHALL be grouped by stream, each group headed by the stream name, its match count, its progress while running and a note when only the first 100,000 hits are listed; groups SHALL be collapsible, and the whole list SHALL be virtualized. Pressing `Enter` in the query box SHALL run the search. Clicking a result, selecting it with the keyboard and pressing `Enter`, or reaching it with the arrow, `Page Up` / `Page Down` or `Home` / `End` keys SHALL bring that stream's tab to the front of its panel, centre the line (or the next visible line when the stream's filters now hide it) and pause follow mode, without changing the stream's own search query; the Find results list SHALL keep the keyboard and the dock focus. When a stream has been reloaded, truncated, rewritten or switched to another file since its results were found, its group SHALL be marked stale and its results SHALL NOT jump.

#### Scenario: Jumping to a result
- **WHEN** the Find results tab lists a match on line 88,120 of `payment.log`, which is a background tab, and the user clicks it
- **THEN** the `payment.log` tab comes to the front, line 88,120 is centred, follow is paused, the search box of `payment.log` keeps its previous query, and the Find results list keeps the keyboard.

#### Scenario: Walking the results with the keys
- **WHEN** after that click the user presses `↓`, then `End`
- **THEN** each key selects the next result, then the last one, and the stream holding it comes to the front with the line centred, while the keyboard stays on the list.

#### Scenario: Stream rewritten after the search
- **WHEN** `gateway.log` is truncated by its writer after the search and the user clicks one of its results
- **THEN** the view does not move and the group says the stream changed and offers Refresh.

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

### Requirement: Bookmark Notes
A manual bookmark SHALL be able to carry one note: a single line of at most 200 characters, trimmed, with line breaks and tabs replaced by spaces. The row context menu of the Text view SHALL offer "Bookmark note…" on every row, opening a one-line editor (`Enter` saves, `ESC` cancels), and "Remove bookmark" on a bookmarked row. Saving a note on a row without a manual bookmark SHALL add one (an automatic bookmark on that row becomes manual); saving an empty note SHALL remove the note and keep the bookmark; removing the bookmark SHALL remove its note. Hovering the row's marker, or a bookmark mark in the overview strip, SHALL show the note in a tooltip. Adding, editing or removing a note SHALL be saved as a bookmark change.

#### Scenario: Adding a note
- **WHEN** the user right-clicks row 1,204, picks "Bookmark note…", types `retry storm starts here` and presses `Enter`
- **THEN** row 1,204 is bookmarked, its marker shows `✏`, and hovering the marker shows `retry storm starts here`.

#### Scenario: Note too long
- **WHEN** the user pastes 350 characters into the note editor
- **THEN** only the first 200 characters are kept.

#### Scenario: Note on an automatic bookmark
- **WHEN** row 42 carries only an automatic bookmark and the user saves the note `check this`
- **THEN** row 42 becomes a manual bookmark with that note, is saved in `fasttail.ini`, and counts toward the 1,000 per file.

### Requirement: Automatic Bookmarks from Rules
Every line matching an enabled highlight rule with "Bookmark matching lines" on SHALL carry an automatic bookmark, whatever the stream's filters: the lines already in the file when it is opened, reloaded or when the rules change, and every line appended afterwards, checked as it arrives. At most `auto_bookmark_max` automatic bookmarks SHALL be kept per stream (a setting in `fasttail.ini` and in Settings, default 10,000, clamped to 100..100,000), the first ones in file order; once the cap is reached no more are added and the stream bar SHALL show that automatic bookmarks are capped at that number. Notes SHALL NOT be written by copy or export. For files above 16 MB the existing lines SHALL be matched on a worker thread with progress in the stream bar, the UI staying responsive, and lines appended during that scan SHALL be matched exactly once. Automatic bookmarks SHALL appear in the overview strip with a dimmer mark than manual bookmarks. Turning the option off or disabling the rule SHALL remove the automatic bookmarks it produced at the next recomputation, and a rules change SHALL clear the dismissals. Standard-input and compressed streams SHALL be covered, compressed streams once their index is complete.

#### Scenario: Existing and appended lines
- **WHEN** a rule `OutOfMemoryError` has "Bookmark matching lines" on, the opened file has 3 matching lines, and 2 more matching lines are appended
- **THEN** all 5 lines show `☆`, and `F2` visits each of them in order.

#### Scenario: Large file
- **WHEN** a 2 GB log is opened with an auto-bookmark rule
- **THEN** the matching runs in the background with progress in the stream bar, rows stay scrollable during it, and the automatic bookmarks appear as the scan reports them.

#### Scenario: Cap reached
- **WHEN** an auto-bookmark rule `INFO` matches 250,000 lines of a file
- **THEN** with the default setting the first 10,000 matching lines are bookmarked, the stream bar shows the capped notice, and further appended `INFO` lines are not bookmarked.

#### Scenario: A larger cap
- **WHEN** the user sets `auto_bookmark_max` to 50,000 in Settings while that stream is open
- **THEN** the stream recomputes its automatic bookmarks and the first 50,000 matching lines are bookmarked.

#### Scenario: Not saved
- **WHEN** a file has 12 automatic bookmarks and 3 manual bookmarks and FastTail is restarted
- **THEN** `fasttail.ini` holds only the 3 manual bookmarks, and the 12 automatic ones are recomputed from the rule when the file is reopened.

### Requirement: Navigation in Collapsed Groups
While a stream's collapse mode is on, search hits and bookmarks SHALL keep referring to file lines and the match counter SHALL count every hit, hidden or not. A collapsed row SHALL show the match marker when any line it stands for is a hit and the bookmark marker when any of them is bookmarked. Match navigation (F3 and SHIFT + F3) SHALL land on the group row for the first hit inside a collapsed group and the next step SHALL move past the other hits of that group. Showing one exact line that is hidden in a collapsed group, from the search results pane, the Find results tab, go to line, bookmark navigation or a timeline click, SHALL expand that group and select the line. Toggling a bookmark on a collapsed row SHALL apply to its first line. The overview strip and the scrollbar SHALL be proportional to the collapsed rows, and marks of hidden lines SHALL be drawn at their group row. Line numbers SHALL show the number of each row's first line.

#### Scenario: Stepping over a collapsed group
- **WHEN** a collapsed `×200` group holds 200 hits of the search `timeout`, followed by one more hit on a later line, and the user presses F3 twice from the top
- **THEN** the first press selects the group row, the second selects the later line, and the counter shows 201 hits.

#### Scenario: Go to a hidden line
- **WHEN** lines 1,000 to 1,499 form a collapsed group and the user goes to line 1,250
- **THEN** the group is expanded and line 1,250 is selected and in view.

#### Scenario: Bookmark inside a group
- **WHEN** line 1,300 is bookmarked and lies inside a collapsed group
- **THEN** the group row shows the bookmark marker and the overview strip marks the group's position.

