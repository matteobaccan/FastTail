## ADDED Requirements

### Requirement: Character Selection Inside a Row
In the Text view each stream SHALL keep at most one character selection, inside a single row, beside its row selection. Pressing the mouse on a row's text and dragging SHALL select the characters between the press and the pointer, clamped to that row and scrolling horizontally near the edges; a press released without moving more than 4 pixels SHALL act as today's click and place a caret at that character. With a caret in the focused stream, `SHIFT + ←` / `SHIFT + →` SHALL extend the selection by one character, `CTRL + SHIFT + ←` / `CTRL + SHIFT + →` by one word, and `SHIFT + Home` / `SHIFT + End` to the start or end of the row; these keys SHALL NOT be consumed when the focused stream has no caret. A double-click SHALL select the word under the pointer and a triple-click the whole row text. The selection SHALL be drawn over the row tints in the theme's selection colour, and SHALL be cleared by `Esc`, by a click on another row, when the file is truncated or reloaded, and when a filter change hides its row. With a non-empty character selection, `CTRL + C` SHALL copy exactly the selected characters of the shown text (without ANSI escapes in render and strip modes, tabs kept, never the `×N` badge or the gutter) and the row context menu SHALL offer "Copy selected text"; otherwise `CTRL + C` SHALL copy the selected rows as before. `CTRL + F` with a non-empty character selection of at most 256 characters SHALL put that text in the search box. Row selection by click, `SHIFT + click`, `CTRL + click` and `CTRL + A` SHALL be unchanged. The HEX and rendered Markdown views SHALL keep row selection only.

#### Scenario: Copying a request id
- **WHEN** row 88 reads `INFO req=7f3a9c21 user=bob done` and the user drags from `7` to `1` of `7f3a9c21` and presses `CTRL + C`
- **THEN** the clipboard holds `7f3a9c21` and row 88 is the selected row.

#### Scenario: Keyboard extension
- **WHEN** the user clicks just before `user` in row 88 and presses `CTRL + SHIFT + →` once
- **THEN** the character selection is `user` and `CTRL + C` copies `user`.

#### Scenario: Drag clamped to the row
- **WHEN** the user presses on row 88 and releases the mouse over row 95
- **THEN** only characters of row 88 are selected and rows 89 to 95 are not selected.

#### Scenario: Rows still copy
- **WHEN** rows 3 and 4 are selected by `SHIFT + click` and no character selection exists
- **THEN** `CTRL + C` copies both rows in full, as before.

#### Scenario: Search from the selection
- **WHEN** the character selection is `OutOfMemoryError` and the user presses `CTRL + F`
- **THEN** the search box of the focused stream holds `OutOfMemoryError`.

#### Scenario: Coloured stream
- **WHEN** a stream in ANSI render mode shows `ESC[31mERRORESC[0m payment failed` and the user selects `ERROR pay`
- **THEN** `CTRL + C` copies `ERROR pay` without any escape byte.
