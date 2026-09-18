## Why

Stack traces, tickets and colleagues refer to log lines by number, and FastTail already shows line numbers, yet the only way to reach line 1,234,567 is scrolling. BareTail and klogg have "go to line"; it is a small feature with daily use.

## What Changes

- Ctrl+G opens a small go-to popup in the focused stream; typing a number and pressing Enter scrolls that line to the middle of the viewport and pauses follow mode.
- Numbers beyond the last line clamp to the last line; under an active filter the nearest visible line at or after the target is used.
- Optional `+N` / `-N` relative form jumps from the current top line.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `search-and-navigation`: adds go-to-line.

## Impact

- `src/tail_engine.rs`: `goto_line(target) -> usize` resolving clamping and filter visibility, setting `requested_scroll_y` through the existing jump path.
- `src/ui/dock.rs`: Ctrl+G popup anchored to the stream bar, Enter/Esc handling; `src/i18n.rs`: keys.
- `tests/integration_tests.rs`: clamping, filtered resolution, relative jumps.
