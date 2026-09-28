## 1. Archive layer

- [x] 1.1 Add `sevenz-rust2` (default features off, needed coders only); check MSRV 1.88 and the release targets
- [x] 1.2 `Format::SevenZ` in `sniff_bytes`, `ArchiveKind::SevenZ`, `Target::SevenZArchive`, `entry_ancestor_kind`
- [x] 1.3 `list_7z_entries`: bounded encoded header (64 MB), 100 000 entries, refusals (encrypted, method, unsafe name, duplicate), block size per entry
- [x] 1.4 `JobSource::SevenZEntry` in `DecompressJob`: folder decode from the start, skip earlier entries, progress over the folder, cancel, cap, free-space checks, dictionary check, nested codec entry
- [x] 1.5 Tests with fixtures built in the test: non-solid and solid, LZMA2 and BZip2, encrypted entry, encrypted header, `../` name, dictionary above 256 MiB, nested `.gz` entry, session round trip with `entry=`

## 2. UI

- [x] 2.1 Entry picker lists 7z entries (single entry opens directly; disabled rows with reason; partial-list notice)
- [x] 2.2 Open dispatch, drag and drop, command line and session restore for 7z archives and entry paths

## 3. Texts and documentation

- [x] 3.1 New i18n keys (refusal reasons, partial list, encrypted header) in all 16 languages; add them to the exhaustive i18n test
- [x] 3.2 README (supported formats, comparison table) and CHANGELOG `[Unreleased]`

## 4. Wrap-up

- [ ] 4.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green
- [ ] 4.2 Local preview exe for the maintainer before the release
- [ ] 4.3 After the release, archive the change so `archive-7z` becomes a spec
