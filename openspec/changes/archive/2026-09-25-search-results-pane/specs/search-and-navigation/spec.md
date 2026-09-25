## ADDED Requirements

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

## MODIFIED Requirements

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
