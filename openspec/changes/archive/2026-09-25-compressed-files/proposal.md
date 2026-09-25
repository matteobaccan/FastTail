## Why

Rotated logs are usually compressed: logrotate writes `app.log.1.gz`, support bundles and CI artifacts arrive as `.zip`. Today FastTail detects such a file as binary and opens it in HEX view, so the user has to extract it by hand before reading it. klogg and lnav open compressed logs directly; FastTail should too, without giving up its rule that the file is never held in memory.

## What Changes

- Opening a `.gz` file (detected by its magic bytes `1f 8b`, not only by the extension) decompresses it in a background job into a temporary spool file; the normal tail engine opens and indexes the spool, so every feature (filters, search, levels, timestamps, bookmarks, export, HEX/MD) works unchanged.
- Opening a `.zip` file (magic `PK\x03\x04`) with a single file entry opens that entry the same way; with several entries an entry picker lists them (name, uncompressed size, filter box) and each chosen entry opens as its own stream.
- Content appears while decompression is still running; the stream bar shows `decompressing 37%` (compressed bytes consumed / compressed size) and a cancel button.
- A compressed stream is read-only and static: follow mode is disabled for it (the archive is not re-read when it changes on disk) and the toggle says why.
- Before and during extraction, a guard checks the free space of the spool volume and a configurable output cap (default 20 GB) to stop runaway archives; extraction stops with a clear message instead of filling the disk.
- Spool files live in `<temp>/fasttail-spool/` (with `spool_dir` in `fasttail.ini`, in the `fasttail-spool` subfolder of that directory), are deleted when the tab is closed and at exit, and spool files left by a crashed instance are swept at the next start.
- Workspace, sessions, recent files and bookmarks refer to the archive path (the entry path `<archive>/<entry>` for zip), never to the spool path; a restored compressed stream is decompressed again in the background.
- Supported: gzip (including multi-member files), zip entries stored or deflated (Zip64 included). Refused with a message: encrypted zip entries, other zip methods (bzip2, zstd, lzma), zip entries with unsafe names, tar archives inside gzip (`.tar.gz` / `.tgz`, recognised by the `ustar` magic of the decompressed data). Standalone `.bz2`, `.xz` and `.zst` files are not recognised and open as they are.

## Capabilities

### New Capabilities
- (none)

### Modified Capabilities
- `stream-engine`: adds compressed input (gzip, zip) via a temporary spool file, the extraction guard and spool cleanup.

## Impact

- New dependencies: `flate2` with the default pure-Rust `miniz_oxide` backend, and `zip` 8 with `default-features = false, features = ["deflate-flate2"]` (which reuses `flate2`). No C code, no new system library. `zip` 8 raises the minimum Rust version to 1.88. Binary-size impact is expected in the low hundreds of KB on the ~8 MB Windows executable; the PR SHALL measure it (release build before/after) and state the number in the CHANGELOG entry, as was done for the LTO choice.
- New `src/spool.rs`: spool directory, naming (`<pid>-<counter>-<name>`), cleanup on drop, startup sweep of spool files whose owning process is not running (via `sysinfo`, already a dependency).
- New `src/compressed.rs`: format sniffing, zip entry listing, the decompression job (thread + cancel flag + progress channel, same pattern as `scan_job`).
- `src/tail_engine.rs`: an engine opened on a spool records its origin (archive path, entry) as the stream identity; encoding and binary detection re-run once when a stream that opened empty first receives 512 bytes; follow is forced off when the source is complete.
- `src/ui/dock.rs` / `src/ui/app.rs`: entry picker dialog, progress and cancel in the stream bar, disabled follow toggle with tooltip, tab tooltip naming the archive.
- `src/config.rs` / `src/session.rs`: `spool_dir`, `compressed_max_gb`; `StreamEntry` gains an optional `archive_entry`.
- i18n: new keys in all 16 languages. README and CHANGELOG.
