## 1. Manifest and discovery

- [ ] 1.1 `src/plugins.rs`: discovery in `plugins/` next to `fasttail.ini` (and the `--config` folder), `plugin.toml` parsing, `api = 1` check, errors per plugin
- [ ] 1.2 Parameter model (`text`, `number`, `choice`, `secret`) and placeholder expansion per argument, reusing the external-tools expander
- [ ] 1.3 Approval state: manifest SHA-256 in `[plugins]` of `fasttail.ini`; changed manifest disables the plugin
- [ ] 1.4 Tests: valid and malformed manifests, unknown `api`, unknown keys as warnings, expansion without shell, hash change

## 2. Source plugins

- [ ] 2.1 `src/plugin_source.rs`: child without shell, stdin closed, stdout into `spool_feed`, stderr 4 KB ring, working folder, `FASTTAIL_PLUGIN_API=1`
- [ ] 2.2 Restart policy (`never`, `on-exit` with back-off) and stream bar states; kill the child (process tree on Windows) on close and at exit
- [ ] 2.3 `plugin://` identity: command line, workspace and sessions; secrets never stored and asked on restore; missing or disabled plugin restored with a notice
- [ ] 2.4 Tests with a small test plugin (a script printing lines, exiting, failing): lines appear, restart, stderr shown, secret not persisted, unapproved plugin not started

## 3. Format bundles (after `custom-log-formats`)

- [ ] 3.1 The format loader also reads `*.fasttail-format.ini` of enabled plugins, read-only, named `<plugin>/<format>`
- [ ] 3.2 Log formats dialog: bundled formats marked with their plugin, "Duplicate" to the user's `formats` folder; parser menu lists them
- [ ] 3.3 Tests: bundle loaded and detected, samples mismatch refused with the plugin's error, disabled plugin's formats not loaded, name clash with a user format

## 4. UI

- [ ] 4.1 Open menu Plugins submenu and parameter dialog
- [ ] 4.2 Settings ▸ Plugins: list with state and error, enable with approval dialog showing the command, disable, open folder, reload

## 5. Texts and documentation

- [ ] 5.1 `docs/plugins.md`: manifest reference, one source example and one format example, security notes
- [ ] 5.2 New i18n keys (menu, dialogs, states, errors) in all 16 languages; add them to the exhaustive i18n test
- [ ] 5.3 README (plugins section, comparison table) and CHANGELOG `[Unreleased]`

## 6. Wrap-up

- [ ] 6.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 6.2 Local preview exe for the maintainer before the release
- [ ] 6.3 After the release, archive the change so `plugin-api` becomes a spec
