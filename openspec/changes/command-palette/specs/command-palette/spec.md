## ADDED Requirements

### Requirement: Command Palette
CTRL + SHIFT + P and the title-bar menu item "Command palette…" SHALL open a palette with a text box and a list of commands. The list SHALL contain every window, stream, search, bookmark, session and view action of the action registry, and one command per boolean or enumerated setting of the Settings page. Each row SHALL show the localized name, the category and the current shortcut when the action has one. `Up`, `Down`, `PgUp` and `PgDown` SHALL move the selection, `Enter` SHALL run the selected command and close the palette, and `Esc` or CTRL + SHIFT + P SHALL close it without running anything. While the palette is open, keys SHALL NOT reach the streams. The palette SHALL NOT open while the window lock is armed.

#### Scenario: Running a stream action by name
- **WHEN** a stream is focused, the user presses CTRL + SHIFT + P, types `wrap` and presses Enter
- **THEN** the focused stream toggles line wrap exactly as ALT + W would, and the palette closes.

#### Scenario: Shortcut shown
- **WHEN** the palette lists "Search all streams"
- **THEN** its row shows `CTRL + SHIFT + F`.

#### Scenario: Keys do not leak
- **WHEN** the palette is open and the user presses Space
- **THEN** a space is typed in the palette box and no stream toggles follow mode.

### Requirement: Palette Search and Ranking
Typing SHALL filter the commands by a case- and accent-insensitive subsequence match against both the localized name and the English name, ranking higher the matches with consecutive characters, matches at word starts and commands used recently. With an empty box the last 8 commands run from the palette SHALL be listed first, and their ids SHALL be kept in `fasttail.ini` `[general]` as `palette_recent`.

#### Scenario: English name in another language
- **WHEN** the interface language is German and the user types `collapse`
- **THEN** the collapse command is listed under its German name.

#### Scenario: Recent first
- **WHEN** the user ran "Toggle time delta" from the palette and reopens it with an empty box
- **THEN** "Toggle time delta" is the first row.

### Requirement: Context-Aware Commands
Stream commands SHALL act on the stream that was focused when the palette opened. A command that cannot run in the current state (no focused stream, no search for "Next match", HEX view for text-only actions) SHALL be listed greyed with the reason and SHALL NOT run on Enter. Enumerated settings (theme, language, renderer, size unit) SHALL open a second step listing their values, with the current value marked.

#### Scenario: No stream open
- **WHEN** no stream is open and the user selects "Toggle follow"
- **THEN** the row is greyed with "No stream focused" and Enter does nothing.

#### Scenario: Choosing a theme
- **WHEN** the user runs "Theme…"
- **THEN** the palette lists the themes with the current one marked, and Enter on another theme switches to it live.

### Requirement: Action Registry
Every action listed by the palette SHALL be defined once in an action registry with a stable identifier, an i18n name, a category, a scope (window, stream or list), an enabled condition and a shortcut label, and running it from the palette SHALL have the same effect as its menu item or shortcut. Every menu item other than a file or session path SHALL have a registry entry.

#### Scenario: Palette equals menu
- **WHEN** the user runs "Export visible lines…" from the palette
- **THEN** the same save dialog opens as from the stream menu.
