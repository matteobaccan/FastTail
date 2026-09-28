## ADDED Requirements

### Requirement: Time Window Options
Without `--print`, the executable SHALL accept `--since <TIME>` and `--until <TIME>`, which SHALL set the "from" and "to" sides of the time window of the streams opened from the command line, the standard-input stream included, exactly as if typed in the time range popup, relative times included (a relative value giving a live window). A value that cannot be read SHALL print usage to stderr and exit with code 2 before any window opens.

#### Scenario: The last three hours
- **WHEN** the user runs `fasttail --since -3h app.log`
- **THEN** `app.log` opens with the live window from `-3h`, showing the lines of the last three hours.

#### Scenario: Unreadable time
- **WHEN** the user runs `fasttail --since yesterday app.log`
- **THEN** usage is printed to stderr and the process exits with code 2.
