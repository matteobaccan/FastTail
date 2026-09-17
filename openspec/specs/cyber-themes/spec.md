## ADDED Requirements

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
The application SHALL allow switching themes dynamically at runtime without restarting the application, and SHALL persist the selected theme to the configuration file.

#### Scenario: Switching themes dynamically
- **WHEN** the user selects a different theme from the top bar theme selector or settings
- **THEN** all UI widgets, borders, scrollbars, and docking tabs re-render with the new palette on the very next frame.

#### Scenario: Theme persistence across sessions
- **WHEN** the user closes the application with the Blade theme active
- **THEN** the next launch automatically applies the Blade theme upon startup.
