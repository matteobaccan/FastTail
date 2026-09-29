## ADDED Requirements

### Requirement: Compare Lines
With exactly two rows selected in a stream, the row context menu SHALL offer "Compare selected lines". The row context menu SHALL also offer "Mark for compare", which records the row under the menu as the window's compare mark, shown in its stream's gutter, and, while a line is marked, "Compare with marked line" on any row of any stream. Comparing SHALL open a Compare tab showing the two lines side by side, wrapped, with the differing words highlighted on each side, the text being the line without ANSI escape sequences as copy produces it. Each side SHALL name its stream and line number, and a double click on a side SHALL focus that line in its stream. "Copy as unified diff" SHALL copy the comparison as unified diff text. Compare tabs SHALL NOT be saved in the workspace or sessions.

#### Scenario: One field differs
- **WHEN** lines 120 and 480 differ only in `status=200` versus `status=503` and the user compares them
- **THEN** the Compare tab highlights `200` on the left and `503` on the right and nothing else.

#### Scenario: Across streams
- **WHEN** the user marks line 77 of `node-a.log` and picks "Compare with marked line" on line 81 of `node-b.log`
- **THEN** the Compare tab shows `node-a.log:77` on the left and `node-b.log:81` on the right.

### Requirement: Compare Regions
When the compare mark holds a selection of several lines, the row context menu of a stream with a selection SHALL offer "Compare selection with marked selection", comparing the two lists of lines in file order with a line diff that aligns unchanged lines and shows added, removed and changed lines, with word highlights inside changed lines. Each side SHALL be limited to 20,000 lines, a larger selection being refused with a message. The comparison SHALL run off the UI thread; if it exceeds 2 seconds, a coarser line-only result SHALL be shown with a notice. The tab SHALL show the number of changes, and `F7` / `SHIFT + F7` SHALL move to the next / previous change.

#### Scenario: Two runs of a job
- **WHEN** the user marks lines 1–300 of `run1.log`, selects lines 1–302 of `run2.log` and compares them
- **THEN** the tab aligns the common lines, shows the 2 extra lines of `run2.log` as added and F7 jumps to them.

### Requirement: Compare Ignore Options
The Compare tab SHALL offer toggles to ignore the leading timestamp (on by default), numbers, hexadecimal ids and UUIDs, differences in whitespace amount and letter case; ignored parts SHALL NOT count as differences but SHALL still be shown as written. When both compared lines contain a JSON object, a "Compare as JSON" toggle SHALL compare them pretty-printed with keys sorted.

#### Scenario: Timestamps ignored
- **WHEN** two lines are identical except for their leading timestamps and the timestamp option is on
- **THEN** the Compare tab reports no difference.

#### Scenario: JSON key order
- **WHEN** two lines hold `{"a":1,"b":2}` and `{"b":3,"a":1}` and "Compare as JSON" is on
- **THEN** only the row `"b": 2` versus `"b": 3` is shown as changed.
