## Why

A tail window is often kept small in a corner while working in another application. BareTail has "Always on top"; FastTail loses the window behind the IDE or browser.

## What Changes

- A pin toggle in the title bar and a Settings checkbox set the window level to always-on-top; the state is persisted in `fasttail.ini`.
- Ctrl+Shift+T toggles it from the keyboard.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: adds the always-on-top window level.

## Impact

- `src/ui/app.rs`: `ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop | Normal)` on toggle and at startup; title bar button.
- `src/config.rs`: `always_on_top: bool`; `src/i18n.rs`: keys.
