## 1. Engine

- [x] 1.1 Add `unseen_lines`, `unseen_severity`, `visible_this_frame` and `mark_seen()` to `TailEngine`; update them in `poll_updates`
- [x] 1.2 Tests: counting while hidden, clearing on seen, severity max, cap

## 2. UI

- [x] 2.1 Render the badge in `TabViewer::title` (count pill with theme colour); clear in `TabViewer::ui`
- [x] 2.2 Settings toggle "Flash window on background alerts" persisted in `fasttail.ini`; send `RequestUserAttention` once per unfocused period
- [x] 2.3 i18n keys in five languages; i18n test

## 3. Docs

- [x] 3.1 README feature list
