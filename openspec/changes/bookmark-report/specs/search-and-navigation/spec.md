## ADDED Requirements

### Requirement: Bookmark Tags
Every `#` word in a bookmark note SHALL be a tag: `#` at the start of the note or after a space, followed by 1 to 32 characters among letters, digits, `-`, `_` and `.`, containing at least one letter, a trailing `.` excluded, compared case-insensitively. Tags SHALL be stored only as part of the note text, so they are saved and restored wherever notes are. The note tooltip of the marker column and of the overview strip SHALL show the tags as chips. The go-to popup (`CTRL + G`) SHALL accept `#tag`: it SHALL jump to the next manual bookmark after the current row whose note carries that tag and that is visible under the active filters, wrapping around; when none exists the popup SHALL say that no bookmark carries the tag. Typing `#` in the popup SHALL suggest the tags of the focused stream.

#### Scenario: Jumping to a tag
- **WHEN** rows 120, 900 and 4,500 carry notes `start #deploy`, `pool exhausted #db` and `rollback #Deploy`, the current row is 200, and the user enters `#deploy` in the go-to popup
- **THEN** the view jumps to row 4,500, and entering `#deploy` again wraps to row 120.

#### Scenario: Numbers are not tags
- **WHEN** a note reads `see issue #42 and #db.`
- **THEN** the note has the single tag `#db`.

#### Scenario: Unknown tag
- **WHEN** no note of the focused stream carries `#cache` and the user enters `#cache` in the go-to popup
- **THEN** the view does not move and the popup says that no bookmark is tagged `#cache`.
