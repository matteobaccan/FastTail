## 1. Core

- [x] 1.1 Add `ExternalTool` to `FastTailConfig` (`[tool.N]` sections) with `to_ini` / `from_ini` and a round-trip test
- [x] 1.2 Add `src/external_tools.rs`: placeholder expansion per argument, `{match}` capture, spawn without shell (and shell mode), Windows `CREATE_NO_WINDOW`
- [x] 1.3 Rule-bound execution in the `poll_updates` alert hook with the 1/s throttle and 10-children cap; dropped counter
- [x] 1.4 Tests: expansion and quoting, `{match}`, throttle and cap (spawning a trivial command such as `cmd /c exit` or `true`)

## 2. UI

- [x] 2.1 External tools editor in Settings (list, fields, shortcut capture, rule binding, dropped counter)
- [x] 2.2 Row context menu (right click) and stream menu entries; shortcut dispatch on the current row
- [x] 2.3 i18n keys in five languages; i18n test

## 3. Docs

- [x] 3.1 README feature list with placeholder reference and the security note about shell mode
