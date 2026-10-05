## Context

`FastTailApp::update` drives everything: `TailEngine::poll_updates`, rule matching,
sounds, the badge of background tabs and the window attention request
(`ViewportCommand::RequestUserAttention`, `flash_on_alert`). Frames are scheduled with
`request_repaint_after`. The window can be minimised and closed through viewport
commands; `window_minimized` is saved. eframe 0.36 on winit; release targets are Windows
x64 and Linux x64 / ARM64 (macOS built, not tested).

## Goals / Non-Goals

**Goals:** keep FastTail watching without a taskbar button; see alerts on the icon;
no new behaviour unless the user turns it on.

**Non-Goals:** toasts, macOS, autostart.

## Decisions

### D1. Update loop while hidden
Engine polling lives in `update`. On Windows a window hidden with
`ViewportCommand::Visible(false)` may stop receiving redraws, so `update` would stop.
The tray thread holds a clone of `egui::Context` and calls `request_repaint()` every
`poll_interval_ms` while the window is hidden; a spike (task 1.1) confirms eframe 0.36
runs `update` for a hidden viewport. If it does not, the fallback is to minimise and
remove the taskbar button (`WS_EX_TOOLWINDOW` toggled through the raw window handle),
which keeps `update` running. Painting is skipped while hidden.

### D2. Windows backend without a GUI toolkit
`Shell_NotifyIconW` on a dedicated thread with a message-only window; the menu is a
native popup menu (`TrackPopupMenu`); events cross to the UI thread through a channel
and a repaint request. `windows-sys` is small and feature-gated. *Alternative:* the
`tray-icon` crate — its Linux backend needs GTK and libappindicator at runtime, which the
release binaries do not link today.

### D3. Linux backend
`ksni` implements StatusNotifierItem over D-Bus in pure Rust. At start FastTail checks
for `org.kde.StatusNotifierWatcher` on the session bus; without it the tray options are
disabled with "No system tray available on this desktop". Wayland and X11 behave the
same (the protocol does not involve the window system).

### D4. Badge
The icon is re-rendered (32×32 and 16×16) from the app icon with a coloured dot and the
count when the count or the colour changes, at most twice a second. Colours come from
the sound preset severity used by the background tab badge. The count resets when the
window is shown and focused.

### D5. Close to tray and quitting
With close to tray on, `close_requested` is cancelled and the window hidden; the
workspace is saved as on a real close so a crash later loses nothing. The tray menu Quit
and CTRL + Q quit for real. The first time the window goes to the tray, a one-line hint
says where it went.

### As implemented (0.14.0)
- **Scope:** Windows only (maintainer's choice); on other systems the options are greyed
  out. The Linux `ksni` backend (D3) moved to the change `linux-tray-icon` when this one
  was archived (2026-10-05).
- **Spike (D1)**, run on Windows 11 with eframe 0.36: a window hidden with
  `Visible(false)` keeps running `update` when another thread requests repaints (~4
  frames/s at 250 ms); a *minimised* window gets no frame at all, even with those
  requests (issue #161). Hence hide-to-tray uses `Visible(false)` and the tray thread's
  `SetTimer` wakes the app every 250 ms while hidden; no `WS_EX_TOOLWINDOW` fallback.
- **FFI (D2):** raw `extern "system"` declarations like the rest of the project, no
  `windows-sys` dependency.
- **Badge (D4):** a red dot on the icon and the count in the tooltip (no digits drawn on
  the 32 × 32 icon); one colour, not per preset.
- **Minimise to tray:** FastTail's own minimise button; the system one is handled on the
  frame that reports the window minimised (best effort, as such frames may not come).
- **First-time hint (D5):** not done.

## Risks / Trade-offs

- [Hidden window stops `update` on some platform] → D1 spike and fallback.
- [GNOME without the AppIndicator extension] → D3 detection and disabled options.
- [Users lose the window] → options off by default; first-time hint; clicking the tray
  icon always shows it.

## Migration Plan

None: all keys default to off.

## Open Questions

- Should the tray menu offer "Start with the system"? Proposed: no, a separate change.
- Should the badge also count lines of any `ERROR` level, not only rules with a sound
  preset? Proposed: no, reuse the attention trigger so there is one definition of alert.
