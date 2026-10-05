## Why

FastTail already opens any file and shows its bytes in the HEX view, with search, go-to
offset and follow. When the bytes are machine code (a crash dump's code region, a firmware
image, an `.exe` or `.so` next to the logs it wrote), the user sees opcodes as hex pairs
and has to copy them to a separate disassembler (objdump, a hex editor with a disassembly
pane such as Hiew or 010 Editor, an online tool) to read them. The maintainer asked for a
disassembly view of what the stream shows, in both the window and the terminal interface.

## What Changes

- A fourth view of a stream, **ASM**, next to TXT, HEX and MD in the window's view
  switcher and on `d` in the terminal interface: from a byte offset, one row per decoded
  instruction with its offset (or virtual address), its bytes in hex and its mnemonic and
  operands, in Intel syntax.
- **Architectures:** x86 16-bit, 32-bit and 64-bit, chosen per stream (x86-64 by default)
  and saved per stream as `disasm_arch=` (`x86-16`, `x86-32`, `x86-64`; older builds
  ignore it). ARM64 is a follow-up (see non-goals).
- **Where it starts:** the byte under the HEX cursor when switching from HEX, the
  current search hit when there is one, else offset 0. Go to (`Ctrl+G`, `:`) takes an
  offset (`4096`, `0x1000`) or, for an executable, an address (`0x401000`) or `entry`.
- **Executables:** a PE (`.exe`, `.dll`) or ELF file is recognised by its header; the
  view then shows virtual addresses, starts at the entry point, names the section of each
  row (`.text`), and marks bytes outside code sections as data (`db`). Other files are
  decoded from offset 0 as raw code.
- **Invalid bytes** show as `db 0xNN` (one byte, one row), never stop the view.
- **Navigation:** the arrows, pages, wheel and scroll bar move by instruction; scrolling
  back re-synchronises by decoding from up to 64 bytes before the target (standard
  heuristic, may be a few bytes off in data mixed with code); `n` / `N` search the bytes
  (same search as HEX); a follow mode shows the end of a growing file.
- **Copy** copies the selected rows as text (offset, bytes, instruction).
- Only the rows on screen are decoded, read through the block cache: no file is held in
  memory and no background scan is needed.

Target release: **0.21.0** (re-planned by the maintainer on 2026-10-05, from 0.23.0, with the structured logs and folder sources),
per the release plan in `docs/competitor-analysis.md` section 8. Priority: **low**
(maintainer request, niche for a log viewer). Effort: **M (1–3 weeks)**.

### Non-goals

- ARM, ARM64, RISC-V, MIPS and other architectures (a follow-up change once x86 ships;
  each needs its own decoder crate).
- AT&T syntax, symbol names from debug information (PDB, DWARF), cross-references,
  control-flow graphs, function detection or decompilation: FastTail is not a reverse
  engineering suite.
- Editing or patching bytes.
- Mach-O and other executable formats beyond PE and ELF.

## Capabilities

### New Capabilities

- `disassembly-view`: the ASM view of a stream, its architectures, start offset,
  executable headers, navigation, search, copy and follow, in both interfaces.

### Modified Capabilities

(none; the view switcher of `cyber-ui-docking` and the key list of `terminal-interface`
gain an entry, described in this capability)

## Impact

- New dependency **`iced-x86`** (pure Rust, MIT, no C code), with only its `decoder` and
  `intel` formatter features: x86 decoding without a C toolchain, so the four release
  targets build as today. *Considered:* `capstone` (more architectures, but a C library in
  every build); `yaxpeax-x86` (smaller, less complete formatter).
- `src/disasm.rs` (new, UI-agnostic): decoding a window of rows from an offset, backward
  re-synchronisation, PE / ELF header reading (entry point, sections, image base).
- `src/tail_engine.rs`: `ViewMode::Asm`, the per-stream architecture, row cache of the
  last decoded screen.
- `src/ui/dock.rs`: the ASM button and the view; `src/tui/app.rs` and `src/tui/keys.rs`:
  `d`, the view, the architecture choice.
- `src/config.rs` / `src/session.rs`: `disasm_arch=` per stream.
- `src/i18n.rs` (every language), README, `docs/ui-design.md`, CHANGELOG, tests.
