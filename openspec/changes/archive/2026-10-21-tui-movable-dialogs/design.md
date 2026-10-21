## Context

`App::dialog()` (`src/tui/app.rs`) draws every terminal dialog: it centres a rectangle of
the requested size with `view::centered`, clears it, draws the border, title and shadow,
the `[ OK ]` / `[ Cancel ]` buttons, and records a `DialogHit { outer, ok, cancel }` for
the mouse. A click outside `outer` closes the dialog like `Esc`.

The floating dock windows already move and resize: `mouse::edges_at` maps a border cell to
`Edges`, `DockDrag::FloatMove { grab }` / `DockDrag::FloatResize { edges, from }` follow the
pointer, and `dock::resized` / `dock::clamp_into` compute the new rectangle; their top
border carries `[x]`. This change applies the same mechanics to two dialogs.

Everything runs on the UI thread at draw and mouse-event time; the cost is a few `Rect`s,
no per-line work and no memory per file or line. Files, sizes and rotation are not
involved.

## Goals / Non-Goals

**Goals:** Settings and Keys movable, resizable and closable with `[x]`, content that
adapts to the size, geometry remembered for the run.

**Non-Goals:** the other dialogs, keyboard move / resize, geometry in `fasttail.ini`.

## Decisions

1. **Opt-in per dialog.** `dialog()` gets a `DialogFrame` argument (or a sibling
   `movable_dialog()`): `None` keeps today's centred, fixed behaviour for every other
   dialog; `Some(&mut Option<Rect>)` uses the stored rectangle when set, else the centred
   default, and draws `[x]`. Alternative: make every dialog movable at once; rejected to
   keep the change small and the prompts (one-line fields) unchanged.
2. **Stored geometry in `App`:** `settings_rect: Option<Rect>` and `help_rect: Option<Rect>`,
   in memory only. `None` means "centred default". Every draw clamps the stored rect into
   the current screen with `dock::clamp_into` and the minimum sizes (40x8 Settings, 30x6
   Keys, capped to the screen), so a terminal resize needs no separate handler. A double
   click on the title sets it back to `None`.
3. **Hit map.** `DialogHit` gains `close: Option<Rect>` (the `[x]` cell) and
   `movable: bool`. The mouse code tests, in order: `[x]`, buttons, edges (reusing
   `edges_at` on `outer`, only when `movable`), title row, then the content. `edges_at`
   stays private to `mouse.rs`; a `dialog_target()` there returns a new
   `Target::DialogClose | DialogTitle | DialogEdge(Edges)`.
4. **Drag state.** Two new `DockDrag` variants, `DialogMove { grab }` and
   `DialogResize { edges, from }`, so the existing drag / release plumbing carries them; the
   release of a dialog drag clears the state and never goes through the "click outside
   closes" path (that path runs on the press, so a press inside the dialog never closes it
   anyway; the test pins it).
5. **Content follows the inner area.** Settings already scrolls by `inner.height`; only the
   height computation moves from "fit the lines" to "the dialog's height". The Keys
   layout today measures `area` (the screen) to pick 1 to 3 columns; it takes the dialog's
   inner width and height instead when a stored rect exists, and the screen as now for the
   default. `help_half` (used by Left / Right between columns) follows the chosen layout.
6. **`[x]`** is drawn as the floating windows draw it (right-aligned in the top border,
   before the corner), with the dialog's border style; it already has its translation
   (the literal `[x]` is not translated).

## Risks / Trade-offs

- [The title row of a narrow dialog is mostly title text, little room to grab] → the
  whole top row except the corners and `[x]` is the grab zone, as for floating windows.
- [Keys at 1 column in a small dialog scrolls a lot] → the minimum is 30x6 and the
  default is still the size that shows everything when the screen allows it.
- [A future dialog opting in forgets to size its content from the inner area] → the
  helper returns the inner rect, and both dialogs get a capture test at two sizes.

## Migration Plan

None: no saved state, no key. Rollback is reverting the PR.

## Open Questions

- Extend the same behaviour to the rule editor, presets, tools and open-file dialogs in a
  later change? Proposed: yes if the maintainer finds it useful after trying these two.
- Save the geometry in `fasttail.ini` (a `[tui]` section) so it survives a restart?
  Proposed: not now; it would be a new key.
