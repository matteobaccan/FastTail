## 1. Listener

- [x] 1.1 No `windows-sys`: the eleven kernel32 functions are declared in `src/debug_output.rs` (design D6)
- [x] 1.2 `src/debug_output.rs`: create the DBWIN objects (local and `Global\`), single-owner detection, reader thread with 500 ms wake-up for close
- [x] 1.3 Writer thread: bounded queue (65 536) with dropped counter, ANSI decoding, pid → name cache (60 s), line format, CR/LF split with indented continuation lines, process filter, own pid excluded
- [x] 1.4 Spool through the shared spool feed (split from `stdin_source` if not yet done): cap, free-space margin, restart notice, delete on close and at exit
- [x] 1.5 Tests: format and splitting of messages, ANSI decoding, filter matching; an integration test (Windows CI) that calls `OutputDebugStringW` from a child process and reads the line; second-monitor refusal

## 2. UI and persistence

- [x] 2.1 "Debug Output…" open-menu entry and dialog (local / global, include / exclude, admin check), Windows only
- [x] 2.2 `dbwin://local` / `dbwin://global` on the command line; stream title, footer identity, follow always on, no recent-file entry
- [x] 2.3 Workspace and session keys (`dbwin_include`, `dbwin_exclude`); restore restarts the capture; Retry when the buffer is taken
- [ ] 2.4 Pause / resume of the capture in the stream bar (if the open question is accepted; not in this change: closing the tab releases the buffer)

## 3. Texts and documentation

- [x] 3.1 New i18n keys (menu, dialog, another monitor, admin needed, dropped messages) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README (sources section, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [x] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [x] 4.2 Local preview exe for the maintainer before the release (0.22.1 release builds and the terminal exe, OK on 2026-10-23)
- [x] 4.3 After the release, archive the change so `debug-output-capture` becomes a spec
