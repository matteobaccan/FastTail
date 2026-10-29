## ADDED Requirements

### Requirement: Rotated Files as One Stream
When a file opened as a stream has rotations next to it (`<name>.<n>`, the same compressed, `<name>-<date>`, `<stem>.<date>.<ext>`), the stream SHALL offer to include them, as `rotations=ask` (default), `always` or `never` decides. A stream with rotations SHALL read the pieces as one file, oldest first, the live file last and followed, and SHALL mark where each piece starts.

#### Scenario: Yesterday and today
- **WHEN** the user opens `app.log` with `app.log.1` and `app.log.2.gz` next to it and accepts the offer
- **THEN** the stream shows the lines of `app.log.2.gz`, then `app.log.1`, then `app.log`, numbered as one file, and the gutter marks the first line of each piece.

#### Scenario: Rotation while following
- **WHEN** a stream with rotations follows `app.log` and the application renames it to `app.log.1` and starts a new `app.log`
- **THEN** the stream keeps every line it showed and goes on with the lines of the new `app.log`.

### Requirement: Rotations Remembered
The choice to include rotations SHALL be kept for the stream in the workspace and sessions (`rotations=1`), and `--rotations` SHALL include them for the files on the command line.

#### Scenario: Session restore
- **WHEN** a session saved with a stream that includes its rotations is loaded
- **THEN** the stream opens with its rotations without asking again.
