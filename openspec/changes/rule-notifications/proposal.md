## Why

FastTail can already react to a highlight rule: a sound preset (`SoundAlertPreset`,
throttled to one per 250 ms), a taskbar flash when the window is in the background
(`flash_on_alert`), the `[N]` badge in the critical colour, automatic bookmarks and an
external tool bound to the rule. None of them tells the user **what** happened while
they are in another application: a sound is easy to miss with headphones off, the flash
only says "something", and a bound `notify-send` tool works on Linux only and needs to be
configured by hand (it is the cookbook's example). LogExpert has triggers, LogFusion and
LogViewPlus alerts, and monitoring dashboards (gonzo) have alerts; the competitor scan
lists alerts as table stakes for a live tailer. A native desktop notification per rule,
with the matching line in it, closes the gap with a checkbox.

## What Changes

- A new **Notify** option per highlight rule, next to the sound preset: when an enabled
  rule with Notify matches an **appended** line (never the lines read when a file is
  opened, re-read or restored), FastTail shows a **desktop notification** titled
  `FastTail · <stream title>` whose body is the matching line (ANSI stripped, cut at 200
  characters) and the rule pattern.
- **Throttle and grouping**: at most one notification per rule per 10 seconds; matches in
  between are counted and the next notification says `+N more`. At most 3 notifications
  per 10 seconds overall.
- **When**: by default only when the matching stream is not visible to the user (window
  unfocused or minimised, or the stream's tab not displayed); a setting
  `notify_when = background | always` changes it. A global switch
  `notifications_enabled` (default on) silences all of them, and a "Test notification"
  button in Settings shows one.
- **Clicking** the notification, where the platform reports it (Linux, Windows), brings
  the window to the front, selects the stream and scrolls to the line.
- Platforms: Windows 10/11 toast, Linux freedesktop notifications (D-Bus), macOS
  Notification Center. When the platform has no notification service, the option is
  disabled with the reason.
- `fasttail.ini`: `notify` per `[highlight_N]` section (default `false`), and
  `notifications_enabled`, `notify_when` in `[general]`.

Target release: **0.23.0** (re-planned by the maintainer on 2026-10-06, from 0.22.0: about one large, two medium and three small changes per release) (planned for 0.15.0, moved after the terminal interface of 0.20.0; sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **S**.

### Non-goals

- E-mail, webhook, Slack or other remote alerts (an external tool bound to the rule
  already covers them).
- Threshold rules ("more than 10 errors per minute"); a possible follow-up with
  `field-statistics`.
- Custom notification sounds (the rule's sound preset plays as today, independently).
- A tray icon.

## Capabilities

### New Capabilities

- `rule-notifications`: the per-rule Notify option, when notifications are shown, their
  content, throttling, click action and settings.

### Modified Capabilities

None. The rule's sound alert (`filters-and-highlighting`, `telemetry-audio-fx`) and the
window attention request (`cyber-ui-docking`) keep working as specified.

## Impact

- `Cargo.toml`: `notify-rust` (pure Rust on Linux through `zbus`; WinRT toast on Windows;
  `mac-notification-sys` on macOS). No C toolchain on Linux targets.
- `src/tail_engine.rs`: `notify` field on `HighlightRule` and `CompiledHighlight`;
  `collect_notify_hits(start_idx)` next to `collect_tool_hits`, bounded queue, called
  only for appended lines.
- New `src/notifications.rs`: throttle and grouping per rule, global rate limit, platform
  back end on a worker thread (showing a notification never blocks the UI), click
  events back to the app.
- `src/ui/app.rs`: drain of the queue each frame (next to the tool hits and
  `flash_on_alert`), visibility check, click handling (focus, select stream, jump to
  line), Settings entries and test button; `src/ui/dock.rs`: Notify checkbox in the rule
  editor next to the sound preset (line ~5023).
- `src/config.rs`: `notify` in rule sections; `notifications_enabled`, `notify_when`.
- Windows: an AppUserModelID `FastTail` registered under
  `HKCU\Software\Classes\AppUserModelId` (with `winreg`, already a dependency) so toasts
  show FastTail's name and icon.
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: the throttle and the hit collection are UI-free; the TUI can
  use the same back end, or an OSC 9 / OSC 777 terminal notification when running over
  SSH.
