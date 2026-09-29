## Why

On Windows, services, drivers, IIS, SQL Server, .NET runtime crashes and PowerShell write
to the Event Log, not to text files. Today FastTail cannot show it: the user opens Event
Viewer, which does not tail, has no highlight rules and filters through a modal dialog,
or exports to `.evtx` and gets a binary blob in FastTail's HEX view. SnakeTail tails the
Event Log without admin rights, LogFusion reads it (remote machines too) and LogViewPlus
opens EVTX; the post-0.12.0 competitor scan ranks it gap 12 (value medium). For a viewer
whose first users came from BareTail on Windows, "the log is in the Event Viewer" is a
common dead end.

## What Changes

- **Event Log channels as streams** (Windows only): a new **Event Log…** entry in the open
  menu lists the channels the user can read (Application, System, Setup, Windows
  PowerShell, `Microsoft-Windows-*/Operational`, custom application channels), with a
  search box; choosing one opens a live stream of its events.
- Each event is **one text line** (continuation lines indented) with an ISO 8601
  timestamp in local time with offset, a level word FastTail's level detection already
  knows, the provider, the event ID and the formatted message:
  `2026-09-28T14:02:05.123+02:00 ERROR [Service Control Manager] 7031: The Print Spooler
  service terminated unexpectedly.` Levels map Critical → `FATAL`, Error → `ERROR`,
  Warning → `WARN`, Information and LogAlways → `INFO`, Verbose → `DEBUG`; audit failure
  → `WARN`, audit success → `INFO`. Filters, rules, level filter, time range, histogram,
  bookmarks and export all work.
- **History and live**: the newest `event_log_initial_events` events (default 10 000,
  0–1 000 000) are shown oldest first, then new events arrive live (under 1 s).
- **Pre-filter** in the dialog: levels and event IDs, turned into the channel query so a
  chatty channel is filtered by Windows before it reaches FastTail.
- **No administrator rights** for the channels a standard user can read; a channel the
  user cannot read (Security without admin or Event Log Readers membership) is shown
  greyed out with the reason, never with an elevation prompt.
- **`.evtx` files** open as a (non-followed) event stream, recognised by content
  (`ElfFile\0`), instead of HEX.
- Sessions, workspace and recent files keep `eventlog://<channel>` with the pre-filter; a
  new `fasttail.ini` key `event_log_initial_events`.

Target release: **0.22.0** (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **medium**. Effort: **M (about 2 weeks)**.

### Non-goals

- Remote computers (`EvtOpenSession`), forwarded-event subscriptions, and writing,
  clearing or exporting the Event Log.
- Reading `.evtx` on Linux and macOS (open question 1); there the file keeps opening in
  HEX with a notice.
- Structured columns of the event's `EventData`: the rendered line is text; the
  `structured-fields` change may add fields later.
- Elevation (UAC) to read the Security channel.

## Capabilities

### New Capabilities

- `windows-event-log`: Event Log channels and `.evtx` files as streams, their rendering,
  level mapping, pre-filter, access rules and persistence.

### Modified Capabilities

_None._

## Impact

- New `src/event_log.rs` (Windows only, `#[cfg(windows)]`): channel enumeration, query and
  subscription worker, renderer, publisher-metadata cache; writes into a spool through the
  shared spool feed (`spool_feed`, split out of `stdin_source` by the first source change
  that lands).
- `Cargo.toml`: `windows-sys` (already in the dependency tree through `winit` / `wgpu`)
  as a direct Windows-only dependency with the `Win32_System_EventLog` and
  `Win32_Foundation` features; no C code.
- `src/compressed.rs` `sniff` (or the open dispatch in `src/ui/app.rs`) recognises the
  EVTX magic; `src/session.rs` / workspace / recent files store `eventlog://` identities;
  `src/ui/app.rs` the Event Log dialog; `src/config.rs` the new key.
- `src/i18n.rs`: new strings in all 16 languages; README and CHANGELOG.
