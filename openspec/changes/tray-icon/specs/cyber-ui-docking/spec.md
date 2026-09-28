## ADDED Requirements

### Requirement: System Tray Icon
When the Settings option "Show tray icon" is on (`fasttail.ini` `[general] tray_icon`, default `false`), FastTail SHALL show an icon in the system tray on Windows, and on Linux when a StatusNotifierItem host is available. A left click SHALL toggle the window between shown and hidden, and the icon's menu SHALL offer Show / Hide FastTail, Follow all / Pause all, Mute sounds, one entry per open stream that shows the window with that stream focused, and Quit. On a Linux desktop without a tray host, the tray options SHALL be disabled with an explanation.

#### Scenario: Focusing a stream from the tray
- **WHEN** the window is hidden and the user picks `db.log` in the tray menu
- **THEN** the window is shown with the `db.log` stream focused.

#### Scenario: No tray on the desktop
- **WHEN** FastTail runs on a Linux desktop with no StatusNotifierItem watcher
- **THEN** the "Show tray icon" option is disabled and says that no system tray is available.

### Requirement: Hide to Tray
With the tray icon shown, the option "Minimise to tray" (`minimize_to_tray`, default `false`) SHALL hide the window and its taskbar button when it is minimised, and the option "Close to tray" (`close_to_tray`, default `false`) SHALL hide the window instead of quitting when it is closed, after saving the workspace. Quit in the tray menu and the new shortcut CTRL + Q SHALL always quit, whatever the tray options. While the window is hidden, tailing, rule matching, sound alerts and automatic bookmarks SHALL keep working.

#### Scenario: Closing to tray keeps tailing
- **WHEN** close to tray is on, the user closes the window and 500 lines are appended to a followed file
- **THEN** FastTail keeps running in the tray, and showing the window displays the 500 new lines.

#### Scenario: Defaults unchanged
- **WHEN** the tray options were never changed and the user closes the window
- **THEN** FastTail quits as in 0.12.0.

### Requirement: Tray Alert Badge
While the window is hidden or unfocused, FastTail SHALL count the lines matching a highlight rule with a sound preset, and the tray icon SHALL show a dot in the colour of the most severe of those presets with the count, capped at `99+`. The icon tooltip SHALL show the number of open streams, the alert count and the stream of the last alert. Showing and focusing the window SHALL reset the count and remove the dot.

#### Scenario: Alert while hidden
- **WHEN** the window is in the tray and a Critical rule matches 3 lines in `app.log`
- **THEN** the tray icon shows a critical-coloured dot with `3` and the tooltip mentions `app.log`.
