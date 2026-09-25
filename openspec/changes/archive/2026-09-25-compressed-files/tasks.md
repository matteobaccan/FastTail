## 1. Dependencies and spool

- [x] 1.1 Add `flate2` (default `rust_backend`) and `zip` 8 (`default-features = false, features = ["deflate-flate2"]`, minimum Rust 1.88) to `Cargo.toml`; record the release `fasttail.exe` size before and after
- [x] 1.2 Add `src/spool.rs`: spool directory (`<spool_dir>/fasttail-spool` or `<temp>/fasttail-spool`, 0700 on Unix), `<pid>-<counter>-<name>` naming, a `SpoolFile` handle that deletes the file on drop, `sweep()` removing spools of processes that are not running
- [x] 1.3 Tests: naming and sanitising, delete on drop, sweep removes a spool with a dead pid and keeps one with the current pid

## 2. Decompression job

- [x] 2.1 Add `src/compressed.rs`: `sniff` (gzip / zip / none), zip entry listing (name, size, method, encrypted), the job thread (`MultiGzDecoder` or zip entry reader → spool in 1 MB chunks with flush), progress by compressed bytes, cancel flag, tar detection (`ustar` at 257)
- [x] 2.2 Guard: pre-check for zip sizes, free-space re-check every 64 MB, `compressed_max_gb` cap, partial-content state on abort
- [x] 2.3 Tests: gzip round trip, multi-member gzip, zip single and multiple entries, stored and deflated entries, unsupported method and encrypted entry refused, tar refused, cap reached leaves a partial spool, cancel stops the job

## 3. Engine integration

- [x] 3.1 Open path: sniff before the binary heuristic; a compressed file opens an engine on its spool with `path` = archive, `current_file` = spool, origin `Compressed { archive, entry, complete }`
- [x] 3.2 Re-run encoding and view-mode detection once when a stream opened empty first holds 512 bytes (or on job completion)
- [x] 3.3 Follow forced off and not watched for compressed streams; a ⟳ button in the stream bar re-extracts
- [x] 3.4 Tests: filters, search, levels and bookmarks on a decompressed stream match the same content opened uncompressed; a UTF-16 entry is detected after the first chunk

## 4. UI

- [x] 4.1 Stream bar: `decompressing N%` with a cancel button, partial-content notice, "not supported" and "not enough space" notices
- [x] 4.2 Zip entry picker: filter box, name / size columns, multi-select, disabled entries with reason
- [x] 4.3 Follow toggle disabled with tooltip; tab title and tooltip name the archive (and entry)
- [x] 4.4 Settings: spool directory and the output cap

## 5. Persistence

- [x] 5.1 `fasttail.ini`: `spool_dir`, `compressed_max_gb`; `StreamEntry.archive_entry` in the workspace and session files; recent files and bookmarks keyed by archive (+ entry)
- [x] 5.2 Restore re-extracts in the background; call `spool::sweep()` at startup and delete own spools at exit
- [x] 5.3 Tests: config and session round trip with `archive_entry`; an old config without the keys loads with defaults

## 6. i18n and docs

- [x] 6.1 New keys (progress, cancel, entry picker, notices, settings labels, follow tooltip) in all 16 languages; i18n coverage test passes
- [x] 6.2 README: compressed logs in the feature list and the comparison table
- [x] 6.3 CHANGELOG `[Unreleased]` entry, including the measured binary-size delta
