## ADDED Requirements

### Requirement: Per-Stream Line Numbers
Each text stream SHALL offer a line-number toggle in its toolbar (`# 123` when shown, `# ---` when hidden) that shows or hides the line-number column of that stream only, in the normal and the wrap layouts. A newly opened stream (file, pattern, standard input, compressed file or archive entry) SHALL start from the `show_line_numbers` default in `fasttail.ini`, which Settings SHALL present as the default for new streams; changing it SHALL NOT change the streams already open. The switch SHALL be saved with the stream in the workspace and in session files as `line_numbers=true` or `line_numbers=false`, written only when it differs from the default, and a file without the key SHALL take the default.

#### Scenario: Hiding numbers in one pane
- **WHEN** two streams are open side by side with line numbers and the user clicks `# 123` in the first one's toolbar
- **THEN** the first stream shows no line numbers and its button reads `# ---`, while the second keeps its numbers.

#### Scenario: New stream takes the default
- **WHEN** `show_line_numbers=false` is set in `fasttail.ini` and the user opens a file
- **THEN** the new stream shows no line numbers.

#### Scenario: Restored on start
- **WHEN** the default is on, the user hides the numbers of one of two streams and restarts FastTail
- **THEN** that stream opens without line numbers, its workspace section holds `line_numbers=false`, and the other stream opens with them.
