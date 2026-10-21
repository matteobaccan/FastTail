## Context

`render_log_stream` (`src/ui/dock.rs`) draws `toggle_button`s for TXT, HEX, ASM and MD;
`TailEngine::set_view_mode` already refuses Markdown above `markdown_max_bytes`
(`markdown_too_large()`), and the MD button turns that into a `view_notice`.

## Decisions

1. **`egui::ComboBox`** with a fixed width, in the place of the first button, so the
   switcher keeps its fixed position before the controls that come and go with the mode.
   The selected text keeps the mode's colour (TXT accent, HEX and ASM secondary accent, MD
   warn), so the state is still readable at a glance.
2. **Usable modes** are computed each frame: TXT, HEX, ASM, and MD when
   `!engine.markdown_too_large()`. A stream already in MD whose file grows past the limit
   is moved back by the engine as today; the list then no longer offers MD.
3. **Why MD is missing**: a disabled label at the end of the popup with the existing
   `md_too_large` text, instead of a separate key.
4. Actions (`act(ActionId::View*)`) are handled outside the combo, unchanged.

## Risks / Trade-offs

- [One more click to switch mode] → the palette and the key bindings stay one step; the
  drop-down also frees room in the bar.
