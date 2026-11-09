## ADDED Requirements

### Requirement: Terminal Remappable Keys
The terminal interface SHALL read key overrides for its actions from `[shortcuts]` under a `tui.` prefix, SHALL offer a Keys dialog to record, reset and check them for conflicts, and SHALL offer the window's applicable setting toggles in its palette.

#### Scenario: A remapped key
- **WHEN** `[shortcuts] tui.stream.follow.toggle=f` is set
- **THEN** `f` toggles follow in the terminal and Space no longer does.

#### Scenario: Conflict
- **WHEN** the user records `b` for Follow in the Keys dialog
- **THEN** the dialog says `b` is taken by Toggle bookmark and asks before moving it.
