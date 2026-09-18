## ADDED Requirements

### Requirement: Capture-Only Highlighting
A regex highlight rule with capture groups MAY set "highlight captures only"; then only the captured spans of a matching row SHALL be painted with the rule's style while the rest of the row keeps its normal style. Rules without the option SHALL keep colouring the whole row. At most 64 spans per row SHALL be painted, first rule winning per byte.

#### Scenario: Colouring request ids
- **WHEN** the rule `req=(\d+)` with captures-only and a cyan foreground is enabled
- **THEN** only the digits after `req=` are cyan on each matching row.

### Requirement: Quick Colour Labels
Ctrl+Shift+1..9 SHALL create or toggle a quick label for the selected text (or the word under the current search match) using preset colour N, applied across all streams, listed in a strip above the stream with a remove button, and not persisted across restarts. Quick labels SHALL rank below user rules.

#### Scenario: Labelling a session id
- **WHEN** the user selects `sess-8f3a` and presses Ctrl+Shift+2
- **THEN** every occurrence of `sess-8f3a` in every stream is painted with preset colour 2 until the label is removed or the application restarts.
