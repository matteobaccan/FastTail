## Why

The tray icon (`tray-icon`, shipped in 0.14.0) works on Windows only: on Linux the
"Show tray icon", "Minimise to tray" and "Close to tray" options are greyed out. Linux
users who keep FastTail open all day (KDE, XFCE, Cinnamon, GNOME with the AppIndicator
extension, the default on Ubuntu) lose the hidden-but-tailing workflow and the alert
badge. This was task 1.3 of `tray-icon`, split out when that change was archived.

## What Changes

- A **Linux tray backend** over the StatusNotifierItem D-Bus protocol, with the `ksni`
  crate (pure Rust, no GTK, no libappindicator at runtime).
- At start FastTail checks for `org.kde.StatusNotifierWatcher` on the session bus; with
  it, the tray options are enabled and behave as on Windows (menu, left click, badge,
  tooltip, minimise and close to tray); without it they stay disabled with "No system
  tray available on this desktop". X11 and Wayland behave the same.
- The badge icon is rendered as on Windows and sent as ARGB pixmaps (32 and 16 px).
- No new `fasttail.ini` key.

Target release: **0.21.0** (the parts of earlier changes left open, per
`docs/competitor-analysis.md` section 8). Priority: **Low**. Effort: **S**.

### Non-goals

- macOS (no tray in this change).
- Desktop notifications (`rule-notifications`).
- A tray for the terminal interface.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cyber-ui-docking`: System Tray Icon gains Linux with a StatusNotifierItem host.

## Impact

- `src/tray.rs`: a `linux` backend next to `win` (`Tray::start`, `available`, menu
  updates, events), the watcher check.
- `Cargo.toml`: `ksni` for `target_os = "linux"` (new dependency, pure Rust D-Bus).
- Settings: the tray options follow `Tray::available()` (already the case).
- README (feature list, Linux tray note), CHANGELOG.
- Build: Linux x86_64 and ARM64 release jobs compile `ksni`; no new runtime library.
