## 1. Engine and throttle

- [ ] 1.1 `notify` on `HighlightRule` / `CompiledHighlight`; `[highlight_N] notify` read and written (default `false`, old files load unchanged)
- [ ] 1.2 `collect_notify_hits(start_idx)` on appended ranges only, queue capped at 64 per engine with overflow count; not called on initial index, reload, rotation, re-decode, restore
- [ ] 1.3 `src/notifications.rs`: per-rule 10 s throttle with `+N more`, summary after a quiet period, global 3 per 10 s, visibility rule (`background` / `always`)
- [ ] 1.4 Tests: no notification for lines present at open; three hits in 1 s give one notification then `+2 more`; global limit across rules; `always` vs `background`; disabled rule and disabled switch send nothing

## 2. Platform back end and UI

- [ ] 2.1 Add `notify-rust`; worker thread with channel; Linux default action and macOS click routed back to the app; Windows AppUserModelID registration with `winreg`
- [ ] 2.2 Back end unavailable: reason shown next to the Notify checkbox and in Settings
- [ ] 2.3 Notify checkbox in the rule editor next to the sound preset
- [ ] 2.4 Settings: `notifications_enabled`, `notify_when`, "Test notification" button
- [ ] 2.5 Click handling: focus window, activate tab, scroll to the line when it still exists

## 3. Texts and documentation

- [ ] 3.1 New i18n keys (checkbox, settings, test text, `+N more`, summary, unavailable reason) in all 16 languages; add them to the exhaustive i18n test
- [ ] 3.2 README (alerts section, comparison table), `docs/external-tools-cookbook.md` note that `notify-send` recipes are no longer needed for simple alerts, CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `rule-notifications` becomes a spec
