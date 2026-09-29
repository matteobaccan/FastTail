## ADDED Requirements

### Requirement: Selection Highlight
Double-clicking on the text of a row in the Text view SHALL pick the token under the pointer, the longest run of letters, digits and the characters `_ . : / @ - %` around it with trailing `.` and `:` removed, when it is 2 to 256 bytes long, and SHALL outline every exact, case-sensitive occurrence of that token in every row of that stream, in the normal and the wrap layouts, without changing the search, the filters or the row colours. The row context menu SHALL offer the same action on the token under the pointer. The outline SHALL be cleared by `Esc` while the rows have the keyboard, by double-clicking on empty space or on the same token again, and by a reload of the stream; double-clicking another token SHALL replace it. The outline SHALL NOT count against the 64 painted spans of a row and SHALL NOT be persisted.

#### Scenario: Other occurrences of a request id
- **WHEN** the user double-clicks `req-7f3a` in a row
- **THEN** every occurrence of `req-7f3a` in the rows of that stream is outlined, including inside rows coloured by a rule, and the search box is unchanged.

#### Scenario: Trailing punctuation
- **WHEN** the user double-clicks the address in `connecting to 10.0.4.17:8443.`
- **THEN** the token `10.0.4.17:8443` is outlined, without the final dot.

#### Scenario: Clearing
- **WHEN** a token is outlined and the user presses `Esc` with the rows focused
- **THEN** no outline remains.

### Requirement: Rule Set Import and Export
The Highlights dialog SHALL offer "Export rules…", which writes every highlight rule to a file chosen in the native save dialog, and "Import rules…", which reads such a file. The file SHALL be an INI file with a `[fasttail_rules]` section holding `version=1` followed by `[highlight_N]` sections with the same keys and values as the rules in `fasttail.ini` (pattern, regex and case flags, colours, bold, italic, sound alert, enabled, captures only, bookmark matching lines); the suggested name SHALL end in `.fasttail-rules.ini`. External tool bindings SHALL NOT be exported. Import SHALL show how many rules the file holds and offer Append, which adds the rules after the existing ones skipping any rule whose pattern, regex flag and case flag equal an existing rule and reports how many were skipped, and Replace, which replaces every rule after a confirmation. A file without the `[fasttail_rules]` section, or with a version above 1, SHALL be refused with a message and change nothing.

#### Scenario: Sharing a rule set
- **WHEN** the user exports 20 rules and a colleague imports the file with Append into an installation that already has 3 rules, one of them equal to an exported rule
- **THEN** the colleague has 22 rules and the dialog reports that 1 rule was skipped.

#### Scenario: Round trip keeps every setting
- **WHEN** a rule with captures only, bold, a Critical sound alert and "Bookmark matching lines" is exported and imported with Replace
- **THEN** the imported rule has the same pattern, flags, colours, style, sound alert and options.

#### Scenario: Not a rule set
- **WHEN** the user imports a `fasttail.ini` or any file without a `[fasttail_rules]` section
- **THEN** the import is refused with a message and the rules are unchanged.

### Requirement: Automatic Token Highlighting
The application SHALL offer automatic highlighting of tokens, off by default, switched in Settings and saved as `auto_highlight` in the `[general]` section of `fasttail.ini`, with a toggle per kind saved as `auto_highlight_kinds` (default all): IPv4 addresses with an optional port, IPv6 addresses, UUIDs, URLs with the schemes http, https, ftp, ws, wss and file, durations (a number followed by `ns`, `µs`, `us`, `ms`, `s`, `m`, `h` or `d`, or a chain such as `2m30s`), and file paths (starting with `/`, `~/`, a drive letter and `:\`, or `\\`). While it is on, each token found in a drawn row SHALL be painted with the foreground colour of its kind from the current theme. Automatic token spans SHALL rank below user rules, quick labels and ANSI colour spans and above the level colouring, and SHALL share the 64-span budget of a row after them. The detection SHALL NOT use regular expressions and SHALL cost no more than one pass over the drawn row. Filters, search, copy and export SHALL be unaffected. The HEX and Markdown views SHALL be unaffected.

#### Scenario: Colouring an access log
- **WHEN** automatic highlighting is on and a row reads `10.0.4.17 GET https://api.example.com/v1/orders 503 in 1.25s`
- **THEN** `10.0.4.17`, `https://api.example.com/v1/orders` and `1.25s` are painted with the IP, URL and duration colours of the theme.

#### Scenario: Rules win
- **WHEN** a user rule paints the whole row red and the row contains a UUID
- **THEN** the UUID stays red.

#### Scenario: Not everything is a token
- **WHEN** a row reads `version 1.2.3 took 1/2 of the budget at 14:02:05`
- **THEN** none of `1.2.3`, `1/2` and `14:02:05` is painted.

#### Scenario: Off by default
- **WHEN** FastTail starts with a `fasttail.ini` that has no `auto_highlight` key
- **THEN** no automatic token colour is painted.
