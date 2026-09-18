## Context

egui widgets never open URLs themselves: `ui.hyperlink_to` pushes an `OutputCommand::OpenUrl` and the platform integration acts on it. In egui-winit 0.36 that handler is `open_url_in_browser`, compiled to a call to `webbrowser::open` only under the `webbrowser` feature, which eframe exposes as `links` and enables by default. FastTail sets `default-features = false` on eframe (to drop accesskit and pick the renderers explicitly) and lists `default_fonts`, `glow`, `wgpu`, `x11`, `wayland`, so `links` was lost and the handler became a `log::warn!` nobody sees.

## Goals / Non-Goals

**Goals:** the About links open the default browser on the three desktop platforms; the fix cannot regress silently.

**Non-Goals:** intercepting `OpenUrl` in the app to show an error notice (egui-winit handles the command before the app can observe the failure; not worth a direct `webbrowser` dependency for two links), links elsewhere in the UI, opening URLs found inside log lines (belongs to `external-tools`).

## Decisions

- **Add `links` to the eframe feature list** rather than re-enabling default features: the explicit list is what keeps accesskit and other unused features out of the build, and `links` is the only missing piece. Alternative considered: depending on `webbrowser` directly and draining `OpenUrl` commands in `update`; rejected because it duplicates what egui-winit already does and adds code to maintain.
- **Regression guard as a test on `Cargo.toml`**: the crate cannot see egui-winit's features at compile time, so the integration test parses the `eframe` line of `Cargo.toml` (`include_str!`) and asserts that `"links"` is in its feature list. Alternative considered: a `build.rs` check; rejected as heavier than a test.
- **Tooltip with the full URL** on each link, using `on_hover_text`, so the user sees the target and the link is not a bare styled label.

## Risks / Trade-offs

- [`webbrowser` adds platform dependencies] → on Windows it calls `ShellExecuteW`, on Linux `xdg-open`, on macOS `open`; all are already present on the target machines and CI builds on all three.
- [A machine without a default browser] → the click logs a warning and nothing else happens, the same as today; documented as an accepted limitation.
