## Context

`TailEngine` already keeps `search_matches: Vec<usize>` with a current index and wrap-around navigation; the marker column in every view is driven by that state. Bookmarks are the same shape with user-chosen indices.

## Goals / Non-Goals

**Goals:** mark and revisit lines with the keyboard, see marks in every view, keep them across restarts for the same file.

**Non-Goals:** named or annotated bookmarks; a bookmark list panel (a later change can add one); sharing bookmarks between files.

## Decisions

- **Bookmarks are engine state** (`BTreeSet<usize>`), navigated with the same wrap-around helper used by search so F2 behaves exactly like F3. Navigation goes over bookmarks that are currently visible under the active filter; hidden ones are skipped but kept.
- **Persistence by absolute path** in `fasttail.ini`, one entry per file with at most 1,000 indices, dropped when the file's line count on reopen is smaller than the largest index (the file was rewritten). Line indices rather than byte offsets keep the config human-readable and match what the UI shows.
- **Marker precedence**: current search match `▶`, other match `●`, bookmark `★`; a bookmarked row that is also a match shows the match glyph and the bookmark tint.
- **Ctrl+F2 / F2 / Shift+F2** mirror Visual Studio; they are free in the current shortcut table.

## Risks / Trade-offs

- [Line indices shift when the file is rotated in place] → bookmarks are cleared on truncation like search matches; for appends they stay valid.
- [Config grows with many files] → cap of 1,000 indices per file and 50 files, least recently used dropped.
