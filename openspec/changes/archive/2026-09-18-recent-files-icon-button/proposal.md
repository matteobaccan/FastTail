## Why

The title bar shows `📁 Open File` first and `🕒 Recent Files` only after Filter, Play, Pause and Settings, so the two ways of opening a log sit at opposite ends of the bar and the recent list costs the widest button of the row. Opening a file and reopening a recent one are the same gesture and belong together.

## What Changes

- The Recent Files dropdown becomes an icon-only button (`🕒`) with the localized "Recent Files" text as its tooltip.
- The button moves immediately to the right of `📁 Open File`, before the Filter button, keeping the same height, fill and border style as its neighbours.
- The dropdown content (MRU list, clear entry, empty-list message) is unchanged.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `cyber-ui-docking`: the "Recent Files Menu (MRU)" requirement changes from a labelled dropdown placed after Settings to an icon-only button next to Open File.

## Impact

- `src/ui/app.rs`: the title-bar block that builds the recent-files `MenuButton` moves up next to the Open File button and drops the text label; a tooltip is attached to the icon.
- `src/i18n.rs`: no new keys, `recent_files` is reused as the tooltip.
- `tests/integration_tests.rs`: no behavioural change to test at the engine level; the i18n coverage test keeps passing.
- `README.md`: screenshot description of the title bar if it lists the buttons in order.
