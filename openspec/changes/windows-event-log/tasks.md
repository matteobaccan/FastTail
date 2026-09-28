## 1. Groundwork

- [ ] 1.1 Reuse the shared spool feed (`spool_feed`) and non-file identity helpers; add them if no earlier source change did
- [ ] 1.2 `windows-sys` as a direct `cfg(windows)` dependency with `Win32_System_EventLog` and `Win32_Foundation`; check `cargo tree` adds no C build

## 2. Event Log worker

- [ ] 2.1 Channel enumeration with lazy enabled check; access-denied detection
- [ ] 2.2 History query (reverse, bounded, batched to disk above 10 000 events), bookmark, oldest-first write, then subscription after the bookmark (design D2)
- [ ] 2.3 Renderer: system values, formatted message with the publisher-metadata LRU (256), fallback to event data, continuation lines, 64 KB cap
- [ ] 2.4 Level mapping including audit keywords; unit tests of the mapping and of the line format against the level and timestamp detectors
- [ ] 2.5 XPath pre-filter builder (levels, up to 32 IDs or ranges); unit tests
- [ ] 2.6 `.evtx` detection by the `ElfFile\0` magic and file query, not followed; non-Windows notice
- [ ] 2.7 Integration test on Windows CI: write events with `ReportEventW` to a test source in Application (or read a checked-in small `.evtx` fixture), check history, live delivery without duplicates, and levels

## 3. UI and persistence

- [ ] 3.1 Event Log dialog: channel list with search, levels and IDs pre-filter, greyed unreadable channels with the reason
- [ ] 3.2 Stream bar: history progress, live state, "subscription lost, resubscribing" notice
- [ ] 3.3 Sessions, workspace and recent files store `eventlog://<channel>?levels=&ids=`; round-trip test; on non-Windows a restored entry is reported as unavailable, not dropped
- [ ] 3.4 `event_log_initial_events` in `fasttail.ini` and Settings (0–1 000 000)

## 4. Texts and documentation

- [ ] 4.1 New i18n keys in all 16 languages; exhaustive i18n test updated
- [ ] 4.2 README section "Windows Event Log", comparison table row, FAQ on Security and Event Log Readers
- [ ] 4.3 CHANGELOG `[Unreleased]`: bold opening sentence, before / after

## 5. Wrap-up

- [ ] 5.1 Manual check as a standard user: Application, System, PowerShell Operational live; Security refused with the reason; a 50 MB `.evtx`
- [ ] 5.2 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green on Linux and Windows CI
- [ ] 5.3 After the release, archive the change so `windows-event-log` becomes a spec
