## Why

LogFusion reads the Event Log of other Windows machines; FastTail's `eventlog://` reads
the local one.

## What Changes

- `eventlog://host/Channel` follows a remote machine's channel through the Windows Event Log
  API's remote session, with the current user's Windows credentials (no password is asked
  or stored; other credentials are left for later).
- The open dialogs take a host; errors (access denied, RPC unavailable) are shown in the
  stream bar.

Target release: **0.33.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low–Med**. Effort: **M**.

### Non-goals

- Event forwarding subscriptions (WEF).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `windows-event-log`: the requirement below is added.

## Impact

`src/eventlog.rs` remote sessions (Windows only); dialogs; texts; docs.
