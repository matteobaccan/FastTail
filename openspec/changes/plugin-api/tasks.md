## 1. Manifest and discovery

- [x] 1.1 `src/plugins.rs`: discovery in `plugins/` next to `fasttail.ini` (the `--config` file's folder when given), `plugin.toml` parsing, `api = 1` check, errors per plugin
- [x] 1.2 Parameter model (`text`, `number`, `choice`, `secret`) and placeholder expansion per argument, (a small expander of its own: the external-tools one knows row placeholders only)
- [x] 1.3 Approval state: manifest SHA-256 in `[plugins]` of `fasttail.ini`; changed manifest disables the plugin
- [x] 1.4 Tests: valid and malformed manifests, unknown `api`, unknown keys as warnings, expansion without shell, hash change

## 2. Source plugins

- [x] 2.1 `src/plugin_source.rs`: child without shell, stdin closed, stdout into `spool_feed`, stderr 4 KB ring, working folder, `FASTTAIL_PLUGIN_API=1`
- [x] 2.2 Restart policy (`never`, `on-exit` with back-off) and stream bar states; kill the child (process tree on Windows through `taskkill /T`, process group elsewhere; no job object, no new crate) on close and at exit
- [x] 2.3 `plugin://` identity: command line, workspace and sessions; secrets never stored and asked on restore; missing or disabled plugin restored with a notice
- [x] 2.4 Tests with a small test plugin (a script printing lines, exiting, failing): lines appear, restart, stderr shown, secret not persisted, unapproved plugin not started

## 3. Format bundles (after `custom-log-formats`)

- [x] 3.1 The format loader also reads `*.fasttail-format.ini` of enabled plugins, read-only, named `<plugin>/<format>`
- [x] 3.2 Log formats dialog: bundled formats marked with their plugin, "Duplicate" to the user's `formats` folder; parser menu lists them
- [x] 3.3 Tests: bundle loaded and detected, samples mismatch refused with the plugin's error, disabled plugin's formats not loaded, name clash with a user format

## 4. UI

- [x] 4.1 Open menu Plugins submenu and parameter dialog (the 🔌 toolbar menu, next to the other source buttons)
- [x] 4.2 Settings ▸ Plugins: list with state and error, enable with approval dialog showing the command, disable, open folder, reload

## 5. Texts and documentation

- [x] 5.1 `docs/plugins.md`: manifest reference, one source example and one format example, security notes
- [x] 5.2 New i18n keys (menu, dialogs, states, errors) in all 16 languages; add them to the exhaustive i18n test
- [x] 5.3 README (plugins section, comparison table) and CHANGELOG `[Unreleased]`

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 6.2 Local preview exe for the maintainer before the release
- [ ] 6.3 After the release, archive the change so `plugin-api` becomes a spec
