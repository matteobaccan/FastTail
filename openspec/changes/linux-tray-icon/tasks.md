## 1. Backend

- [ ] 1.1 `ksni` for `target_os = "linux"` in `Cargo.toml`; `src/tray.rs` `linux` backend: icon, tooltip, menu, left click, events channel
- [ ] 1.2 `Tray::available()` on Linux: StatusNotifierWatcher owner on the session bus, checked once
- [ ] 1.3 Badge pixmaps (32 and 16 px, ARGB32) from the shared badge rendering; 250 ms wake-up while hidden

## 2. Tests and documentation

- [ ] 2.1 Unit tests for the pixmap conversion; availability and menu checked by hand on KDE and GNOME with AppIndicator
- [ ] 2.2 README (Linux tray note), CHANGELOG `[Unreleased]`

## 3. Wrap-up

- [ ] 3.1 `cargo fmt`, `cargo clippy --all-targets` (also `--target x86_64-unknown-linux-gnu`), focused tests; PR with Linux and Windows CI green
- [ ] 3.2 After the release, archive the change
