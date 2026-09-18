## Why

Long lines (JSON payloads, stack traces, URLs) run off the right edge and force horizontal scrolling. BareTail offers line wrap; FastTail renders every row with `TextWrapMode::Extend` and no alternative.

## What Changes

- A per-stream "Wrap" toggle in the stream bar (Alt+W) switches rows between single-line extended rendering and soft wrapping at the viewport width.
- In wrap mode row heights vary; the virtualised renderer measures wrapped heights for the visible rows only and keeps scrolling by line index accurate.
- Wrap state is persisted per stream in the workspace.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: adds the wrap mode to the virtualised stream view.

## Impact

- `src/ui/dock.rs`: wrap toggle, variable-height row layout in `show_rows` (egui `ScrollArea::show_viewport` with a height cache for visible rows), text layout with `TextWrapMode::Wrap`.
- `src/tail_engine.rs`: `wrap_lines: bool` per stream; `src/config.rs`: persisted in the stream entry; `src/i18n.rs`: keys.
