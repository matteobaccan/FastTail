## 1. Engine

- [x] 1.1 Add `bookmarks: BTreeSet<usize>`, `toggle_bookmark`, `bookmark_next`, `bookmark_prev` (visible-only, wrap-around) and `clear_bookmarks` to `TailEngine`; drop them on truncation
- [x] 1.2 Tests: toggle, navigation skipping filtered rows with wrap, clearing on truncation

## 2. Persistence

- [x] 2.1 Add a `[bookmarks]` section to `FastTailConfig` (path -> indices, caps 1,000 per file / 50 files, LRU eviction) with `to_ini` / `from_ini`
- [x] 2.2 Restore bookmarks on open when the file has enough lines; save on close and on toggle
- [x] 2.3 Tests: round-trip, cap, discard when file shrank

## 3. UI

- [x] 3.1 Handle Ctrl+F2 / F2 / Shift+F2 for the focused stream and scroll to the target row
- [x] 3.2 Draw `★` in the marker column (search glyph wins) and the bookmark tint in TXT and MD source views (HEX rows are byte rows, not lines: no bookmark marker there)
- [x] 3.3 Add "Clear bookmarks" to the stream menu; i18n keys in five languages; extend the i18n test

## 4. Docs

- [x] 4.1 README shortcut table and F1 help dialog
