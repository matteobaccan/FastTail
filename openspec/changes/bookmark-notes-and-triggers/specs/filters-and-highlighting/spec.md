## ADDED Requirements

### Requirement: Bookmark Matching Lines Rule Option
Each highlight rule SHALL have a "Bookmark matching lines" option, off by default and off for new rules, edited in the rule editor beside the sound alert. It SHALL be independent of the rule's colours, font styles, sound alert and capture-only mode: a rule lower in the list whose colours are hidden by a higher rule under first-match evaluation SHALL still bookmark the lines it matches. The option SHALL be persisted in the rule's `[highlight_<n>]` section of `fasttail.ini` as `bookmark` (`true` / `false`); a section without the key SHALL load with the option off. Changing the option, the pattern or the enabled state of a rule with the option SHALL recompute the automatic bookmarks of every open stream.

#### Scenario: Option persisted
- **WHEN** the user turns on "Bookmark matching lines" for the rule `FATAL` and restarts FastTail
- **THEN** `fasttail.ini` holds `bookmark=true` in that rule's section and the option is still on.

#### Scenario: Rule hidden by a higher rule
- **WHEN** rule 1 `ERROR` has no bookmark option, rule 2 `timeout` has it on, and a line contains both `ERROR` and `timeout`
- **THEN** the line is painted with rule 1's colours and carries an automatic bookmark.

#### Scenario: Older settings file
- **WHEN** `fasttail.ini` has a `[highlight_0]` section without a `bookmark` key
- **THEN** the rule loads with "Bookmark matching lines" off.
