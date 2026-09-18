## 1. Parser

- [ ] 1.1 Add `src/cli.rs` with `CliArgs` and a hand-written parser (positional paths, the listed options, `--` terminator), usage text, exit codes
- [ ] 1.2 Tests: every option, combined options, unknown option, `--` handling, relative path resolution

## 2. Startup integration

- [ ] 2.1 Parse in `main.rs` before the viewport; attach the parent console on Windows for `--help` / `--version`; honour `--config`
- [ ] 2.2 Pass `CliArgs` to `FastTailApp::new`: `--fresh`, `--session`, open paths deduplicated, apply `--filter` / `--exclude` / follow flags to those streams
- [ ] 2.3 Test through the app constructor with a temp config that CLI paths are opened and filters applied

## 3. Docs

- [ ] 3.1 README usage section replacing the current one-line claim
