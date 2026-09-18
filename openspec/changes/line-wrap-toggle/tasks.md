## 1. Renderer

- [ ] 1.1 Add `wrap_lines` per stream (engine field, persisted in the workspace stream entry) with a round-trip test
- [ ] 1.2 Implement variable-height layout in `show_rows` for wrap mode: estimated virtual height, real heights for visible rows, line-index anchoring of the scroll offset
- [ ] 1.3 Make search/bookmark/go-to/PgUp/PgDown jumps use the anchoring path in wrap mode
- [ ] 1.4 Cap layout to 64 KB per row

## 2. UI

- [ ] 2.1 Wrap toggle in the stream bar and Alt+W; i18n keys in five languages; i18n test

## 3. Docs

- [ ] 3.1 README shortcut table and feature list
