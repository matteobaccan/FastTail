# Matrix Screensaver Specification

## Purpose
Renders a Matrix-style digital rain screensaver after a configurable idle period and dismisses it instantly on any user input.

## Requirements

### Requirement: Matrix digital rain screensaver
The application SHALL render a full-viewport digital rain screensaver (cascading vertical streams of green phosphor glyphs with fading trails and bright leading heads) when the user remains idle.

#### Scenario: Inactivity timeout triggers screensaver
- **WHEN** the application detects no mouse movement, mouse clicks, or keyboard input for the configured idle duration
- **THEN** it transitions smoothly from the active workspace into the Matrix digital rain screensaver animation.

#### Scenario: Immediate dismissal on user input
- **WHEN** the screensaver is running and the user moves the mouse, clicks, or presses any key
- **THEN** the screensaver dismisses immediately and returns the user to the exact prior workspace state without delay or visual stutter.

#### Scenario: Dismissal hands over to the PIN lock
- **WHEN** the screensaver is dismissed and the PIN lock is armed (see the window-lock capability)
- **THEN** the PIN prompt is shown instead of the workspace, and the workspace reappears only once the PIN is accepted.

#### Scenario: Unfocused window never animates
- **WHEN** the FastTail window does not have the keyboard focus (it sits behind another window, on another desktop, or is minimized) and the idle timeout elapses
- **THEN** the screensaver does not start, and a running screensaver stops as soon as the focus is lost, so the animation never consumes CPU while nobody can see it.

#### Scenario: Bounded frame rate
- **WHEN** the screensaver is running
- **THEN** it repaints at most about 30 times per second, independently of the renderer's vsync setting.

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

#### Scenario: Timeout of zero
- **WHEN** the user sets the idle timeout to 0 in Settings (the field accepts 0 to 120 and its tooltip reads "0 = never")
- **THEN** the value is kept as 0, persisted to `fasttail.ini`, and the screensaver never triggers.
