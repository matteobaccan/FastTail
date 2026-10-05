## 1. Reader

- [x] 1.1 `src/json_tree.rs`: order-preserving reader into a flat node list, string unescape, error position, 4 MB cap
- [x] 1.2 Fold state, visible rows with the 200-children cut, expand / collapse all below a node (5,000-row cap)
- [x] 1.3 Node path and value text to copy; tests (order kept, nested arrays, escapes, invalid JSON, paths)

## 2. GUI

- [x] 2.1 Detection after a leading timestamp (`fields::json_start`); `[+] JSON` unchanged
- [x] 2.2 The tree replaces the pretty-printed block in both row paths; typed colours; a click folds
- [x] 2.3 Node context menu: copy value, copy path, expand all below, collapse all below

## 3. Terminal interface

- [x] 3.1 `J` opens the JSON dialog on the cursor row; keys and mouse; copy through the terminal clipboard
- [x] 3.2 Help and palette entries; tests with `TestBackend`

## 4. Texts and documentation

- [x] 4.1 i18n keys in all 16 languages, exhaustive i18n test
- [x] 4.2 README, `docs/ui-design.md`, CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [x] 5.1 `cargo fmt`, `cargo clippy --all-targets` (both feature sets), focused tests; PR with Linux and Windows CI green
- [x] 5.2 Preview build for the maintainer; archive the change after the release (0.16.2)
