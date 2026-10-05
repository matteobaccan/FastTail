## Context

A stream is a `TailEngine` over one path (`FastTailTab::LogStream(PathBuf)`), with its
filter (`FilterSpec`: up to 8 include / 8 exclude terms, minimum level, time range),
computed on a background scan above 16 MB. Standard input is a spool file tailed as a
stream with a pseudo-path (`<stdin>`); `ssh-sources` and `system-sources` propose a
shared spool feed and a non-file stream identity. Show in context switches a stream to
its unfiltered lines around a line.

## Goals / Non-Goals

**Goals:** several simultaneous filtered views of one file, each following; every stream
feature on them; the way back to the source line.

**Non-Goals:** editable frozen filters, multi-source derived tabs.

## Decisions

### D1. A spool copy, not a virtual view
The derived tab is a real `TailEngine` over a spool file holding the matched lines, fed
by a worker, plus a `Vec<u64>` mapping each derived line to its source line number.
Everything (search, filters, collapse, wrap, export, bookmarks) works unchanged.
Cost: disk equal to the matched bytes and 8 bytes of memory per derived line.
*Alternative:* a view over the source engine's line index with its own visible list —
rejected, every engine feature assumes one file per engine; the refactor is L.

### D2. Feeding
Initial fill: a scan job over the source with the frozen spec (same code as the
background filter), writing matches in order. Then the feeder follows the source path
itself with its own shared-read handle and the same tail logic (rotation and truncation
detection), so the derived tab keeps working if the source tab is closed while the file
still exists. The "source closed" state applies when the source path's stream is closed:
the feeder stops, by choice, to avoid invisible background work. Truncation or rotation
of the source empties the spool (handled by the derived engine as a truncation) and
refills it.

### D3. Identity and persistence
Identity `filter:<n>:<source path>` (n unique per window) as a non-file pseudo-path,
never in recent files. Workspace and session entries store the source path, the frozen
terms, toggles, level and time range with the existing stream keys, plus the derived
tab's own filters and view options. On restore the spool is rebuilt from the source.
Bookmarks of a derived tab are stored by source line number, so they survive rebuilds.

### D4. Line numbers and navigation
The gutter and go-to line use source numbers (go-to `1204` finds the derived row holding
source line 1204, or the next one). "Show in context" focuses the source tab and runs its
show in context on the mapped line; if the source tab is closed it is reopened.

### D5. Bounds
The spool uses `stdin_spool_max_mb` and the 512 MB free-space margin; on reaching them
the derived stream restarts from empty (as stdin), saying earlier matches were dropped.

### As implemented (0.14.0)
- **Feeding (D2):** the spool is fed from the source *stream* (`DerivedFeeder::step`, a
  few milliseconds per frame), not from a second handle on the file: the source engine
  already decodes, strips ANSI, follows appends and reports truncation and rotation
  (`reload_generation`), so one reader serves both and every encoding works. An
  unterminated last line waits for its newline.
- **Persistence (D3), done in 0.16.2:** the tab is saved as `filter:<n>:<source path>` in
  `open_files`, the dock layout and sessions (its tab is renamed from the spool to that name
  when saved and back when restored); the section holds the frozen filter as `frozen.*`
  keys and, in a session file, the source's relative path as `source_rel` (not `rel`, so
  an older build does not open the source in its place). A restored tab opens its source
  when it is not open; its bookmarks are saved by source line and applied as the fill
  reaches them. The terminal interface does not show derived tabs but keeps their entries
  while their source stays open.
- **Bounds (D5):** a full spool stops the tab from following (it says so) instead of
  restarting from empty; the large-copy warning is not done.
- **Bookmarks** are kept by derived line while the tab is open and saved by source line.

## Risks / Trade-offs

- [Broad filter copies most of a huge file] → the dialog warns when the source filter
  keeps more than 50 % of a file above 1 GB before creating the tab.
- [Two feeders tail the same file] → shared-read handles; tail cost is per appended
  bytes, small.
- [Global filter applied twice] → the frozen spec excludes the global filter; the global
  filter applies live to the derived tab like any stream.

## Migration Plan

None: new entry type in workspace and session files; older versions ignore it.

## Open Questions

- Should the derived tab include the context lines when `context-lines` is on in the
  source? Proposed: no in 0.13.0 (non-goal); maybe an option later.
- Should the frozen filter be editable from the derived tab's menu ("Edit source
  filter…", rebuilding)? Proposed: no; open another derived tab instead.
