## MODIFIED Requirements

### Requirement: Arming the lock
With a PIN set, the application SHALL lock on demand — a button in the settings and the `Ctrl+L` shortcut — and, when the master switch is on, SHALL lock when the Matrix screensaver ends. Without a PIN set, no action SHALL ever lock the window. The terminal interface SHALL arm the same lock from the same stored PIN: `Ctrl+L` and a "Lock now" entry in its Settings when a PIN is set, and, when the master switch is on and `screensaver_enabled` is true, after `screensaver_timeout_mins` minutes without a key or mouse event, going straight to its lock screen without a screensaver. The PIN check, the attempt counter and the cooldown SHALL be the same code in both interfaces.

#### Scenario: Returning from the screensaver
- **WHEN** the lock is armed, the screensaver is running, and the user moves the mouse or presses a key
- **THEN** the screensaver dismisses and the PIN prompt is shown instead of the workspace.

#### Scenario: Locking on demand
- **WHEN** the user presses `Ctrl+L` or clicks "Lock now" with a PIN set
- **THEN** the window locks immediately, whatever the screensaver switch says.

#### Scenario: No PIN set
- **WHEN** `Ctrl+L` is pressed and no PIN is stored
- **THEN** nothing happens and the workspace stays usable.

#### Scenario: Idle lock in the terminal
- **WHEN** the lock is armed with `screensaver_timeout_mins=10` and the terminal interface receives no key or mouse event for 10 minutes
- **THEN** it shows its full-screen PIN dialog, no screensaver is drawn, and no log text is visible.

#### Scenario: PIN set in the GUI unlocks the terminal
- **WHEN** the PIN `4821` was set in the GUI and the user locks the terminal interface with `Ctrl+L`
- **THEN** `4821` unlocks it, and three wrong PINs start the same 60-second cooldown as in the GUI.
