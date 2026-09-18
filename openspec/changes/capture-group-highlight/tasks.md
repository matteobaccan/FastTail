## 1. Engine

- [x] 1.1 Add `captures_only` to `HighlightRule` (INI round-trip) and `match_highlight_spans` returning byte ranges with first-rule-wins and the 64-span cap
- [x] 1.2 Add quick labels (in-memory list, case-insensitive plain text, preset colour index) evaluated after user rules
- [x] 1.3 Tests: spans for captures, whole-row fallback, precedence, cap, quick label toggling

## 2. Renderer and UI

- [x] 2.1 Render rows with `LayoutJob` per-span formats when span rules or labels exist; keep the plain path otherwise
- [x] 2.2 "Highlight captures only" checkbox in the Highlights dialog; Ctrl+Shift+1..9 handling; labels strip with remove buttons; theme preset palette
- [x] 2.3 i18n keys in five languages; i18n test

## 3. Bench and docs

- [x] 3.1 Add a span-highlight phase to `benches/filter_bench.rs`
- [x] 3.2 README feature list and shortcut table
