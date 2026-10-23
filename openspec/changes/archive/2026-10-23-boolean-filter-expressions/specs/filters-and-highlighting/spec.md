## ADDED Requirements

### Requirement: Filter Expression Mode
Each stream SHALL offer an expression mode for the include side of its filter, switched with a toggle beside the Include field and off by default. In expression mode the include terms SHALL be replaced by one expression, and the exclude terms SHALL keep applying as in term mode, including to stack-trace continuation lines. An expression SHALL be made of operands joined by the upper-case operators `AND`, `OR` and `NOT` and by parentheses, with precedence `NOT` over `AND` over `OR`; two operands side by side SHALL mean `AND`, and the lower-case words `and`, `or`, `not` SHALL be operands. An operand SHALL be a bare word, a double-quoted phrase, a `/regex/`, or, on a stream with an active field parser, a field term in the form defined by the structured-fields capability; bare words and phrases SHALL be matched as text or as regular expressions according to the stream's case-sensitive and regex toggles, and a `/regex/` SHALL always be a regular expression under the case-sensitive toggle. A line SHALL pass the include side when the expression evaluates true on it; the exclude terms, the minimum level, the time range and the global filter SHALL then apply as in term mode. An expression SHALL hold at most 1,024 characters, 32 operands and 16 levels of nesting. A syntax error, a limit exceeded or an invalid regex SHALL be flagged under the field with its position, and the stream SHALL keep filtering with the last valid expression until the text is valid again; an expression that has never been valid SHALL filter nothing. Switching to expression mode with an empty expression SHALL fill it from the non-empty include terms joined by `AND`; switching back SHALL restore the include terms as they were, and neither switch SHALL clear the other mode's text. Filter presets SHALL save and restore the mode and the expression. The mode and the expression SHALL be persisted per stream in the workspace and in session files as `filter_mode=expr` (written only in expression mode) and `filter_expr`, and a file without them SHALL load in term mode.

#### Scenario: OR across conditions
- **WHEN** a stream in expression mode has the expression `(payment OR refund) AND timeout`
- **THEN** a line containing `refund` and `timeout` is visible, a line containing `payment` and `timeout` is visible, and a line containing `payment` without `timeout` is hidden.

#### Scenario: NOT and implicit AND
- **WHEN** the expression is `error NOT "retry worker"`
- **THEN** a line containing `error` is visible unless it also contains the phrase `retry worker`.

#### Scenario: Lower-case words are text
- **WHEN** the expression is `"not found" or`
- **THEN** only lines containing both the phrase `not found` and the word `or` are visible.

#### Scenario: Field terms as operands
- **WHEN** a stream with the logfmt parser active has the expression `status>=500 OR "circuit open"`
- **THEN** a line with `status=503` is visible, a line with `status=200` containing `circuit open` is visible, and a line with `status=200` without that phrase is hidden.

#### Scenario: Exclude terms still hide continuation lines
- **WHEN** the expression is `ERROR` and the exclude term is `at com.acme.health`
- **THEN** an ERROR entry is visible with its stack-trace lines, except the stack-trace lines containing `at com.acme.health`.

#### Scenario: Typing does not blank the view
- **WHEN** the expression `payment` is applied and the user types it into `(payment OR`
- **THEN** the field shows a syntax error at the end of the text, and the view keeps showing the lines containing `payment`.

#### Scenario: Switching modes keeps both
- **WHEN** a stream has the include terms `payment` and `timeout`, the user turns expression mode on, edits the expression to `payment OR timeout`, and turns it off again
- **THEN** the expression field first read `payment AND timeout`, the stream is back on the two include terms, and turning expression mode on again shows `payment OR timeout`.

#### Scenario: Large file
- **WHEN** a 2 GB stream switches to an expression with 4 operands
- **THEN** the interface stays responsive, the stream bar shows the background filter progress, and the result equals the one computed synchronously on the same lines.

#### Scenario: Persisted in a session
- **WHEN** the user saves a session with a stream in expression mode and loads it after a restart
- **THEN** the stream is in expression mode with the same expression, and an older FastTail loading the same session filters with the saved include terms.

### Requirement: Global Filter Expression Mode
The global filter SHALL offer the same expression mode for its include side, with the same syntax, limits and error handling as a stream's, switched in the global filter bar. A line of any stream SHALL pass the global include side when the global expression evaluates true on it, and SHALL be visible only when it also passes that stream's own filter, in term or expression mode. The global mode and expression SHALL be persisted in the `[global_filter]` section of `fasttail.ini` as `mode=expr` and `expression`, and SHALL reach the streams at most 300 ms after the last keystroke.

#### Scenario: Global OR with a stream term
- **WHEN** the global expression is `req-7f3a OR req-9b21` and a stream's own include term is `ERROR`
- **THEN** that stream shows only the ERROR lines containing `req-7f3a` or `req-9b21`.

#### Scenario: Global expression survives a restart
- **WHEN** the user sets the global expression `NOT healthcheck` and restarts FastTail
- **THEN** the global filter bar is in expression mode with `NOT healthcheck`, and no stream shows a line containing `healthcheck`.
