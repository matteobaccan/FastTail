# Crash Reporting Specification

## Purpose
Defines what the panic handler records in `fasttail_crash.log`, where the file is written, and where users are sent to report a crash.

## Requirements

### Requirement: Crash Report Points to the Project Issue Tracker
The crash report written to `fasttail_crash.log` and the crash dialog SHALL direct the user to `https://github.com/matteobaccan/FastTail/issues`. The URL SHALL come from a single constant shared by the report and the dialog.

#### Scenario: Crash log footer
- **WHEN** the application panics and writes `fasttail_crash.log`
- **THEN** the report footer contains `https://github.com/matteobaccan/FastTail/issues` and no reference to `baccan/fasttail`.

#### Scenario: Crash dialog
- **WHEN** the crash dialog is shown after a panic
- **THEN** its text contains the same issues URL as the crash log.

### Requirement: Crash Report Content
The crash report SHALL include the application version, git commit hash, git tag or ref, UTC timestamp, OS and architecture, panic location, panic message and the captured backtrace, and SHALL be written next to the current working directory and the executable, falling back to the temp directory.

#### Scenario: Read-only install directory
- **WHEN** the application panics while running from a directory the user cannot write to
- **THEN** the report is written to the system temp directory and the dialog names that path.
