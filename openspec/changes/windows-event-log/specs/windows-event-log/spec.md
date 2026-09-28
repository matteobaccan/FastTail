## ADDED Requirements

### Requirement: Event Log Channels as Streams
On Windows the application SHALL open an Event Log channel (such as Application, System, Setup, a `Microsoft-Windows-*/Operational` channel or an application's own channel) as a stream: it SHALL show the newest `event_log_initial_events` events (default 10 000, 0 to 1 000 000) oldest first, then SHALL append each new event within 1 second of it being logged, with no event missing or shown twice between the history and the live part. Events SHALL be written into a temporary spool file tailed as a followed stream, bounded like the standard input stream (2048 MB and a 512 MB free-space margin, restart from empty with a notice), so no event text is held in memory beyond one batch of 64 events and the history buffer of at most 10 000 events. The open dialog SHALL list the enabled channels with a search box. The Event Log entry SHALL NOT be offered on Linux and macOS.

#### Scenario: Live Application channel
- **WHEN** the user opens the Application channel and a service then logs an error
- **THEN** the stream shows the newest 10 000 events oldest first and, within 1 second, a new line for the error at the bottom.

#### Scenario: No gap at the switch to live
- **WHEN** 200 events are logged to the channel while its history is being read
- **THEN** each of the 200 events appears exactly once, after the history.

### Requirement: Event Rendering and Level Mapping
Each event SHALL be rendered as one line `<time> <LEVEL> [<provider>] <event id>: <message>`, the time being the event's creation time in local time, ISO 8601 with milliseconds and the UTC offset, and further lines of the message SHALL follow as continuation lines indented by two spaces. The level word SHALL be `FATAL` for Critical, `ERROR` for Error, `WARN` for Warning, `INFO` for Information and LogAlways, and `DEBUG` for Verbose; an event of level LogAlways with the audit failure keyword SHALL be `WARN` and with the audit success keyword `INFO`. An event whose message cannot be formatted SHALL show its event data values instead. A message longer than 64 KB SHALL be cut with the long-line marker. Level detection, the level filter, timestamp detection, the time range and the histogram SHALL work on these lines without configuration.

#### Scenario: Service crash line
- **WHEN** Service Control Manager logs event 7031 at Error level
- **THEN** the line reads `2026-09-28T14:02:05.123+02:00 ERROR [Service Control Manager] 7031: The Print Spooler service terminated unexpectedly…`, is counted as ERROR and is kept by a `≥ ERROR` level filter.

#### Scenario: Multi-line message
- **WHEN** a .NET Runtime event carries a 30-line stack trace in its message
- **THEN** it is shown as one line followed by 29 indented continuation lines that stack-trace grouping keeps with it.

### Requirement: Event Log Pre-filter and Access
The Event Log dialog SHALL offer a pre-filter by levels and by up to 32 event IDs or ID ranges, applied by Windows to both the history and the live events, the stream's own filters applying on top. Opening a channel SHALL NOT require administrator rights when the user's account can read it, and SHALL NOT ask for elevation; a channel the user cannot read SHALL end with a message naming administrator rights or membership of Event Log Readers, and the dialog SHALL mark it as not readable.

#### Scenario: Only errors of one service
- **WHEN** the user opens System with levels Critical and Error and the IDs `7031, 7034`
- **THEN** only Critical and Error events with ID 7031 or 7034 are shown, in history and live.

#### Scenario: Security channel as a standard user
- **WHEN** a user without administrator rights and not in Event Log Readers opens Security
- **THEN** no elevation prompt appears and the stream says the channel needs administrator rights or Event Log Readers membership.

### Requirement: EVTX Files and Persistence
On Windows a file whose content starts with `ElfFile` followed by a zero byte SHALL open, whatever its name, as a non-followed event stream rendered as above, oldest event first. A channel stream SHALL be saved in the workspace, recent files and sessions as `eventlog://<channel>` with its pre-filter and restored by subscribing again; on Linux and macOS such an entry SHALL be reported as unavailable on this platform and kept in the session file. On Linux and macOS an EVTX file SHALL keep opening in HEX view with a notice that event log files are read on Windows only.

#### Scenario: Exported log from a customer
- **WHEN** the user opens `customer-system.evtx` (40 MB) on Windows
- **THEN** it opens as text lines with levels and timestamps, follow is disabled, and filters and the time range work on it.

#### Scenario: Session opened on Linux
- **WHEN** a session saved on Windows with `eventlog://Application` and a file stream is loaded on Linux
- **THEN** the file stream opens, the Event Log entry is listed as unavailable on this platform, and saving the session again keeps it.
