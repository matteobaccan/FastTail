## Context

`src/tray.rs` defines `Tray` (events channel, `start`, `available`, menu and badge
updates) with a Windows backend (`Shell_NotifyIconW` on a message-only window thread);
on other systems `start` returns `None` and `available()` is false, so Settings greys the
options out. The window keeps polling while hidden through the tray thread's 250 ms timer
(issue #161 explains why minimising is not used).

## Goals / Non-Goals

**Goals:** the Windows behaviour on Linux desktops with a StatusNotifierItem host, no
runtime library beyond D-Bus.

**Non-Goals:** macOS, notifications, a tray for the terminal interface.

## Decisions

### D1. `ksni`
`ksni` implements StatusNotifierItem and its DBusMenu in pure Rust on its own thread; the
icon is given as ARGB32 pixmaps. *Alternative:* the `tray-icon` crate — its Linux backend
needs GTK and libappindicator at runtime, which the release binaries do not link.

### D2. Availability
`Tray::available()` on Linux asks the session bus whether
`org.kde.StatusNotifierWatcher` has an owner, once at start (cached). No bus or no owner:
false, and the options stay disabled with the existing explanation.

### D3. Wake-up while hidden
The `ksni` thread has no timer: a small thread requests a repaint every 250 ms while the
window is hidden, as the Windows backend's `SetTimer` does.

## Risks / Trade-offs

- [GNOME without the AppIndicator extension has no watcher] → the options say why.
- [Pixmap scaling differs between hosts] → both 16 and 32 px are sent; the host picks.
