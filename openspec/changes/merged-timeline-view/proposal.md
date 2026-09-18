## Why

A request crosses several services and several log files. Tailviewer's merged view interleaves files by timestamp so the sequence of events reads top to bottom. FastTail can dock the files side by side but the user has to correlate times by eye.

## What Changes

- "New merged view..." creates a virtual stream from two or more open streams; its rows are the union of their lines ordered by detected timestamp, each row prefixed with a colour chip and the short name of its source.
- The merged view follows all sources live, supports the usual filters, search, highlight rules, bookmarks and export, and lets the user toggle sources on and off.
- Sources without timestamps cannot be merged and are refused with a hint.

## Capabilities

### New Capabilities
- `merged-timeline`: a virtual stream interleaving several streams by timestamp.

### Modified Capabilities
- (none)

## Impact

- New `src/merged_stream.rs`: `MergedStream { sources: Vec<StreamRef>, order: Vec<(u32 src, usize line)> }` with an incremental k-way merge on append; adapters so the renderer treats it like a `TailEngine` (a `StreamView` trait extracted from the engine's read-side API).
- `src/ui/dock.rs` / `app.rs`: creation dialog, source chips, tab handling; `src/config.rs`: merged views persisted by source paths.
- Depends on `timestamp-range-filter` (timestamp cache).
