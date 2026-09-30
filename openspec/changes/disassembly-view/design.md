## Context

The HEX view (`ViewMode::Hex`) already reads any file through the block cache, draws only
the visible rows (`total_hex_rows`, bytes per row from the window width), searches bytes
(`search_byte_matches`), goes to an offset and follows a growing file, in the window and in
the terminal interface. A disassembly view shares all of that except the row model: a HEX
row is a fixed number of bytes, an x86 instruction is 1 to 15 bytes, so row N does not map
to an offset by multiplication.

## Goals / Non-Goals

**Goals:**
- Read x86 machine code of any file in place, in both interfaces, with the same keys and
  the same memory rule as the HEX view.
- Start where the user is looking (HEX cursor, search hit, entry point).

**Non-Goals:**
- A reverse-engineering tool (symbols, graphs, decompiler), other architectures (see the
  proposal).

## Decisions

1. **Decoder: `iced-x86`**, features `decoder` + `intel` only (no encoder, no block
   encoder, no `code_asm`), `default-features = false` plus `std`. Pure Rust, so no C
   toolchain on the four targets; it decodes hundreds of MB/s, far above what a screen
   needs. *Rejected:* `capstone` (C library and bindgen in every build, cross-builds for
   Linux ARM64 and macOS ARM64 harder), `yaxpeax-x86` (formatter less complete).
2. **Rows by offset, not by index.** The view keeps `top_offset` (the offset of the first
   row) instead of a row index; a screen is decoded from `top_offset` forward, up to the
   rows of the window, from at most `rows × 15` bytes read through the block cache. The
   scroll bar maps the offset over the file size (the number of instructions of a large
   file is unknown and never computed).
3. **Scrolling back re-synchronises.** Moving up from offset `o` decodes from each
   candidate start in `o-64 .. o` and keeps the one whose instruction chain lands exactly
   on `o` with the fewest invalid bytes (the classic linear-sweep heuristic; 64 bytes are
   enough for x86 chains to converge in practice). A page up repeats it per row. In data
   mixed with code it may be a few bytes off; the offset column shows where decoding
   starts, and go to an offset restarts exactly there.
4. **Invalid bytes are data.** A byte that does not start a valid instruction is shown as
   `db 0xNN` and decoding resumes at the next byte, so the view never stops.
5. **Executables.** On opening the ASM view, the first 4 KB are checked for `MZ` + `PE\0\0`
   or `\x7FELF`. For PE and ELF (32 and 64 bit, little endian) the header gives the
   architecture, the image base, the entry point and the section table; the view then shows
   virtual addresses, starts at the entry point, prefixes the section name on the first row
   of each section, decodes executable sections as code and shows other sections as `db`
   rows of up to 8 bytes. The architecture of the header overrides the stream's choice
   while the file is recognised. Header parsing is a small reader over the bytes (no
   `goblin` / `object` dependency: only a handful of fields are needed, and a malformed
   header must fall back to raw decoding rather than fail).
6. **Search, go to, follow reuse HEX.** `n` / `N` step through `search_byte_matches` and put
   the row containing the hit at the top (decoding from the hit offset, so the hit starts a
   row); go to takes an offset, `0x` address (translated through the sections), or `entry`;
   follow decodes the last `rows × 15` bytes and shows the rows that end at the file end.
7. **Copy** writes the selected rows as `offset  bytes  instruction` lines, the same text
   in both interfaces.
8. **Architecture per stream** is saved as `disasm_arch=` in `[stream_N]` and in sessions,
   written only when it differs from the default `x86-64`; older builds ignore the key.

Threads and memory: decoding runs on the UI thread, bounded by the screen (at most
`rows × 15` bytes forward plus `rows × 64` bytes when scrolling back, e.g. 200 rows ⇒
under 16 KB read and decoded per frame, well under a millisecond). The row cache holds the
decoded screen only (about 100 bytes per row). No background job, no per-line or per-byte
memory for the file: a 10 GB image costs the same as a 10 KB one. Growing files: follow
re-decodes the tail; truncation or rotation resets `top_offset` to 0 (or the entry point).
Windows file sharing: reads go through the existing block cache, which opens files with
the same share flags as the text view.

## Risks / Trade-offs

- [Wrong sync in data regions] → the offset column is always shown, go to an offset resets
  decoding exactly, and invalid bytes never stop the view.
- [New dependency size] → only `decoder` + `intel`: expected to add a few hundred KB to the
  executables; measured in the PR and reported against the current size.
- [Niche feature in a log viewer] → it lives entirely in its own view and module; a stream
  that never opens ASM pays nothing.

## Open Questions

- ARM64 next (with `yaxpeax-arm` or `disarm64`)? Decided after 0.23.0 from feedback.
- Should the TXT view offer "Disassemble selection" for hex dumps inside log lines
  (`code: 55 48 89 e5`)? Left out of this change.
