# Window PIN Lock Specification

## Purpose
Puts the window behind a numeric PIN when the operator walks away, so a log left open on screen is not readable by whoever passes by the desk. It is a deterrent, explicitly not a security boundary.

## Requirements

### Requirement: PIN definition and storage
The application SHALL let the user set a PIN of 4 to 12 digits in the settings, SHALL reject anything else, and SHALL let the user remove it. The PIN SHALL NOT be written to `fasttail.ini` in clear: only a scrambled form (`lock_pin`) SHALL be stored, together with the master switch (`lock_enabled`).

#### Scenario: Setting a PIN
- **WHEN** the user types four to twelve digits and confirms
- **THEN** the scrambled PIN is stored in `fasttail.ini` and the clear text never leaves the dialog; the typed text is never echoed on screen.

#### Scenario: Rejecting an invalid PIN
- **WHEN** the user types fewer than 4 digits, more than 12, or anything that is not a digit
- **THEN** the confirm button stays disabled and explains the accepted format.

#### Scenario: Removing the PIN
- **WHEN** the user removes the PIN
- **THEN** the stored value is cleared and the master switch is turned off, so the application can never lock itself with a PIN nobody knows.

### Requirement: Arming the lock
With a PIN set, the application SHALL lock on demand — a button in the settings and the `Ctrl+L` shortcut — and, when the master switch is on, SHALL lock when the Matrix screensaver ends. Without a PIN set, no action SHALL ever lock the window.

#### Scenario: Returning from the screensaver
- **WHEN** the lock is armed, the screensaver is running, and the user moves the mouse or presses a key
- **THEN** the screensaver dismisses and the PIN prompt is shown instead of the workspace.

#### Scenario: Locking on demand
- **WHEN** the user presses `Ctrl+L` or clicks "Lock now" with a PIN set
- **THEN** the window locks immediately, whatever the screensaver switch says.

#### Scenario: No PIN set
- **WHEN** `Ctrl+L` is pressed and no PIN is stored
- **THEN** nothing happens and the workspace stays usable.

### Requirement: Behaviour while locked
While locked, the application SHALL cover the window with an **opaque** backdrop and the modal PIN prompt, so none of the log contents stays readable behind it; SHALL drop every keyboard event the workspace could act on, keeping only what the PIN field needs (text, the editing keys, `Enter`) — a bare `Escape` included, since the workspace behind must not react to it; and SHALL keep tailing every stream so no appended line is missed. Unlocking SHALL restore the exact prior workspace state.

#### Scenario: Log keeps growing behind the lock
- **WHEN** the window is locked and the tailed files receive new lines
- **THEN** the streams keep reading them, and unlocking shows the log up to date.

#### Scenario: The workspace is hidden, not dimmed
- **WHEN** the window is locked over an open log
- **THEN** the log text is not visible behind the prompt.

#### Scenario: Keys do not reach the workspace
- **WHEN** the user presses `Escape`, `F1` or any shortcut while locked
- **THEN** nothing behind the prompt reacts and the window stays locked.

#### Scenario: Wrong PIN
- **WHEN** the entered PIN does not match
- **THEN** the field is cleared, a "wrong PIN" message is shown, and the window stays locked.

### Requirement: The lock is a deterrent, not a security boundary
The application SHALL NOT present the lock as protection for the log contents. A maintenance unlock phrase SHALL always open the prompt regardless of the stored PIN, the log files stay readable on disk and `fasttail.ini` stays editable; the settings and the user documentation SHALL state this plainly.

#### Scenario: The settings say what the lock is worth
- **WHEN** the user opens the PIN lock section
- **THEN** it states that the lock is a deterrent and that the log files remain readable on disk.
