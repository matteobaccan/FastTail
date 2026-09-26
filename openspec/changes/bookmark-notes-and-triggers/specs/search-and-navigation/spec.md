## MODIFIED Requirements

### Requirement: Line Bookmarks
Each stream SHALL keep a set of manual bookmarked line indices and a set of automatic bookmarks (see Automatic Bookmarks from Rules). `CTRL + F2` SHALL toggle a manual bookmark on the current row; on a row that carries only an automatic bookmark it SHALL dismiss that automatic bookmark instead. `F2` / `SHIFT + F2` SHALL jump to the next / previous bookmark, manual or automatic, visible under the active filters with wrap-around, and the stream menu SHALL offer "Clear bookmarks", which removes every manual bookmark with its note and dismisses every current automatic bookmark. Manual bookmarked rows SHALL show `★` in the marker column (`✎` when the bookmark has a note) and automatic-only rows `☆`, unless the row is a search match, and every bookmarked row SHALL be tinted across its width. Bookmarks, notes, automatic bookmarks and dismissals SHALL be dropped when the file is truncated or rewritten.

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

## ADDED Requirements

### Requirement: Bookmark Notes
A manual bookmark SHALL be able to carry one note: a single line of at most 200 characters, trimmed, with line breaks and tabs replaced by spaces. The row context menu of the Text view SHALL offer "Bookmark note…" on every row, opening a one-line editor (`Enter` saves, `ESC` cancels), and "Remove bookmark" on a bookmarked row. Saving a note on a row without a manual bookmark SHALL add one (an automatic bookmark on that row becomes manual); saving an empty note SHALL remove the note and keep the bookmark; removing the bookmark SHALL remove its note. Hovering the row's marker, or a bookmark mark in the overview strip, SHALL show the note in a tooltip. Adding, editing or removing a note SHALL be saved as a bookmark change.

#### Scenario: Adding a note
- **WHEN** the user right-clicks row 1,204, picks "Bookmark note…", types `retry storm starts here` and presses `Enter`
- **THEN** row 1,204 is bookmarked, its marker shows `✎`, and hovering the marker shows `retry storm starts here`.

#### Scenario: Note too long
- **WHEN** the user pastes 350 characters into the note editor
- **THEN** only the first 200 characters are kept.

#### Scenario: Note on an automatic bookmark
- **WHEN** row 42 carries only an automatic bookmark and the user saves the note `check this`
- **THEN** row 42 becomes a manual bookmark with that note, is saved in `fasttail.ini`, and counts toward the 1,000 per file.

### Requirement: Automatic Bookmarks from Rules
Every line matching an enabled highlight rule with "Bookmark matching lines" on SHALL carry an automatic bookmark, whatever the stream's filters: the lines already in the file when it is opened, reloaded or when the rules change, and every line appended afterwards, checked as it arrives. At most 10,000 automatic bookmarks SHALL be kept per stream, the first ones in file order; once the cap is reached no more are added and the stream bar SHALL show that automatic bookmarks are capped at 10,000. For files above 16 MB the existing lines SHALL be matched on a worker thread with progress in the stream bar, the UI staying responsive, and lines appended during that scan SHALL be matched exactly once. Automatic bookmarks SHALL appear in the overview strip with a dimmer mark than manual bookmarks. Turning the option off or disabling the rule SHALL remove the automatic bookmarks it produced at the next recomputation, and a rules change SHALL clear the dismissals. Standard-input and compressed streams SHALL be covered, compressed streams once their index is complete.

#### Scenario: Existing and appended lines
- **WHEN** a rule `OutOfMemoryError` has "Bookmark matching lines" on, the opened file has 3 matching lines, and 2 more matching lines are appended
- **THEN** all 5 lines show `☆`, and `F2` visits each of them in order.

#### Scenario: Large file
- **WHEN** a 2 GB log is opened with an auto-bookmark rule
- **THEN** the matching runs in the background with progress in the stream bar, rows stay scrollable during it, and the automatic bookmarks appear as the scan reports them.

#### Scenario: Cap reached
- **WHEN** an auto-bookmark rule `INFO` matches 250,000 lines of a file
- **THEN** the first 10,000 matching lines are bookmarked, the stream bar shows the capped notice, and further appended `INFO` lines are not bookmarked.

#### Scenario: Not saved
- **WHEN** a file has 12 automatic bookmarks and 3 manual bookmarks and FastTail is restarted
- **THEN** `fasttail.ini` holds only the 3 manual bookmarks, and the 12 automatic ones are recomputed from the rule when the file is reopened.
