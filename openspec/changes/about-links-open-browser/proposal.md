## Why

The About dialog shows two links, `www.baccan.it` and `github.com/matteobaccan/FastTail`, drawn as hyperlinks that do nothing when clicked. The `cyber-ui-docking` spec already requires them to open the default browser, so this is a defect: `Cargo.toml` declares `eframe` with `default-features = false` and a feature list that omits `links`, so egui-winit is built without the `webbrowser` crate and every click ends in the log line `Cannot open url - feature "links" not enabled.`

## What Changes

- Enable the `links` feature of `eframe`, so egui's `OpenUrl` output is handed to the operating system's default browser on Windows, Linux and macOS.
- Give the two links a hover tooltip with the full URL, so the target is visible before clicking.
- Add a regression guard: a test that fails if the `eframe` dependency ever loses the `links` feature again.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: "Clickable External Hyperlinks in About Dialog" states the full-URL tooltip and the requirement that the build carries the browser-opening support.

## Impact

- `Cargo.toml`: `eframe` features gain `links`; `Cargo.lock` gains `webbrowser` and its platform dependencies (a few hundred KB of code, no new build tools).
- `src/ui/app.rs`: the three `hyperlink_to` calls of the About dialog gain `on_hover_text` with the URL.
- `tests/integration_tests.rs`: the regression guard reads `Cargo.toml` and asserts the feature list.
- `CHANGELOG.md` `[Unreleased]` under Fixed.
