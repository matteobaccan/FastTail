## 1. Read-side abstraction

- [x] 1.1 A merged view is a `TailEngine` on a spool (see design): no `StreamView` trait needed
- [x] 1.2 Tests: the merged engine is an ordinary stream (filters checked in `merged_views`)

## 2. Merged stream

- [x] 2.1 Add `src/merged_stream.rs` with sources, the order vector, incremental merge on append and rebuild on truncation, source toggles
- [x] 2.2 Implement `StreamView` for the merged stream, including filters, search and highlight evaluation over source lines
- [x] 2.3 Tests: ordering, live append from both sources, truncation rebuild, toggles, refusal without timestamps

## 3. UI and config

- [x] 3.1 "New merged view..." dialog listing open streams with timestamp status; source chips and tooltips in the marker column; source toggles in the stream bar
- [ ] 3.2 Persist merged views by source paths and recreate at startup
- [x] 3.3 i18n keys in all 16 languages; add them to the exhaustive i18n test

## 4. Docs

- [x] 4.1 README feature list and comparison table; CHANGELOG `[Unreleased]`

## 5. Wrap-up

- [ ] 5.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 5.2 Local preview exe for the maintainer before the release
- [ ] 5.3 After the release, archive the change so `merged-timeline` is created
