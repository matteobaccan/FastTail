## ADDED Requirements

### Requirement: Matrix digital rain screensaver
The application SHALL render a full-viewport digital rain screensaver (cascading vertical streams of green phosphor glyphs with fading trails and bright leading heads) when the user remains idle.

#### Scenario: Inactivity timeout triggers screensaver
- **WHEN** the application detects no mouse movement, mouse clicks, or keyboard input for the configured idle duration
- **THEN** it transitions smoothly from the active workspace into the Matrix digital rain screensaver animation.

#### Scenario: Immediate dismissal on user input
- **WHEN** the screensaver is running and the user moves the mouse, clicks, or presses any key
- **THEN** the screensaver dismisses immediately and returns the user to the exact prior workspace state without delay or visual stutter.

### Requirement: Screensaver configuration and defaults
The screensaver SHALL be enabled by default with a default timeout of 10 minutes, and SHALL provide configuration options to adjust the timeout duration or disable the feature entirely.

#### Scenario: Default screensaver settings on clean start
- **WHEN** FastTail boots for the first time
- **THEN** screensaver is configured to `enabled: true` with a timeout of `10 minutes`.

#### Scenario: User customizes timeout duration
- **WHEN** the user sets the idle timeout to 5 minutes in settings
- **THEN** the screensaver activates after 5 minutes of inactivity and the setting is persisted to `fasttail.ini`.

#### Scenario: User disables screensaver
- **WHEN** the user toggles the screensaver off in settings
- **THEN** the screensaver never triggers regardless of idle duration.
