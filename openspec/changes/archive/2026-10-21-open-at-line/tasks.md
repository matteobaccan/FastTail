## 1. Command line

- [x] 1.1 `src/cli.rs`: `path:line[:col]` split only when unambiguous; `--line N`; unit tests (drive letters, existing names with `:`, missing files, `:0`, huge numbers)
- [x] 1.2 `--help` and the error texts in every language

## 2. Landing

- [x] 2.1 Window: the target goes through go-to after the stream opens; waits for the index; notice past the end
- [x] 2.2 Terminal interface: the same (cursor on the line, follow paused)
- [x] 2.3 Print mode: `--line N` starts the output at line N
- [x] 2.4 Single-instance hand-off carries the line: FastTail has no single-instance hand-off (each start is its own process); the GUI / terminal hand-off forwards the arguments as given, `--line` included (in `VALUE_OPTIONS`)

## 3. Tests and docs

- [x] 3.1 Integration tests: open at a line in a small file, past the end, on a growing large file, with a saved filter hiding the line
- [x] 3.2 README (command line), `docs/tui.md`, CHANGELOG
- [x] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
