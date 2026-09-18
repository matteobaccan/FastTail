## 1. Title bar

- [ ] 1.1 Move the Recent Files `MenuButton` block (and its `file_to_open` handling) in `src/ui/app.rs` to right after the Open File button, before the Filter button
- [ ] 1.2 Replace the `🕒 Recent Files` label with the bare `🕒` glyph, attach `on_hover_text(t(lang, "recent_files"))`, use the accent border of Open File and `min_size(30, 26)`

## 2. Verification and docs

- [ ] 2.1 `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; run the release build and check the bar in the five languages (tooltip text, button order, menu still opens and clears)
- [ ] 2.2 Update the README title-bar description if it lists the buttons in order; CHANGELOG `[Unreleased]` entry under Changed
