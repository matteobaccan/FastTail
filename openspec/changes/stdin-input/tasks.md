## 1. Command line

- [x] 1.1 `CliArgs.stdin`: `-` sets it before and after `--`; a second `-` is a usage error (exit 2); `-` no longer resolved to `<cwd>/-`; USAGE text documents `-`
- [x] 1.2 Tests: `fasttail -`, `fasttail -- -`, `fasttail - -` error, `fasttail ./-` opens a file named `-`, `-` combined with `--filter` and paths

## 2. Spool and reader

- [x] 2.1 Reuse or add `src/spool.rs` (see `compressed-files`): directory, `<pid>-<counter>-stdin` naming, delete on close and exit, startup sweep
- [x] 2.2 Add `src/stdin_source.rs`: classification (Windows `GetStdHandle` + `GetFileType`; Unix `fstat` + `isatty`), reader thread (64 KB reads, write + flush, wake callback), EOF and error state, cap `stdin_spool_max_mb` and 512 MB free-space check with spool restart
- [x] 2.3 Tests: classification of pipe, regular file and `/dev/null` on Unix; reader copies a synthetic producer's output and reports EOF; spool restart at a small cap is seen by the engine as a truncation; 50 MB/s producer keeps up (the Unix classification test is compiled on Unix only and was not run on this Windows machine; the Windows variant covers pipe, disk file, NUL and NULL/invalid handles)

## 3. Engine and app

- [x] 3.1 Origin `Stdin`: title `stdin`, footer text, skipped by workspace, recent files and session saving (listed in the save summary)
- [x] 3.2 Re-run encoding and view-mode detection once a stream opened empty first holds 512 bytes (shared with `compressed-files`) (already provided by `compressed-files`: `encoding_pending`; stdin settles it at end of input)
- [x] 3.3 Startup: create the stream immediately with `-`, on first byte when detected without `-`; stderr message for `-` without piped input; `--filter` / `--exclude` / `--follow` applied
- [x] 3.4 Stream bar notices: input ended (with line count), read error, earlier input discarded at the limit
- [x] 3.5 Settings: stdin spool limit

## 4. i18n and docs

- [x] 4.1 New keys (tab title, footer text, notices, setting label, session-save summary line) in all 16 languages; i18n coverage test passes (the tab title is the literal `stdin`, a constant, as the spec names it; the other nine keys are translated)
- [x] 4.2 README: `producer | fasttail -` examples for bash, cmd, PowerShell and Git Bash, with the verified results from task 5 and the `cmd /c` fallback
- [x] 4.3 CHANGELOG `[Unreleased]` entry

## 5. Manual verification on Windows (release build, GUI subsystem)

- [x] 5.1 `type app.log | fasttail -` and `ping -t localhost | fasttail -` in cmd.exe: content arrives, prompt behaviour noted (debug build: cmd waits for FastTail before the prompt returns; content and UTF-8 bytes arrive; `ping -t` is live)
- [x] 5.2 Same in Windows PowerShell 5.1 and PowerShell 7.4+: pipe connected or not, non-ASCII text preserved or not, wait behaviour noted (PowerShell 7.6: pipe connected, prompt returns at once, `Get-Content` input complete with UTF-8 kept, but native-to-native `ping -t | fasttail -` only delivered when the producer ended; Windows PowerShell 5.1: pipe connected, non-ASCII becomes `?` with the default `$OutputEncoding`, a UTF-8 BOM is prepended; `cmd /c` fallback keeps bytes and streams)
- [x] 5.3 Same in Git Bash; `fasttail < app.log` in cmd; auto-detection without `-` (Git Bash pipe and redirect keep bytes; `fasttail - < app.log` in cmd; auto-detection without `-` in cmd)
- [ ] 5.4 Launch from Explorer, a shortcut and Windows Terminal without redirection: no stdin tab, no console window — partial: `Start-Process` and `cmd /c start` (ShellExecute launches) give no standard input, so no stdin tab; Explorer, a desktop shortcut and Windows Terminal not checked directly
- [ ] 5.5 Ctrl+C in the console ends the producer and FastTail shows "input ended"; closing the tab ends the producer with a broken pipe — partial: a producer that ends (killed `ping -t`) shows `input ended · N lines`; Ctrl+C and the broken pipe on tab close were not checked by hand (synthetic input could not be sent to the window); the copier stopping and closing its input is covered by a unit test
- [x] 5.6 Record the matrix in the PR description and in the README (in the README and in design.md; the PR description is still to be written)
