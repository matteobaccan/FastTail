## ADDED Requirements

### Requirement: Desktop Notification per Highlight Rule
Each highlight rule SHALL have a Notify option (off by default), persisted as `notify` in its `fasttail.ini` rule section. When an enabled rule with Notify matches a line appended to a stream, the application SHALL show a desktop notification titled `FastTail · <stream title>` whose body holds the matching line without ANSI escape sequences, cut at 200 characters, and the rule's pattern. Lines read when a stream is opened, reloaded after a truncation or rotation, re-decoded or restored from a session SHALL NOT cause notifications. The rule's sound alert, automatic bookmark and bound tools SHALL keep working independently of this option. On Windows the notification SHALL be a toast, on Linux a freedesktop notification and on macOS a Notification Center notification; when the platform service is unavailable, the Notify option SHALL be disabled with the reason.

#### Scenario: Error while working elsewhere
- **WHEN** a rule `FATAL` has Notify on, the FastTail window is not focused, and `FATAL db connection lost` is appended to `api.log`
- **THEN** a desktop notification titled `FastTail · api.log` shows `FATAL db connection lost` and the pattern `FATAL`.

#### Scenario: Opening an old file
- **WHEN** the user opens a file that already holds 5 000 lines matching a rule with Notify on
- **THEN** no notification is shown, and the next matching line appended afterwards shows one.

#### Scenario: No notification service
- **WHEN** FastTail runs on Linux without a D-Bus session
- **THEN** the Notify checkbox is disabled and its tooltip says that no notification service is available.

### Requirement: Notification Throttling and Visibility
The application SHALL show at most one notification per rule every 10 seconds; the matches in between SHALL be counted, the next notification of that rule SHALL say `+N more`, and when the 10 seconds pass with matches counted and no new notification, a summary notification SHALL give their number. At most 3 notifications SHALL be shown every 10 seconds in total, the others being counted in the same way. With `notify_when = background` (the default) a match SHALL notify only when the window is unfocused or minimised or the stream's tab is not displayed; with `always` it SHALL notify regardless. The Settings switch `notifications_enabled` (default on) SHALL turn every notification off, and Settings SHALL offer a button that shows a test notification.

#### Scenario: Burst of errors
- **WHEN** a rule with Notify on matches 3 appended lines within one second while the window is unfocused
- **THEN** one notification shows the first line, and 10 seconds later a summary says there were 2 more matches.

#### Scenario: User already looking
- **WHEN** `notify_when` is `background`, the window is focused, and the matching stream is the displayed tab
- **THEN** no notification is shown.

### Requirement: Notification Click Action
Where the platform reports a click on a notification, clicking it SHALL bring the FastTail window to the front, activate the tab of the stream that produced it and scroll to the matching line; when that stream has been closed or the line no longer exists, the click SHALL only bring the window to the front.

#### Scenario: Jump to the error
- **WHEN** the user clicks the notification of line 18 204 of `api.log` on Linux
- **THEN** the FastTail window comes to the front with the `api.log` tab active and line 18 204 visible and selected.
