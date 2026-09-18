## Context

`FastTailTabViewer::ui` is invoked only for tabs that are displayed, so "the tab was drawn this frame" is exactly "the user can see it". `poll_updates` appends lines in the engine independently of visibility.

## Goals / Non-Goals

**Goals:** make activity in hidden tabs visible at a glance, cheap to maintain, no new threads.

**Non-Goals:** OS notifications (toasts); per-tab unread history; badges on floating windows' title bars beyond the tab strip.

## Decisions

- **Counter in the engine, cleared by the viewer**: `poll_updates` increments `unseen_lines` for every appended line while `visible_this_frame` is false; the tab viewer sets `visible_this_frame` when it draws the stream and clears the counter. Simple and testable.
- **Severity = max over unseen lines** of (highlight rule with sound preset > plain highlight > none), extended with the log level when available. The badge colour follows the theme's warn/error colours.
- **Attention request only when the window is unfocused** and only for rule matches with a sound preset, so it mirrors the audio alert semantics; off by default.

## Risks / Trade-offs

- [Tab strip clutter with many tabs] → the badge is a compact pill after the file name, capped at `999+`.
- [egui_dock title API is a `WidgetText`] → the badge is rendered as a coloured suffix in the title text; a custom-painted tab is not needed.
