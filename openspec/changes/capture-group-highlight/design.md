## Context

Rows are painted as a single `RichText` per line with the style of the first matching rule; there is no span-level formatting yet. egui `LayoutJob` supports per-range formats at the same cost order as a plain label.

## Goals / Non-Goals

**Goals:** colour parts of a line, ad-hoc labels in two keystrokes, no slowdown for existing whole-row rules.

**Non-Goals:** persisting quick labels (promote to a rule instead); nested/overlapping span merging beyond "first rule wins per byte".

## Decisions

- **Two evaluation paths**: rows keep the fast `match_highlight` whole-row path; only when a `captures_only` rule or a quick label exists does the renderer call `match_highlight_spans`, which returns byte ranges. This keeps the common case allocation-free.
- **First rule wins per byte**, consistent with the top-down priority of whole-row rules; quick labels rank below user rules.
- **Quick label text** is the selected text if any, else the word under the current search match; case-insensitive plain text, painted with the 9 preset colours of the theme.
- **Cap of 64 spans per row** to bound layout work on pathological lines.

## Risks / Trade-offs

- [Span path is slower than whole-row] → only active when such rules exist; measured in `filter_bench` with a new phase.
- [Selection dependency] → quick labels from selection need `copy-lines-and-export`; the search-match word path works without it.
