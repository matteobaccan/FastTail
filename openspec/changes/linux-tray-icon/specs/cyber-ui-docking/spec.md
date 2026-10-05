## MODIFIED Requirements

### Requirement: System Tray Icon
When the Settings option "Show tray icon" is on (`fasttail.ini` `[general] tray_icon`, default `false`), FastTail SHALL show an icon in the system tray on Windows, and on Linux when a StatusNotifierItem host is available. A left click SHALL toggle the window between shown and hidden, and the icon's menu SHALL offer Show / Hide FastTail, Follow all / Pause all, Mute sounds, one entry per open stream that shows the window with that stream focused, and Quit. Where no tray is available (Linux without a StatusNotifierItem host, and every other system), the tray options SHALL be disabled with an explanation.

#### Scenario: Focusing a stream from the tray
- **WHEN** the window is hidden and the user picks `db.log` in the tray menu
- **THEN** the window is shown with the `db.log` stream focused.

#### Scenario: Linux desktop with a tray host
- **WHEN** FastTail runs on KDE Plasma and the user turns "Show tray icon" on
- **THEN** the icon appears in the system tray with the same menu as on Windows.

#### Scenario: No tray on the desktop
- **WHEN** FastTail runs on a Linux desktop with no StatusNotifierItem watcher
- **THEN** the "Show tray icon" option is disabled and says that no system tray is available.
