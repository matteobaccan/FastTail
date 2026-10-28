## 1. Discovery

- [ ] 1.1 `src/rotation.rs`: sibling names (D1), ordering, cap; unit tests for each naming scheme
- [ ] 1.2 Settings `rotations=ask|always|never`, `--rotations`, workspace and session `rotations=1`; round-trip tests

## 2. Chained source

- [ ] 2.1 Chained byte range over plain and spooled compressed pieces; piece start offsets in the index
- [ ] 2.2 Rotation while following (D3)
- [ ] 2.3 Tests: three pieces (one `.gz`), order and line numbers, a piece without a final newline, rotation while following

## 3. Interface

- [ ] 3.1 Stream bar offer, piece marks in the gutter and the overview strip, `Go to piece…`
- [ ] 3.2 The same in the terminal interface (chip, key, piece marks)

## 4. Texts and documentation

- [ ] 4.1 i18n keys in every language (window and terminal)
- [ ] 4.2 README, CHANGELOG `[Unreleased]`, `docs/ui-design.md`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, focused tests; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change
