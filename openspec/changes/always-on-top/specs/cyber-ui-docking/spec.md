## ADDED Requirements

### Requirement: Always-On-Top Window
The title bar SHALL offer a pin toggle, mirrored by a Settings checkbox and by Ctrl+Shift+T, that keeps the main window above other windows. The state SHALL be persisted in `fasttail.ini` and applied at startup.

#### Scenario: Pinning the window
- **WHEN** the user clicks the pin and then focuses another application
- **THEN** the FastTail window stays visible above it, and the pin is highlighted.

#### Scenario: Restart with pin active
- **WHEN** FastTail was closed with the pin active
- **THEN** the next start opens the window already on top.
