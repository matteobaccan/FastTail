## MODIFIED Requirements

### Requirement: Live theme switching and persistence
The application SHALL allow switching themes dynamically at runtime without restarting the application. The graphical interface SHALL persist the selected theme to the configuration file. The terminal interface SHALL start with the theme from the configuration file (or `--theme`), SHALL switch to the next theme with `T` for the running session only, and SHALL NOT write the configuration file.

#### Scenario: Switching themes dynamically
- **WHEN** the user selects a different theme from the top bar theme selector or settings
- **THEN** all UI widgets, borders, scrollbars, and docking tabs re-render with the new palette on the very next frame.

#### Scenario: Theme persistence across sessions
- **WHEN** the user closes the application with the Blade theme active
- **THEN** the next launch automatically applies the Blade theme upon startup.

#### Scenario: Theme switch in the terminal is not saved
- **WHEN** `fasttail.ini` holds `theme=Tron`, the user presses `T` in the terminal interface until Matrix is active, and quits
- **THEN** the terminal redrew with the Matrix palette on the next frame, and `fasttail.ini` still holds `theme=Tron`.

## ADDED Requirements

### Requirement: Theme Palette for the Terminal Interface
Each theme's palette (level colours, accent, border, dim text, search hit, bookmark and highlight colours) SHALL be defined with a UI-neutral RGBA type, usable without the GUI toolkit; applying it to egui visuals SHALL stay in the graphical interface. The terminal interface SHALL use the palette for level colours, the focused border, the current search hit and the bookmark gutter, reduced to the terminal's colour depth by nearest match, and SHALL leave the background and plain text to the terminal's own colours, so the Light theme on a dark terminal (or a dark theme on a light terminal) keeps plain text readable.

#### Scenario: Matrix accent in a truecolor terminal
- **WHEN** the terminal interface runs with the Matrix theme in Windows Terminal
- **THEN** the focused window's border is drawn in the Matrix accent green as a 24-bit colour, and the window background is the terminal's own.

#### Scenario: Same palette in both interfaces
- **WHEN** the level colour of ERROR is read for the Blade theme by the GUI and by the terminal interface
- **THEN** both get the same RGBA value from the same function.
