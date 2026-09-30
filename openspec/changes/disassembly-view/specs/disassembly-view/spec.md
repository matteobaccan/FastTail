## ADDED Requirements

### Requirement: Disassembly View
Each stream SHALL offer an ASM view next to its text and HEX views: the ASM button of the window's view switcher (in a fixed position, as TXT, HEX and MD) and `d` in the terminal interface. The view SHALL show one row per decoded instruction with its file offset (or virtual address for a recognised executable), its bytes in hexadecimal and its mnemonic and operands in Intel syntax, in the theme's colours. A byte that does not start a valid instruction SHALL be shown as `db 0xNN` on its own row and decoding SHALL resume at the next byte. Only the rows on screen SHALL be decoded, reading at most 15 bytes per row forward and 64 bytes per row when scrolling back, through the block cache; no file SHALL be read whole and no background scan SHALL be started. Both interfaces SHALL show the same text for the same file, offset and architecture.

#### Scenario: A function prologue
- **WHEN** a stream's bytes at offset 0 are `55 48 89 E5 48 83 EC 20` and the user opens the ASM view with the x86-64 architecture
- **THEN** the first three rows read `00000000  55  push rbp`, `00000001  48 89 E5  mov rbp, rsp` and `00000004  48 83 EC 20  sub rsp, 0x20`.

#### Scenario: Invalid bytes do not stop the view
- **WHEN** the bytes at the top of the view hold an opcode that is invalid in 64-bit mode followed by valid code
- **THEN** the invalid byte is shown as `db 0xNN` and the next rows show the following instructions.

#### Scenario: A large image costs the same as a small one
- **WHEN** the user opens the ASM view on a 4 GB file
- **THEN** the first screen is drawn without a progress bar or a background job, and the memory used by the stream does not grow with the file size.

### Requirement: Architecture
The ASM view SHALL decode x86 in 16-bit, 32-bit or 64-bit mode, chosen per stream from the stream toolbar in the window and with `Shift+D` in the terminal interface, 64-bit by default. The choice SHALL be saved per stream as `disasm_arch=` (`x86-16`, `x86-32` or `x86-64`) in `fasttail.ini` and in session files only when it is not `x86-64`; a build without this capability SHALL ignore the key. The architecture read from a recognised executable header SHALL be used while the file is recognised.

#### Scenario: 32-bit code
- **WHEN** the stream's architecture is changed from x86-64 to x86-32 on the bytes `66 B8 01 00`
- **THEN** the row reads `mov ax, 1` in 32-bit mode.

#### Scenario: Choice restored
- **WHEN** the user chooses x86-16 for a stream, closes FastTail and starts it again
- **THEN** the stream's ASM view decodes in 16-bit mode.

### Requirement: Where the View Starts
Switching from HEX to ASM SHALL start decoding at the byte under the HEX cursor, and switching back SHALL put the HEX cursor on the offset of the first ASM row. Otherwise the view SHALL start at the current byte search hit when there is one, at the entry point of a recognised executable, or at offset 0. Go to (`Ctrl+G` and `:` in the terminal, the go-to dialog in the window) SHALL accept a decimal or `0x` offset, a `0x` virtual address for a recognised executable, and `entry`; the target SHALL become the first row, decoded exactly from there.

#### Scenario: From HEX to ASM
- **WHEN** the HEX cursor is on offset `0x1F40` and the user presses `d`
- **THEN** the first ASM row starts at offset `0x1F40`.

#### Scenario: Go to the entry point
- **WHEN** the stream is a PE executable and the user goes to `entry`
- **THEN** the first row is the instruction at the entry point, shown with its virtual address.

### Requirement: Executables
When the first 4 KB of the file hold a PE header (`MZ` then `PE\0\0`) or an ELF header (`\x7FELF`, 32 or 64 bit, little endian), the ASM view SHALL show virtual addresses, name the section on the first row of each section, decode executable sections as code and show the bytes of other sections as `db` rows of at most 8 bytes. A malformed or truncated header SHALL fall back to decoding the file from offset 0 as raw code, with a notice.

#### Scenario: Code and data sections
- **WHEN** the user scrolls an ELF executable from the end of `.text` into `.rodata`
- **THEN** the last rows of `.text` are instructions and the rows after the `.rodata` label are `db` rows.

#### Scenario: A damaged header
- **WHEN** a file starts with `MZ` but its PE offset points past the end of the file
- **THEN** the view decodes from offset 0 as raw code and says the header could not be read.

### Requirement: Navigation, Search, Follow and Copy
In the ASM view the arrow keys SHALL move by one instruction, page keys by a screen, and the wheel and scroll bar SHALL move over the file by offset. Scrolling back SHALL re-synchronise by decoding from up to 64 bytes before the target and keeping the chain that lands on it. `n` / `N` and the window's search SHALL step through the byte search hits as in HEX view, the row of a hit starting at the hit. Follow SHALL show the last instructions of a growing file; a truncation or rotation SHALL move the view back to its start. Copy SHALL put the selected rows on the clipboard as `offset  bytes  instruction` lines.

#### Scenario: Down and up again
- **WHEN** the user presses Page Down three times and Page Up three times on a code section
- **THEN** the first row is again at the offset it started from.

#### Scenario: Search a byte pattern
- **WHEN** the user searches `E8` bytes in ASM view and presses `n`
- **THEN** the view shows a row starting at the next `E8` byte, decoded as a `call`.

#### Scenario: Copy rows
- **WHEN** the user selects the three prologue rows and copies them
- **THEN** the clipboard holds three lines, each with the offset, the bytes and the instruction.
