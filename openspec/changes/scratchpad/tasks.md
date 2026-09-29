## 1. Tab and editor

- [x] 1.1 `FastTailTab::Scratchpad` in the dock (saved in the layout); title-bar menu item
- [x] 1.2 `src/ui/scratchpad.rs`: multiline editor with the stream font, find box, Save as…, Clear with confirmation, 4 MB cap

## 2. Sending and jumping

- [x] 2.1 Row menu "Send to scratchpad" (with and without reference line) and CTRL + SHIFT + N in the focused stream; collapsed rows expanded as copy does
- [x] 2.2 "Send to scratchpad" in the Find results tab
- [x] 2.3 Reference line parsing, paths map, CTRL + click / Enter jump (open the file if needed)
- [x] 2.4 Tests: send from two streams, collapsed rows, cap refusal, reference parsing and resolution, missing file

## 3. Persistence

- [x] 3.1 Sidecar `<session>.scratch.txt` and default `scratchpad.txt`; debounced save and save on exit; "Save session as…" copies the pad
- [x] 3.2 Tests: round trip, session switch loads the other pad, missing file is an empty pad

## 4. Texts and documentation

- [x] 4.1 New i18n keys (tab, menu items, messages) in all 16 languages; add them to the exhaustive i18n test
- [x] 4.2 Help dialog, README (feature list, shortcuts) and CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 5.3 After the release, archive the change so the `scratchpad` capability is created
