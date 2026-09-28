## Context

Highlight rules live in `config.highlight_rules` and are compiled per engine into
`CompiledHighlight` (`sound_alert`, `auto_bookmark`, ...). On new lines the engine calls
`check_sound_alerts(prev_lines_count)` (first match plays, 250 ms throttle) and
`collect_tool_hits(start_idx)`, which queues `(pattern, line)` in `pending_tool_hits`
(capped) for the app to run bound tools through `external_tools::Runner` (1 run per
second per tool, 10 children). `unseen_severity` drives the `[N]` badge; `app.rs` sends
`RequestUserAttention` once while the window is unfocused when `flash_on_alert` is on.

## Goals / Non-Goals

**Goals:** a readable, actionable notification with the line; never a flood; never a
notification for old lines; no UI stall.

**Non-Goals:** remote alerts, thresholds, tray icon.

## Decisions

### D1. Hits collected like tool hits
`collect_notify_hits(start_idx)` runs on the same appended ranges as `collect_tool_hits`
and only for rules with `notify` and `enabled`. It records `(rule index, line index,
line text cut at 200 characters)` in a queue capped at 64 entries per engine; beyond, it
only counts. Initial indexing, reloads after truncation or rotation, re-decodes and
restored sessions never call it, so opening a 2 GB file full of `ERROR` shows nothing.

### D2. Throttle in the app, not the engine
`notifications.rs` keeps per rule the time of the last notification and a pending count.
A hit within 10 s of the last one for that rule adds to the count; the next notification
of that rule says `+N more`. When the 10 s pass with a pending count and no new hit, a
summary notification (`N more matches of <rule>`) is sent. A global limit of 3 per 10 s
drops the rest into the same counters. Several streams matching the same rule share the
rule's budget, and the title names the stream of the line shown.

### D3. Visibility
`notify_when = background` (default): a hit notifies when the window is unfocused or
minimised, or when the engine is not `displayed`. Otherwise the user is looking at the
line already. `always` notifies regardless.

### D4. Back end
`notify-rust` on a dedicated thread fed by a channel. Linux: freedesktop notifications
with a default action; the action callback sends `(stream id, line)` back to the app.
Windows: toast through the registered AppUserModelID `FastTail` (display name and icon
under `HKCU\Software\Classes\AppUserModelId\FastTail`, written at first use); activation
of an unpackaged app's toast needs a COM activator, so in v1 a click on Windows opens
nothing unless the platform crate reports it (see open questions). macOS: Notification
Center, click brings the app to the front. If creating the back end fails (no D-Bus
session, notification service missing), the Notify checkbox and the settings show the
reason and nothing is retried per hit.

### D5. Click action
A click event carries a stream id and a line index; the app focuses the window
(`ViewportCommand::Focus`), activates the stream's tab and scrolls to the line if the
stream is still open and the line still exists (a truncated file may not have it), else
it only focuses.

## Risks / Trade-offs

- [Line text leaks into the OS notification history] → documented; notifications are
  opt-in per rule.
- [Windows click not handled without a COM activator] → accepted for v1; the toast still
  informs.
- [A noisy rule] → per-rule and global throttles, summary with counts.

## Open Questions

- Implement the Windows COM activator so a toast click jumps to the line? Proposed: only
  if `notify-rust` / `tauri-winrt-notification` exposes activation for unpackaged apps
  without a Start-menu shortcut; otherwise defer.
- Should `notify_when = background` also count a stream shown in a floating window that is
  covered by another application? Proposed: no, the window-focus check is enough.
