## MODIFIED Requirements

### Requirement: Recent Files Menu (MRU)
The top navigation bar SHALL provide a Recent Files dropdown as an icon-only button (🕒) placed immediately to the right of the Open File button, whose tooltip SHALL read the localized "Recent Files" label. The dropdown SHALL list previously opened log files in Most Recently Used order, allowing instant reopening with a single click and a clear recent list option.

#### Scenario: Reopening a file from the recent list
- **WHEN** the user opens the Recent dropdown and clicks a previously opened log file
- **THEN** the file opens in a new stream tab and moves to the top of the Most Recently Used list.

#### Scenario: Icon button next to Open File
- **WHEN** the title bar is rendered in any language
- **THEN** the 🕒 button sits directly after the Open File button, before the Filter button, shows no text, and hovering it displays the "Recent Files" label of the current language.
