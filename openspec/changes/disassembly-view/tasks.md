## 1. Decoding core

- [ ] 1.1 Add `iced-x86` (`default-features = false`, features `std`, `decoder`, `intel`); measure the executable size before and after for the PR
- [ ] 1.2 `src/disasm.rs`: `Arch { X86_16, X86_32, X86_64 }`, decode a screen of rows from an offset (offset, bytes, text), invalid bytes as `db 0xNN`
- [ ] 1.3 Backward re-synchronisation from up to 64 bytes before an offset; unit tests on known code (a function prologue and epilogue, a jump table, data between functions)
- [ ] 1.4 PE and ELF header reader (32 and 64 bit): architecture, image base, entry point, sections; malformed headers fall back to raw decoding; tests with small generated images

## 2. Engine

- [ ] 2.1 `ViewMode::Asm`, per-stream `disasm_arch`, `top_offset`, row cache of the decoded screen
- [ ] 2.2 Start offset from the HEX cursor, the current search hit, the entry point or 0; go to offset, `0x` address and `entry`
- [ ] 2.3 Byte search steps (`n` / `N`) placing the hit at the top; follow on a growing file; reset on truncation or rotation
- [ ] 2.4 `disasm_arch=` in `[stream_N]` and sessions (written only when not `x86-64`); round-trip test; an older build ignores it
- [ ] 2.5 Integration tests: rows of a known binary, scrolling down then up returns to the same offsets, follow, truncation

## 3. Window interface

- [ ] 3.1 ASM button in the TXT/HEX/MD switcher (fixed position), the view with offset / bytes / instruction columns in theme colours (mnemonic, registers, immediates, `db` dimmed)
- [ ] 3.2 Architecture selector in the stream toolbar while in ASM; section names and virtual addresses for executables
- [ ] 3.3 Keys, wheel and scroll bar by instruction; selection and copy; command palette entries

## 4. Terminal interface

- [ ] 4.1 `d` toggles ASM (from HEX, keeping the offset); the view drawn with the same columns and theme colours, ASCII mode included
- [ ] 4.2 Architecture choice (`Shift+D` cycles), shown in the window's bottom border; go to, search, follow and copy as in HEX
- [ ] 4.3 Help entries; capture tests at 80 and 200 columns

## 5. Texts and documentation

- [ ] 5.1 New i18n keys in every language, added to the exhaustive i18n test
- [ ] 5.2 README section, `docs/ui-design.md` (switcher, toolbar, TUI keys), CHANGELOG `[Unreleased]`

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 6.2 Local preview exe for the maintainer before the release
- [ ] 6.3 After the release, archive the change so the `disassembly-view` capability is created
