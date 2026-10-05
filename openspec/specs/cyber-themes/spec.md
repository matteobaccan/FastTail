# Cyber Themes Specification

## Purpose
Defines the curated Tron, Matrix, Blade and Light color themes, live theme switching at runtime, and persistence of the selected theme.
## Requirements
### Requirement: Preset sci-fi visual themes
The application SHALL provide four curated, switchable color themes: three sci-fi themes inspired by cinematic aesthetics, **Tron**, **Matrix** and **Blade**, plus a clean **Light** theme for bright environments.

#### Scenario: Tron theme selection
- **WHEN** the user selects the Tron theme
- **THEN** the UI updates immediately with deep obsidian background, neon cyan primary borders, electric blue accents, and cold white text.

#### Scenario: Matrix theme selection
- **WHEN** the user selects the Matrix theme
- **THEN** the UI updates immediately with pure black background, phosphor terminal green borders and accents, and muted olive secondary highlights.

#### Scenario: Blade theme selection
- **WHEN** the user selects the Blade theme (Blade Runner noir)
- **THEN** the UI updates immediately with dark industrial charcoal background, warm amber/neon orange primary highlights, and magenta/crimson warning accents.

#### Scenario: Light theme selection
- **WHEN** the user selects the Light theme
- **THEN** the UI updates immediately with an off-white background, white panels, slate text and cool blue accents, keeping every widget readable in daylight.

### Requirement: Live theme switching and persistence
The application SHALL allow switching themes dynamically at runtime without restarting the application, in both the graphical and the terminal interface, and SHALL persist the selected theme to the configuration file. The terminal interface SHALL switch theme from its Settings dialog or with `T` (next theme), and `--theme` SHALL select a theme for one start without saving it.

#### Scenario: Switching themes dynamically
- **WHEN** the user selects a different theme from the top bar theme selector or settings
- **THEN** all UI widgets, borders, scrollbars, and docking tabs re-render with the new palette on the very next frame.

#### Scenario: Theme persistence across sessions
- **WHEN** the user closes the application with the Blade theme active
- **THEN** the next launch automatically applies the Blade theme upon startup.

#### Scenario: Theme chosen in the terminal, used by the GUI
- **WHEN** `fasttail.ini` holds `theme=Tron`, the user presses `T` in the terminal interface until Matrix is active, and quits
- **THEN** the terminal redrew with the Matrix palette on the next frame after each press, and the GUI started afterwards uses the Matrix theme.

#### Scenario: Theme from the command line is not saved
- **WHEN** `fasttail.ini` holds `theme=Tron` and the user runs `fasttail-tui --theme blade app.log` and quits without changing the theme
- **THEN** the terminal used the Blade palette, and `fasttail.ini` still holds `theme=Tron`.

### Requirement: Token Colours per Theme
Each theme SHALL define a foreground colour for each automatic highlighting kind (IP address, UUID, URL, duration, file path), each with a contrast ratio of at least 4.5:1 against the theme's row background, and switching the theme SHALL repaint the automatic token colours with the new theme's colours.

#### Scenario: Switching theme
- **WHEN** automatic highlighting is on and the user switches from Tron to Light
- **THEN** the IP addresses in the drawn rows change to the Light theme's IP colour.

#### Scenario: Readable on every theme
- **WHEN** the contrast of every token colour is measured against its theme's background
- **THEN** every ratio is at least 4.5:1.

### Requirement: Theme Palette for the Terminal Interface
Each theme's palette (level colours, accent, border, dim text, search hit, bookmark and highlight swatches) SHALL be defined with a UI-neutral RGBA type, usable without the GUI toolkit; applying it to egui visuals SHALL stay in the graphical interface. The terminal interface SHALL use the palette for level colours, the focused border, the current search hit, the bookmark gutter and the colour swatches of the rule editor, reduced to the terminal's colour depth by nearest match, and SHALL leave the background and plain text to the terminal's own colours, so the Light theme on a dark terminal (or a dark theme on a light terminal) keeps plain text readable.

#### Scenario: Matrix accent in a truecolor terminal
- **WHEN** the terminal interface runs with the Matrix theme in Windows Terminal
- **THEN** the focused window's border is drawn in the Matrix accent green as a 24-bit colour, and the window background is the terminal's own.

#### Scenario: Same palette in both interfaces
- **WHEN** the level colour of ERROR is read for the Blade theme by the GUI and by the terminal interface
- **THEN** both get the same RGBA value from the same function.

