## 1. Engine

- [ ] 1.1 Add `goto_line(&mut self, one_based: usize) -> GotoResult` resolving clamp and filter visibility and setting `requested_scroll_y` (centre the row)
- [ ] 1.2 Add relative resolution from a given top line
- [ ] 1.3 Tests: exact, clamped, hidden-line substitution, relative jumps

## 2. UI

- [ ] 2.1 Ctrl+G popup in the stream bar with a numeric field, Enter/Esc, inline validation and the substitution message
- [ ] 2.2 Pause follow mode on jump; i18n keys in five languages; i18n test

## 3. Docs

- [ ] 3.1 README shortcut table and F1 help dialog
