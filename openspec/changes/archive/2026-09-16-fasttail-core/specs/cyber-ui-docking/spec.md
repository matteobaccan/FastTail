## ADDED Requirements

### Requirement: Cyberpunk sci-fi HUD visual styling
The application SHALL render an eDEX-UI inspired sci-fi HUD theme using `egui` and `eframe` with high-contrast neon accents, glowing panel borders, dark metallic backgrounds, and embedded monospace typography with font ligatures.

#### Scenario: Application startup rendering
- **WHEN** FastTail launches
- **THEN** it displays the cyber HUD color scheme with monospace log lines rendered at 60 FPS or higher.

### Requirement: Modular docking system
The UI SHALL implement a docking workspace using `egui_dock` where every panel (telemetry, highlight rules, search filters, log view) can be resized, dragged, docked, collapsed, or closed independently.

#### Scenario: User collapses or closes a panel
- **WHEN** the user clicks the close or collapse button on a docked panel (such as telemetry)
- **THEN** the panel disappears and the remaining log viewing space expands to fill the vacated area.

#### Scenario: User resets workspace layout
- **WHEN** the user selects the default HUD layout preset or presses the reset shortcut
- **THEN** all closed or relocated panels return to their default positions.

### Requirement: High-performance virtual scrolling
The log viewer panel SHALL use virtual scrolling to render exclusively the lines visible within the current viewport window, maintaining responsiveness regardless of total line count.

#### Scenario: Scrolling through millions of lines
- **WHEN** the user navigates a log buffer containing 5 million lines
- **THEN** the UI only allocates and renders the approximately 50 to 100 rows currently on screen, keeping CPU usage below 5%.
