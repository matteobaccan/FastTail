## Context

The title bar (`src/ui/app.rs`, "Primary Title Bar" panel) lays out its buttons in one `ui.horizontal`: Open File, Filter, Play, Pause, Settings, Recent Files, then a right-to-left group with Help and About. Every button is an `egui::Button` with `min_size(0, 26)`, the theme's `button_bg()` fill and a 6 px corner radius; the recent list is an `egui::menu::MenuButton::from_button`, and its handler opens the picked path after the closure through a `file_to_open` local because the closure cannot borrow `self` mutably.

## Goals / Non-Goals

**Goals:** recent files reachable next to Open File, a narrower title bar, no change to the menu's behaviour or persistence.

**Non-Goals:** redesigning the rest of the title bar, keyboard shortcut for the recent menu, the empty-state screen of PR #16.

## Decisions

- **Icon-only button with a tooltip** rather than a shorter label: the clock glyph is already the menu's identity, and every other icon-only control in the app (pin, backend chip) explains itself through `on_hover_text`. The tooltip reuses the existing `recent_files` key, so no i18n work is needed.
- **Position: right after Open File**, before Filter, because Open File and Recent are both "get a log in" actions while Filter, Play, Pause and Settings act on streams that are already open. The button keeps the accent-coloured border of Open File so the pair reads as one group; the alternative of keeping the dim border was rejected because the icon alone would look disabled.
- **Move the code block, do not duplicate it**: the `MenuButton` block and its `file_to_open` handling move as-is to their new place in the horizontal layout; only the `RichText` changes from `format!("🕒 {}", ...)` to `"🕒"`.

## Risks / Trade-offs

- [Tooltip is the only label] → the glyph is the same one used today, and the menu content names itself ("Recent Files" items and the clear entry), so discoverability is preserved.
- [Width of an emoji-only button varies by font] → `min_size` is raised to 30 × 26 so the button is a comfortable click target even when the glyph falls back to a narrow font.
