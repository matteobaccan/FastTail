## 1. Command line

- [ ] 1.1 `CliArgs.stdin`: `-` sets it before and after `--`; a second `-` is a usage error (exit 2); `-` no longer resolved to `<cwd>/-`; USAGE text documents `-`
- [ ] 1.2 Tests: `fasttail -`, `fasttail -- -`, `fasttail - -` error, `fasttail ./-` opens a file named `-`, `-` combined with `--filter` and paths

## 2. Spool and reader

- [ ] 2.1 Reuse or add `src/spool.rs` (see `compressed-files`): directory, `<pid>-<counter>-stdin` naming, delete on close and exit, startup sweep
- [ ] 2.2 Add `src/stdin_source.rs`: classification (Windows `GetStdHandle` + `GetFileType`; Unix `fstat` + `isatty`), reader thread (64 KB reads, write + flush, wake callback), EOF and error state, cap `stdin_spool_max_mb` and 512 MB free-space check with spool restart
- [ ] 2.3 Tests: classification of pipe, regular file and `/dev/null` on Unix; reader copies a synthetic producer's output and reports EOF; spool restart at a small cap is seen by the engine as a truncation; 50 MB/s producer keeps up

## 3. Engine and app

- [ ] 3.1 Origin `Stdin`: title `stdin`, footer text, skipped by workspace, recent files and session saving (listed in the save summary)
- [ ] 3.2 Re-run encoding and view-mode detection once a stream opened empty first holds 512 bytes (shared with `compressed-files`)
- [ ] 3.3 Startup: create the stream immediately with `-`, on first byte when detected without `-`; stderr message for `-` without piped input; `--filter` / `--exclude` / `--follow` applied
- [ ] 3.4 Stream bar notices: input ended (with line count), read error, earlier input discarded at the limit
- [ ] 3.5 Settings: stdin spool limit

## 4. i18n and docs

- [ ] 4.1 New keys (tab title, footer text, notices, setting label, session-save summary line) in all 16 languages; i18n coverage test passes
- [ ] 4.2 README: `producer | fasttail -` examples for bash, cmd, PowerShell and Git Bash, with the verified results from task 5 and the `cmd /c` fallback
- [ ] 4.3 CHANGELOG `[Unreleased]` entry

## 5. Manual verification on Windows (release build, GUI subsystem)

- [ ] 5.1 `type app.log | fasttail -` and `ping -t localhost | fasttail -` in cmd.exe: content arrives, prompt behaviour noted
- [ ] 5.2 Same in Windows PowerShell 5.1 and PowerShell 7.4+: pipe connected or not, non-ASCII text preserved or not, wait behaviour noted
- [ ] 5.3 Same in Git Bash; `fasttail < app.log` in cmd; auto-detection without `-`
- [ ] 5.4 Launch from Explorer, a shortcut and Windows Terminal without redirection: no stdin tab, no console window
- [ ] 5.5 Ctrl+C in the console ends the producer and FastTail shows "input ended"; closing the tab ends the producer with a broken pipe
- [ ] 5.6 Record the matrix in the PR description and in the README
