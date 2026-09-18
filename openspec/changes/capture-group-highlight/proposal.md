## Why

Highlight rules colour the whole row. klogg can colour only the part captured by a regex group and lets the user create a quick colour label from selected text with Ctrl+Shift+1..9. Both make dense logs readable without hiding anything.

## What Changes

- A regex rule with capture groups gets a "highlight captures only" option: only the captured spans are painted (foreground/background/bold/italic), the row keeps its normal style; without the option the behaviour is unchanged.
- Quick colour labels: with a row selected, Ctrl+Shift+1..9 creates or toggles a temporary label for the word under the cursor or the selected text, painted with one of nine preset colours across all streams; labels are listed in a small strip above the stream and are not persisted.
- Span painting is limited to the first 64 matches per row.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `filters-and-highlighting`: adds capture-only highlighting and quick colour labels.

## Impact

- `src/tail_engine.rs`: `HighlightRule.captures_only`, `match_highlight_spans(&str) -> Vec<(Range<usize>, HighlightStyle)>`, quick labels as an in-memory rule list evaluated after user rules.
- `src/ui/dock.rs`: rows rendered as `LayoutJob` with per-span formats; labels strip; shortcuts. `src/config.rs`: `captures_only` in the rule sections; `src/i18n.rs`: keys.
