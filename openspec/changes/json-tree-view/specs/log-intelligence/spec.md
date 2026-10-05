## MODIFIED Requirements

### Requirement: Inline JSON auto-detection and expansion
The application SHALL treat a line as a JSON payload when, trimmed, it starts with `{` and ends with `}` or starts with `[` and ends with `]`, or when a JSON object follows a leading timestamp, and SHALL render an inline toggle (`[+] JSON`, `[-] JSON` once expanded) that expands the payload into a foldable tree below the row. The tree SHALL keep the order of the line; objects and arrays SHALL be nodes showing their key and a summary (`{4 keys}`, `[12 items]`), the first level open and deeper levels folded; scalars SHALL be coloured by type; strings longer than 500 characters SHALL be cut with `…`; a container SHALL show at most its first 200 children followed by a `… N more` row. A payload over 4 MB SHALL NOT be parsed, and malformed JSON SHALL show what was read and where it stopped. The fold state of each line SHALL be kept while the stream is open and cleared when it reloads.

#### Scenario: Line containing valid JSON payload
- **WHEN** a log line is a JSON object or array, or `2026-10-05T10:00:00Z {"level":"error"}`
- **THEN** the viewer displays a `[+] JSON` toggle next to the line, leaving the collapsed line unchanged by default.

#### Scenario: User clicks expand JSON
- **WHEN** the user clicks the inline `[+] JSON` toggle of `{"b":1,"a":{"c":[1,2]}}`
- **THEN** the line expands into a tree listing `b: 1` then `a {1 key}` folded, in the line's order, without breaking the overall scroll position, and the toggle reads `[-] JSON` until it is clicked again.

#### Scenario: Folding a node
- **WHEN** the user clicks the folded node `a {1 key}`
- **THEN** it opens and shows `c [2 items]`, folded.

## ADDED Requirements

### Requirement: JSON node actions
Every node of a JSON tree SHALL offer: copy the value (a scalar as its text, an object or array as indented JSON in the line's order), copy the path (`.key` steps for identifier keys, `["key"]` otherwise, `[n]` for array items, without a leading dot), expand all below it (at most 5,000 rows, saying so when cut) and collapse all below it. In the GUI they SHALL be in the node's context menu.

#### Scenario: Copy a path
- **WHEN** the user copies the path of `id` in the first item of `items` in `{"items":[{"id":7}],"x-req":{"a b":1}}`, then the path of `a b`
- **THEN** the clipboard holds `items[0].id`, then `["x-req"]["a b"]`.

### Requirement: JSON tree in the terminal interface
In the terminal interface `J` SHALL open a dialog titled `JSON - line N` with the tree of the cursor row's payload: `↑` / `↓` move, `→`, `Enter` or `Space` unfold, `←` folds the node or goes to its parent, `*` expands all, `-` collapses all, `y` copies the value and `Y` the path through the terminal clipboard, a click toggles a node and `Esc` closes. On a row without JSON, `J` SHALL say so in the status bar.

#### Scenario: Terminal tree
- **WHEN** the cursor is on `{"user":{"id":7}}` and the user presses `J`, then `→` on `user {1 key}`
- **THEN** the dialog shows `user {1 key}` open with `id: 7` below it.
