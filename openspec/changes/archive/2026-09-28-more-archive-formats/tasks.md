## 1. Dependencies

- [x] 1.1 Add `tar` (0.4, `default-features = false`), `bzip2` (0.6, `libbz2-rs-sys` backend, no `bzip2-sys`), `lzma-rust2` and `ruzstd` to `Cargo.toml`, with a comment per crate as for `flate2`; check `cargo tree` has no `*-sys` C build for them and that `rust-version` still builds
- [x] 1.2 Confirm `lzma-rust2` reads concatenated xz streams and exposes the dictionary size before allocating; if not, switch to `lzma-rs` with a `Write`→spool adapter (design D1)
- [ ] 1.3 Record the release binary size before / after (Windows x86_64) in the PR; target under 1 MB added — not measured: no release build from before #126 was kept; to be recorded with the 0.12.0 release

## 2. Codec layer and sniffing

- [x] 2.1 `Codec { Gzip, Bzip2, Xz, Zstd }`, `open_decoder(codec, BufRead) -> Box<dyn Read + Send>` reading concatenated members / streams / frames and skipping zstd skippable frames
- [x] 2.2 `sniff` reads 512 bytes: bzip2 with the block / end-of-stream magic check, xz, zstd (incl. skippable), `ustar` at 257 → `Format::Tar`
- [x] 2.3 Decoder memory cap 256 MiB: read the xz LZMA2 dictionary size and the zstd `Window_Size` before decoding; refuse above it with a reason
- [x] 2.4 `Target` gains `Compressed(Codec)`, `TarArchive(Option<Codec>)` and a generic `Entry { archive, entry, kind }`; `classify` peeks ≤ 512 decoded bytes for a tar; `split_entry_path` / `source_exists` accept tar and compressed-tar ancestors
- [x] 2.5 `JobSource::Compressed(path, codec)` replaces `Gzip`; spool names strip `.gz/.gzip/.bz2/.xz/.zst/.zstd`; `StopReason::Tar` kept only as the fallback for a single file that turns out to be a tar, with reworded text
- [x] 2.6 Unit tests with fixtures built in the test (flate2, bzip2, lzma-rust2, ruzstd encoders): each codec round-trips; concatenated gzip / bzip2 / xz / zstd frames; `BZh1 text` stays plain; `trace.dat` holding xz is recognised; a zstd frame header declaring a 2 GiB window is refused; cap and cancel behave as for gzip

## 3. Tar scan and extraction

- [x] 3.1 `ArchiveEntryInfo { name, size, kind, data_offset, refusal }` shared by zip and tar; `EntryRefusal` gains `LinkOrSpecial` and `Sparse`; the unsafe-name and first-wins duplicate checks shared with zip
- [x] 3.2 `TarScan` worker: walks headers with `tar::Archive`, seeks over data for a plain tar, pushes rows into a shared snapshot, publishes compressed-bytes progress, stops on cancel, at 100 000 entries, at 1024 GB decoded, or on a damaged header (partial flag)
- [x] 3.3 In-memory scan cache keyed by (path, length, mtime)
- [x] 3.4 `JobSource::TarEntry`: plain tar seeks to the indexed header and verifies the name (else scans); compressed tar decodes from the start and skips earlier entries; `StopReason::NoSuchEntry` when the stream ends first; up-front space check when the size is known
- [x] 3.5 Nested decode: an entry (tar or zip) whose first 512 bytes sniff as a codec is wrapped once with `open_decoder`
- [x] 3.6 Tests with tar fixtures built by `tar::Builder` (plain, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.tar.bz2`): listing; `../`, absolute, symlink, hardlink, FIFO rows refused; directories hidden; duplicate names first-wins; GNU long name and pax path; entry `logs/app.log.1.gz` decoded; missing entry on restore; scan cap reached with the limits lowered in the test; damaged header ends with a partial list

## 4. UI, sessions and identity

- [x] 4.1 Generalise `src/ui/zip_picker.rs` into the archive picker: streaming rows from the scan snapshot, `scanning N%` with ✖, partial-list and damaged notices, open while scanning, auto-open of a single openable entry (only if nothing was opened or checked), scan stopped when the picker closes
- [x] 4.2 `src/ui/app.rs` open dispatch for the new targets (drag and drop, command line, recent files, workspace and session restore without the picker); `src/ui/dock.rs` new stop reasons
- [x] 4.3 Titles `bundle.tar.gz › logs/server.log`; sessions keep `archive_entry` for tar entries; round-trip test of a session with a tar entry and 2 bookmarks (pending bookmarks applied once indexed)

## 5. Texts and documentation

- [x] 5.1 New i18n keys (scan progress, stop scan, partial list, damaged archive, no openable file, link or special file, sparse entry, entry no longer in the archive, decoder window too large, reworded `compressed_tar`) in all 16 languages; add them to the exhaustive i18n test
- [x] 5.2 README "Compressed logs" section and FAQ: formats supported, tar picker and scan, nested entries, refusals; remove the statements that tar inside gzip is refused and that `.bz2` / `.xz` / `.zst` open as they are
- [x] 5.3 CHANGELOG `[Unreleased]`: bold opening sentence, before / after

## 6. Wrap-up

- [x] 6.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green; PR with Linux and Windows CI green (release workflow dispatch to confirm the ARM64 and macOS builds link)
- [ ] 6.2 Manual check with a real `sosreport`-style `.tar.xz` and a `journalctl`-rotated `.zst`; note throughput of a 1 GB xz and zst in the PR — deferred to the 0.12.0 preview round (the maintainer tries real archives with the preview exe)
- [x] 6.3 After the 0.12.0 release, archive the change so `stream-engine` gains the updated requirements
