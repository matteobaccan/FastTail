## 1. Build

- [x] 1.1 Add `links` to the `eframe` feature list in `Cargo.toml` and refresh `Cargo.lock` (`webbrowser` appears)
- [x] 1.2 Add the regression test in `tests/integration_tests.rs` that reads `Cargo.toml` via `include_str!` and asserts the `eframe` line contains `"links"`, with a message naming the feature

## 2. About dialog

- [x] 2.1 Add `on_hover_text` with the full URL to the three `hyperlink_to` calls in `src/ui/app.rs` (header website link, Website row, Repository row)

## 3. Verification and docs

- [x] 3.1 `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`; run the release build, open About and click both links on Windows (browser opens); confirm no `Cannot open url` warning on stderr
- [x] 3.2 CHANGELOG `[Unreleased]` entry under Fixed
