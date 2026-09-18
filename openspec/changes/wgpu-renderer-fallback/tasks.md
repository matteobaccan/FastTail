## 1. Backend and Startup

- [x] 1.1 Enable the `wgpu` feature of `eframe` in `Cargo.toml`
- [x] 1.2 Add `src/renderer.rs` with `RendererChoice { Auto, Glow, Wgpu }` (parse/display, env > config > auto resolution) and `ActiveRenderer { kind, api, adapter, driver, fallback }` built from `CreationContext`
- [x] 1.3 In `main.rs`: resolve the choice, run glow first for `auto`/`glow`, retry with wgpu on `Err` for `auto` logging the error and `renderer: falling back to wgpu`, run wgpu directly for `wgpu`
- [x] 1.4 Add `renderer` to `FastTailConfig` with `to_ini` / `from_ini`
- [x] 1.5 Tests: choice parsing and resolution precedence, config round-trip, `ActiveRenderer` label and tooltip formatting

## 2. UI

- [x] 2.1 Store `ActiveRenderer` in `FastTailApp` from `new(cc)`; status bar chip `GL` / `WGPU` (+ `fallback`) with the adapter tooltip
- [x] 2.2 Renderer line in the About dialog
- [x] 2.3 Renderer selector in Settings with the "applies at next start" note
- [x] 2.4 i18n keys in five languages; extend the exhaustive i18n test

## 3. Verification

- [x] 3.1 `cargo test`, `cargo fmt --check`, `cargo clippy`
- [x] 3.2 Run locally with `FASTTAIL_RENDERER=wgpu` and with the default and check both chips
- [x] 3.3 Record in `design.md`: binary size before/after, cold and warm build time
- [x] 3.4 Build a Windows x86_64 release executable for the affected machine

## 4. Docs

- [x] 4.1 README: renderer section (fallback, env var, setting, chip)
