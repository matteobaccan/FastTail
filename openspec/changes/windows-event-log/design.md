## Context

Non-file sources become streams through a spool: the stdin copier (`src/stdin_source.rs`)
writes chunks into a `SpoolFile` (`src/spool.rs`) and wakes the engine, which tails the
spool under a pseudo-path identity. Level detection (`src/log_level.rs`) reads level words
such as `ERROR` and `WARN` from the line, timestamp detection (`src/timestamp.rs`) reads
ISO 8601 with offsets, and multiline grouping keeps indented continuation lines with
their parent. On Windows the only platform crate today is `winreg`; `windows-sys` is in the
tree through `winit` and `wgpu`. The Windows Event Log API (`wevtapi.dll`) lets a standard
user query and subscribe to every channel whose ACL grants read to Users or
Authenticated Users; Security needs admin or the Event Log Readers group.

## Goals / Non-Goals

**Goals:** live channels and `.evtx` files as ordinary text streams, no admin rights for
readable channels, correct levels and timestamps for the existing detectors, bounded
memory. **Non-goals:** see proposal.

## Decisions

### D1. `wevtapi` through `windows-sys`, rendered to text lines

A worker thread per stream uses `EvtQuery` (history) and `EvtSubscribe` (live) and renders
each event itself: `EvtRender(EvtRenderEventValues)` with a system render context for
`TimeCreated`, `Level`, `Keywords`, `EventID`, `Provider/@Name`, `EventRecordID`, then
`EvtFormatMessage(EvtFormatMessageEvent)` with a publisher-metadata handle for the message.

Rejected: running `wevtutil qe` or PowerShell `Get-WinEvent` as a child process (a
console-window flash from a GUI binary, polling instead of push, seconds per call,
locale-dependent text to parse); the `windows` crate with WinRT projections (much larger
compile, no benefit for a C API); the pure-Rust `evtx` crate for live channels (it parses
files only, and cannot format messages, which live in each provider's resource DLL).

### D2. History, then live, without gap or duplicate

1. `EvtQuery(channel, xpath, EvtQueryChannelPath | EvtQueryReverseDirection)` reads up to
   `event_log_initial_events` events newest first into a bounded buffer (10 000 events ×
   ~300 bytes ≈ 3 MB; at the 1 000 000 maximum they are rendered in batches of 10 000 to
   a temporary file and reversed from there, never all held in memory);
2. an `EvtCreateBookmark` on the newest one;
3. the buffer is written oldest first into the spool;
4. `EvtSubscribe(…, EvtSubscribeStartAfterBookmark)` with a signal event and `EvtNext`
   pulls in batches of 64: events that arrived during steps 1–3 are delivered once.

With `event_log_initial_events = 0` only step 4 runs, from `EvtSubscribeToFutureEvents`.
The spool uses the shared spool feed with a 2048 MB cap and the 512 MB free-space margin
(restart from empty, notice), like stdin.

### D3. Line format and level mapping

`<TimeCreated local ISO 8601 ms+offset> <LEVEL> [<Provider>] <EventID>: <message>`. The
message's line breaks become continuation lines indented by two spaces, and a message
longer than 64 KB is cut with the engine's truncation marker. Level mapping: 1 → `FATAL`,
2 → `ERROR`, 3 → `WARN`, 0 and 4 → `INFO`, 5 → `DEBUG`; when `Level` is 0 and `Keywords`
holds audit failure (`0x0010000000000000`) → `WARN`, audit success (`0x0020000000000000`)
→ `INFO`. A message that cannot be formatted (provider not installed, resource missing)
is replaced by `(no message; data: <EventData values joined by "; ">)`, as Event Viewer
does. Publisher-metadata handles are cached, at most 256 providers per stream (LRU).

### D4. Pre-filter

The dialog's level checkboxes and event-ID list (up to 32 IDs or ranges) build an XPath
query such as `*[System[(Level=1 or Level=2) and (EventID=7031 or EventID=7034)]]`, passed
to `EvtQuery` and `EvtSubscribe`; the stream's own filters still apply on top. The
pre-filter is part of the stream identity in sessions.

### D5. Access and channel list

`EvtOpenChannelEnum` lists channels; for each shown row the dialog calls
`EvtOpenChannelConfig` + `EvtGetChannelConfigProperty(Enabled)` lazily (only visible rows)
to hide disabled channels. Readability is known only by trying: opening a stream on an
unreadable channel ends at once with `ERROR_ACCESS_DENIED`, and the stream shows "needs
administrator rights or membership of Event Log Readers" and the dialog marks the
channel. FastTail never requests elevation.

### D6. `.evtx` files, identity, threads and memory

A file starting with `ElfFile\0` opens as an event stream on Windows via
`EvtQuery(path, "*", EvtQueryFilePath)`, oldest first, not followed (it is a snapshot);
its identity is its path, as a compressed file's is. A channel's identity is
`eventlog://<channel>` with `/` percent-encoded, plus `?levels=…&ids=…` for the
pre-filter. UI thread: the dialog and state atomics only. Worker: one thread per stream
(query, then subscription wait with a 500 ms timeout to observe closing). Memory per
stream: the render buffers (64 KB), the metadata cache, the history buffer during start
(≤ 3 MB by default); the content lives in the spool, indexed at 8 bytes per line.
Channels are never "rotated" or truncated from FastTail's view; a cleared channel stops
delivering and the stream keeps what it had, with a notice when `EvtNext` reports the
subscription became invalid (resubscribed after 5 s).

## Risks / Trade-offs

- [Message formatting is slow for some providers (tens of ms)] → formatting on the worker,
  batches of 64, metadata cache; history of 10 000 events renders in a few seconds, with
  progress in the stream bar.
- [Local-time timestamps across a DST change] → the offset is written on every line, so
  timestamp detection and the time range stay exact.
- [Very chatty channels (thousands of events per second)] → pre-filter; the spool cap
  bounds disk use.

## Open Questions

1. Read `.evtx` on Linux and macOS with the pure-Rust `evtx` crate (MIT/Apache-2.0),
   showing event data without the formatted message — worth a phase 2?
2. Should Windows' own "Custom Views" (saved XPath queries) be offered in the dialog?
