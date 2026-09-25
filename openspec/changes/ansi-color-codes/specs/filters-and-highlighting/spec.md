## MODIFIED Requirements

### Requirement: Capture-Only Highlighting
A regex highlight rule with capture groups MAY set "highlight captures only"; then only the captured spans of a matching row SHALL be painted with the rule's style while the rest of the row keeps its normal style. Rules without the option SHALL keep colouring the whole row. At most 64 spans per row SHALL be painted, first rule winning per byte. The 64-span budget SHALL be shared, in priority order, by user rules, quick labels and then the ANSI colour spans of a stream in render mode (see the ansi-escape-codes capability); once the budget is used, the remaining bytes of the row keep their normal style.

#### Scenario: Colouring request ids
- **WHEN** the rule `req=(\d+)` with captures-only and a cyan foreground is enabled
- **THEN** only the digits after `req=` are cyan on each matching row.

#### Scenario: Captures inside a coloured row
- **WHEN** the same rule is enabled and a row in ANSI render mode shows `req=42` inside a yellow ANSI span
- **THEN** the digits `42` are cyan and the rest of the yellow span stays yellow.
