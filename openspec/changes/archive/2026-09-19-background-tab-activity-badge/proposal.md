## Why

With several streams docked as tabs, only the visible one shows that lines are arriving. SnakeTail highlights a tab when its file changes; FastTail offers only the audio alert, which is off by default and tied to highlight rules.

## What Changes

- A tab whose stream received lines while not visible shows a badge with the number of unseen lines (capped at `999+`), coloured by the highest level or highlight seen among them (rule sound preset or, once `log-level-detection` lands, level).
- The badge clears when the tab becomes visible.
- Optional: the taskbar/dock icon requests attention when a rule with a sound alert matched in a background tab and the window is not focused (`eframe` `RequestUserAttention`).

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: adds background activity badges and window attention requests.

## Impact

- `src/tail_engine.rs`: `unseen_lines: usize` and `unseen_severity` maintained on append, `mark_seen()`.
- `src/ui/dock.rs`: tab title rendering with the badge via `TabViewer::title`; call `mark_seen` when the stream is drawn.
- `src/ui/app.rs`: `ViewportCommand::RequestUserAttention` when appropriate; Settings toggle; `src/i18n.rs`: keys.
