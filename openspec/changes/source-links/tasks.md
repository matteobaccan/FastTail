## 1. Locations

- [ ] 1.1 `src/source_links.rs`: patterns (D1), unit tests per language shape and for times not matched
- [ ] 1.2 Resolution against source roots with the Java package mapping (D2); tests
- [ ] 1.3 Editor command template and presets, no-shell spawn (D3); `editor_command=`, `source_roots=` round trip

## 2. Interface

- [ ] 2.1 Window: hover underline, Ctrl + click, row menu "Open source location" / "Open in FastTail", Settings fields
- [ ] 2.2 Terminal: `o` on the cursor row, pick list for several locations

## 3. Texts and documentation

- [ ] 3.1 i18n keys in every language (window and terminal)
- [ ] 3.2 README, CHANGELOG `[Unreleased]`, `docs/ui-design.md`, the external tools cookbook

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, focused tests; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change
