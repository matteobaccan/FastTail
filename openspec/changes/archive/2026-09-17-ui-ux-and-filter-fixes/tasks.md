## 1. Visuals & Layout Ergonomics

- [x] 1.1 Configure zero expansion for hover and active widget visuals in `src/theme.rs`
- [x] 1.2 Enable draggable centered modal dialogs in `src/ui/app.rs` using `.pivot(CENTER_CENTER).default_pos(...)`
- [x] 1.3 Remove redundant in-tab file banner from `render_log_stream` in `src/ui/dock.rs`
- [x] 1.4 Implement dynamic active file status footer in `src/ui/app.rs` displaying full absolute path
- [x] 1.5 Calculate virtual scroll row height proportionally from font metrics without a 16px minimum floor in `src/ui/dock.rs`

## 2. Engine & Persistence Fixes

- [x] 2.1 Fix `is_line_visible_filtered` in `src/tail_engine.rs` to not filter out lines when include and exclude filters are empty
- [x] 2.2 Add `from_name` helper to `SoundAlertPreset` in `src/audio.rs`
- [x] 2.3 Implement `sound_alert` persistence in `to_ini` and `from_ini` in `src/config.rs`

## 3. Verification & Build

- [x] 3.1 Add integration tests for highlight rule sound alert INI persistence
- [x] 3.2 Add integration tests for filtered view empty filter semantics and proportional row heights
- [x] 3.3 Run all unit and integration tests to verify zero regressions
- [x] 3.4 Build standalone Windows release executable (`fasttail.exe`)
