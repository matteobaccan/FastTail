## Why

FastTail is often left running all day to watch a few logs. Minimised, it still takes a
taskbar slot; closed, it stops watching. Users of SnakeTail (and of many Windows
monitoring tools) expect a notification-area icon: hide the window there, keep tailing,
and see at a glance that an alert fired. FastTail already flashes the taskbar on
background alerts ("Window Attention on Background Alerts"), but that needs a taskbar
button. The post-0.12.0 competitor scan lists the tray icon among the small gaps (row 20).

## What Changes

- A **tray icon** (Windows notification area; Linux StatusNotifierItem), shown while the
  option "Show tray icon" is on (default off).
- **Minimise to tray** (option, default off): minimising hides the window and its taskbar
  button; **close to tray** (option, default off): the close button hides instead of
  quitting, and "Quit" in the tray menu or CTRL + Q (a new shortcut, unused in 0.12.0,
  active whatever the tray options) quits.
- **Tray menu**: Show / Hide FastTail, Follow all / Pause all, Mute sounds, the open
  streams (clicking one shows the window with that stream focused), Quit. Left click
  toggles the window; double click shows it.
- **Alert badge**: while the window is hidden or unfocused, lines matching a highlight
  rule with a sound preset (the trigger of the window attention request) are counted; the
  icon gets a dot in the colour of the most severe preset and the count (up to `99+`),
  and the tooltip says `FastTail — 3 streams · 12 alerts (app.log)`. Showing the window
  clears it.
- Tailing, rules, sounds and automatic bookmarks keep running while hidden; rendering
  stops.
- **Platforms**: Windows first. Linux through the StatusNotifierItem D-Bus protocol (KDE,
  XFCE, Cinnamon, GNOME with the AppIndicator extension, default on Ubuntu); when no
  watcher is on the bus, the options are disabled with an explanation. macOS: not in this
  change.
- New `fasttail.ini` keys in `[general]`: `tray_icon`, `minimize_to_tray`,
  `close_to_tray` (all default `false`).

Target release: **0.15.0** for the Linux backend (task 1.3); the Windows tray shipped in 0.14.0 (planned for 0.13.0, moved twice), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S–M**.

### Non-goals

- Desktop notifications (toasts) for rule matches: that is `rule-notifications`; the
  tray only shows the badge and forwards clicks.
- A macOS menu-bar item (no macOS test machine; possible follow-up).
- Starting FastTail with the operating system, or starting hidden (a later option).
- Taskbar overlay icons or jump lists.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cyber-ui-docking`: new requirements System Tray Icon, Hide to Tray and Tray Alert
  Badge.

## Impact

- `src/tray.rs` (new): platform tray behind a small trait; Windows via
  `Shell_NotifyIconW` with a hidden message window on its own thread (`windows-sys`),
  Linux via the `ksni` crate (pure Rust D-Bus, no GTK); the badge icon drawn at runtime
  into RGBA from the app icon.
- `src/ui/app.rs`: close and minimise handling (`close_requested` cancelled with
  `ViewportCommand::CancelClose` when closing to tray), `ViewportCommand::Visible`,
  alert counting next to the existing attention request, tray events drained each frame,
  a periodic wake-up while hidden so engines keep polling.
- `src/config.rs`: three keys; Settings page switches.
- `Cargo.toml`: `windows-sys` (Windows target), `ksni` (Linux target).
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: not applicable; a terminal has no tray.
