## Why

FastTail cannot put a log line on the clipboard or write a set of lines to a file: rows are plain egui labels with no selection, and there is no export path. BareTail Pro and klogg both copy lines and export search results, and it is the first thing a user reaches for when a line has to go into a ticket or a chat.

## What Changes

- Click selects a row, Shift+click extends the selection, Ctrl+A selects every visible row of the focused stream.
- Ctrl+C copies the selected rows (or the current row when nothing is selected) as plain text, one line per row, without line numbers or markers.
- A stream menu action "Export visible lines..." writes the rows that pass the active filters to a file chosen with the native save dialog; "Export search matches..." writes only the matching rows.
- Selection is per stream, cleared when the file is truncated or reopened, and never blocks follow mode.

## Capabilities

### New Capabilities
- `selection-and-export`: row selection in the stream view, clipboard copy and export of visible or matching rows to a file.

### Modified Capabilities
- (none)

## Impact

- `src/tail_engine.rs`: `selection: BTreeSet<usize>` (line indices) with select/extend/clear helpers and an iterator over selected lines in file order; export helpers that stream lines to a `Write` sink.
- `src/ui/dock.rs`: row click handling, selected-row tint, Ctrl+C/Ctrl+A handling scoped to the focused stream, export menu entries using `rfd::FileDialog::save_file`.
- `src/i18n.rs`: keys for the menu entries and tooltips in five languages.
- `tests/integration_tests.rs`: selection semantics and export content.
