## Context

Rows are painted by `show_rows` in `src/ui/dock.rs` from `TailEngine::get_line` over either all lines or `filtered_lines`. There is no notion of a selected row; egui labels are not selectable and the clipboard is only reachable through `ctx.copy_text`. `rfd` is already a dependency (crash dialog, open dialog), so a native save dialog costs nothing new.

## Goals / Non-Goals

**Goals:** copy one or many rows with the keyboard, export what the user currently sees or what the search found, keep the virtualised renderer untouched in cost.

**Non-Goals:** character-level text selection inside a row; rich-text or HTML clipboard formats; export of HEX or rendered Markdown views (the underlying text lines are exported instead).

## Decisions

- **Selection lives in the engine as line indices**, not in the UI, so it survives scrolling, filter changes and view switches, and tests can drive it without egui. It is a `BTreeSet<usize>` so iteration is in file order.
- **Anchor + Shift+click range** over the *visible* ordering: extending a selection under an active filter selects the visible rows between anchor and target, not hidden ones. Ctrl+click toggles a single row.
- **Ctrl+C copies text only**: the raw line content, joined with `\n`. Line numbers and the search marker are UI decoration and stay out.
- **Export streams to disk** through `BufWriter`; 12 million lines must not be collected into a `String`. Export runs on the UI thread with a progress-less write because writes of a few hundred MB take under a second on local disks; a background thread is deferred until someone needs it.
- **Selected rows use the theme accent at 25% alpha** so they remain distinguishable from the search row tint (stronger) and highlight-rule backgrounds.

## Risks / Trade-offs

- [Ctrl+C conflicts with copying from the search box] → when a text edit has focus, egui handles the shortcut first; the stream shortcut applies only when no text field is focused.
- [Huge exports block the UI briefly] → acceptable for v1, documented; background export can follow.
- [Selection indices go stale on truncation] → cleared whenever the engine resets its line index.
