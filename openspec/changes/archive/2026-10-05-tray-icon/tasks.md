## 1. Platform tray

- [x] 1.1 Spike: does eframe 0.36 run `update` for a hidden viewport on Windows and Linux (X11, Wayland) when repaints are requested from another thread; choose D1 or its fallback
- [x] 1.2 `src/tray.rs` trait; Windows backend (`Shell_NotifyIconW`, message-only window thread, popup menu, events channel)
- [x] 1.3 Linux backend with `ksni`: moved to the change `linux-tray-icon` (2026-10-05)
- [x] 1.4 Badge rendering (dot colour and count) from the app icon; tooltip text

## 2. Window behaviour

- [x] 2.1 Minimise to tray and close to tray (cancel close, hide, save workspace); Quit from the tray and CTRL + Q
- [x] 2.2 Tray menu: Show / Hide, Follow all / Pause all, Mute sounds, open streams, Quit; left click and double click
- [x] 2.3 Alert counting next to the attention request; reset when shown and focused
- [x] 2.4 Wake-up while hidden so engines keep polling; painting skipped
- [x] 2.5 Settings switches and `tray_icon`, `minimize_to_tray`, `close_to_tray` in `[general]` (first-time hint not done)
- [x] 2.6 Tests: config round trip; icon scaling and badge; availability (alert counting and menu checked by hand)

## 3. Texts and documentation

- [x] 3.1 New i18n keys (settings, menu, tooltip, hint, no-tray message) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README (feature list, Linux tray note) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the 0.13.0 release
- [x] 4.3 After the release, archive the change so `cyber-ui-docking` gains the new requirements
