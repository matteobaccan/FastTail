## 1. Command line

- [x] 1.1 `CliArgs`: `print`, repeatable `filter` / `exclude` (up to 8 each with `--print`; without it no limit and the window uses the last one, as before), `regex`, `case_sensitive`, `level`, `since`, `until`, `context`, `color`, `line_numbers`, `no_prefix`; print-only options without `--print` are usage errors (not `--since` / `--until`, which set the window's time range per `relative-time-windows`); `USAGE` updated
- [x] 1.2 Parser tests: every option, `--opt=value` forms, repeated terms past 8, invalid level / colour / context, relative times through the shared `timestamp::parse_relative` (`now`, `-45s`, `-15m`, `-1h30m`, `-2d`, `-1w`; exact "to" side), `-` with and without FILEs

## 2. Pipeline

- [x] 2.1 `src/print_mode.rs` (no `ui` / `egui` / `eframe` import): input opening (plain, pattern newest match, single-file compressed via `open_decoder`, standard input), refusal of zip / tar with a message, encoding sniff, line splitting with the 1 MB long-line cap
- [x] 2.2 Line evaluation: ANSI strip for matching, `FilterSpec::visible_in_sequence`, level stage, time window with inherited timestamps and the popup's bare-time rule; context ring buffer with `--` separators
- [x] 2.3 Writer: plain, and colour with theme level styles, rules (captures-only spans), the line's own ANSI below them; truecolor or 256 colours; file prefix and line numbers; 64 KB buffer; quiet exit on broken pipe
- [x] 2.4 Follow loop: offsets and head fingerprints, `notify` plus 250 ms size check, truncation / rewrite notice and restart (held partial line flushed first, notice only once the file is open), pattern rescan every 2 s, `CTRL + C` ends with the exit code earned so far, standard input end, compressed input not followed
- [x] 2.5 Exit codes 0 / 1 / 2 / 3

## 3. Platform

- [x] 3.1 `main.rs`: branch to `print_mode::run` before the configuration is saved, the workspace restored, the spool swept or a window created; read-only configuration load honouring `--config`
- [x] 3.2 Windows: use redirected standard output / error handles as they are, attach and reopen `CONOUT$` only when missing, `ENABLE_VIRTUAL_TERMINAL_PROCESSING` for colour with fallback to none
- [ ] 3.3 Manual check on Windows (cmd, PowerShell, Git Bash; console, pipe and redirect) and Linux, recorded in the PR

## 4. Tests

- [x] 4.1 Integration tests running the built binary: filter, exclude, level, since / until (absolute, bare time, relative), context, several inputs with prefixes, `.gz` input, standard input, exit codes, no colour when not a terminal, `--color always` sequences
- [x] 4.2 Equivalence test: `--print` output equals `TailEngine::export_visible` with the same filters on the fixture logs (including stack traces and a time window)
- [x] 4.3 Follow test: append, truncation and pattern switch observed within 1 s

## 5. Documentation

- [x] 5.1 README: command-line section with `--print` examples (pipe, CI, last hour of errors), the Windows note (`start /wait` in an interactive console), FAQ entry; CHANGELOG `[Unreleased]`
- [x] 5.2 No i18n keys: command-line output stays in English like `--help`; confirm in the PR
- [ ] 5.3 Comment on the `tui-interface` proposal (PR #132) about reusing `print_mode` for `fasttail-tui --print` and the non-terminal fallback

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 6.2 Local preview exe for the maintainer before the 0.13.0 release
- [ ] 6.3 After the release, archive the change so `command-line` gains the new requirements
