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
