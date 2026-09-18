## 1. Engine

- [ ] 1.1 Add `Source` (file or pattern) to `TailEngine`, the `*`/`?` matcher, `resolve_newest(dir, pattern)` and `switch_to(path)` preserving the documented state
- [ ] 1.2 Rescan on the poll timer every 2 s; empty-stream state when nothing matches
- [ ] 1.3 Tests: matcher, newest selection with ties, switch preserving filters and resetting buffer/bookmarks, no-match then first-file

## 2. Config and UI

- [ ] 2.1 Persist pattern entries in open files and recent list (`to_ini` / `from_ini`, round-trip test)
- [ ] 2.2 Accept patterns in the open dialog (typed path) and via drag & drop of a directory (prompt for a pattern); show pattern + current file in the stream bar; switch notice
- [ ] 2.3 i18n keys in five languages; i18n test

## 3. Docs

- [ ] 3.1 README feature list and comparison table
