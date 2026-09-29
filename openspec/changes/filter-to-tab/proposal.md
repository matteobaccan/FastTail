## Why

A stream has one set of filters at a time. To keep "the ERROR lines" in view while also
reading "the lines of request 7f3a" of the same file, the user today edits the filter back
and forth, or opens the file twice and filters each copy (which saves both as the same
path and confuses bookmarks). LogExpert's **filter to tab** answers this: the result of
the current filter becomes its own tab that keeps following the file, and the original
tab is free for the next question. The post-0.12.0 competitor scan lists it among
LogExpert's strengths.

## What Changes

- Stream menu and row context menu item **"Open filter as new tab"** (enabled when the
  stream has an active filter: include / exclude terms, minimum level or time range).
- It opens a **derived stream** next to the source tab holding the lines that pass the
  source's filter **at that moment**; the filter is copied (frozen), so later changes of
  the source's filter do not change the derived tab.
- The derived stream **keeps following**: lines appended to the source that pass the
  frozen filter are appended to it; a truncation or rotation of the source rebuilds it.
- It is a **normal stream**: its own search, filters (narrowing further), highlight
  rules, collapse, bookmarks, copy, export and "Open filter as new tab" again. The global
  filter applies to it as to every stream.
- The line-number gutter shows the **source line numbers**; "Show in context" and
  `CTRL + K` on a derived row focus the source stream and show that line in context.
- Tab title `app.log ▸ ERROR` (the first include term, or the level / time range when
  there is no term; the full frozen filter in the tooltip) with a distinct tab icon.
- If the source stream is closed, the derived tab stays open, stops following and says
  so in its stream bar.
- The derived stream is saved in the workspace and in session files as its source path
  plus the frozen filter, and rebuilt on restore.
- Storage: the matched lines are copied into a spool file, like standard input, bounded
  by the same `stdin_spool_max_mb` and free-space margin.

Target release: **0.21.0** for saving and restoring the derived tabs (tasks 3.1, 3.2); the rest shipped in 0.14.0 (planned for 0.13.0, moved after 0.14.0 and after the terminal interface of 0.20.0), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Editing the frozen filter of a derived tab (the user narrows it with the tab's own
  filters, or makes a new one from the source).
- Derived tabs from the Find results tab or from several sources at once
  (`merged-timeline-view` covers the multi-file case).
- Writing the derived content to disk outside the spool (export covers that).
- Context lines (`context-lines`) copied into the derived tab: only matching lines.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filters-and-highlighting`: new requirements Filter Result Tab and Derived Stream
  Persistence.

## Impact

- `src/filter_tab.rs` (new): `DerivedSpec` (source path, frozen `FilterSpec`), a feeder
  that scans the source (existing `scan_job` filter pass) then follows appended lines,
  writing the spool and a `Vec<u64>` of source line numbers.
- `src/spool.rs` / the shared spool feed from `ssh-sources` / `system-sources`: reused
  for the derived spool; `src/tail_engine.rs`: an optional source line-number map used by
  the gutter, go-to line and show in context.
- `src/ui/dock.rs`: menu items, tab title and icon, source-closed state, show in context
  redirected to the source.
- `src/session.rs`, `src/config.rs`: a `derived` stream entry (source path, filter keys
  as for a stream, `filter:` identity); not added to recent files.
- `src/i18n.rs` (16 languages), README, CHANGELOG.
- **TUI (0.20.0, PR #132)**: the feeder is UI-free; the TUI can open a derived view as a
  new buffer with the same spec.
