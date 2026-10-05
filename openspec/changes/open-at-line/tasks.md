## 1. Command line

- [ ] 1.1 `src/cli.rs`: `path:line[:col]` split only when unambiguous; `--line N`; unit tests (drive letters, existing names with `:`, missing files, `:0`, huge numbers)
- [ ] 1.2 `--help` and the error texts in every language

## 2. Landing

- [ ] 2.1 Window: the target goes through go-to after the stream opens; waits for the index; notice past the end
- [ ] 2.2 Terminal interface: the same (cursor on the line, follow paused)
- [ ] 2.3 Print mode: `--line N` starts the output at line N
- [ ] 2.4 Single-instance hand-off carries the line; an older instance ignores it

## 3. Tests and docs

- [ ] 3.1 Integration tests: open at a line in a small file, past the end, on a growing large file, with a saved filter hiding the line
- [ ] 3.2 README (command line), `docs/tui.md`, CHANGELOG
- [ ] 3.3 `cargo fmt`, clippy, focused tests; PR with Linux and Windows CI green
