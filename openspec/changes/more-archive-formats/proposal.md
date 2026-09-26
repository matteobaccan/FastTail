## Why

Support bundles and rotated logs often arrive as `.tar.gz` / `.tgz` / `.tar`, `.bz2`,
`.xz` or `.zst`. Today FastTail opens only gzip and zip: a gzip holding a tar archive is
stopped with "tar archives are not supported", a bare `.tar` opens as one binary blob, and
`.bz2` / `.xz` / `.zst` files open as they are, in HEX view. The user has to extract the
bundle by hand, into a temporary folder they must remember to clean, before reading a
single line. Linux distributions rotate with xz or zstd and `sosreport` / `kubectl
cluster-info dump` style bundles are tarballs, so this is the most common "cannot open"
case left after 0.11.0.

## What Changes

- **Single-file codecs**: bzip2 (`BZh` + block magic), xz (`FD 37 7A 58 5A 00`) and zstd
  (`28 B5 2F FD`, and skippable frames) are recognised by content, whatever the name, and
  decompressed into a spool on a background thread exactly like gzip today (progress,
  cancel, re-extract, no follow, `compressed_max_gb` cap, free-space guard, partial
  content kept). Concatenated streams / frames / members are read as one stream.
- **Tar archives**: a plain tar (`ustar` at offset 257) and a tar inside any of the four
  codecs (`.tar.gz`, `.tgz`, `.tar.bz2`, `.tar.xz`, `.tar.zst`) open an **entry picker**
  like the zip one. Because a compressed tar can only be read front to back, the picker
  opens at once and fills in while a background scan walks the headers, with a progress
  percentage and a stop button; entries can be opened before the scan ends. When the scan
  finds exactly one openable file, it opens directly, as a one-entry zip does today.
- Tar entries that are symlinks, hard links, devices, FIFOs or sparse files, and entries
  whose name is absolute or climbs out (`../`), are listed disabled with the reason;
  nothing is ever written at a path taken from an archive.
- **Nested compression**: an archive entry (tar or zip) that is itself gzip, bzip2, xz or
  zstd is decompressed once more, so `bundle.tgz › logs/app.log.1.gz` shows text.
- Stream identity, titles and sessions reuse the zip scheme: the stream path is
  `<archive>/<entry>`, the title `bundle.tar.gz › logs/server.log`, a session stores the
  archive path plus `entry=`; restored tar entries are extracted again in the background
  and their bookmarks applied once indexed.
- The "refused" statements for tar and bz2/xz/zst are removed from the README; the stop
  reason `compressed_tar` is reworded (it remains only as a fallback).

Target release: **0.12.0**. No key is added to `fasttail.ini`: `compressed_max_gb` and
`spool_dir` apply unchanged.

### Non-goals

- Other containers: 7z, rar, cpio, ar, `.lz4`, `.lzma` (legacy LZMA-alone), `.Z`.
- Zip entries compressed with bzip2 / zstd / lzma inside the zip (still refused).
- Extracting archives to a user-chosen folder, or writing any file other than the spool.
- A shared decompression pass feeding several tar entries at once, and a persistent index
  of scanned archives across restarts.
- More than one level of nesting (a tar inside a tar entry, a zip inside a tar entry open
  as they are).

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `stream-engine`: "Compressed Log Input" gains bzip2, xz, zstd, tar and compressed-tar
  input, nested entry decompression and the tar entry refusals; "Decompression Space
  Guard" gains the decoder-memory and scan bounds.

## Impact

- `src/compressed.rs`: `Format` / `Target` / `JobSource` gain the new codecs and tar;
  a codec layer (`open_decoder`) shared by single files and entries; tar scan and tar
  entry extraction; `split_entry_path` accepts tar archives.
- `src/ui/zip_picker.rs` becomes the generic archive picker (scan progress, streaming
  rows); `src/ui/app.rs` open dispatch; `src/ui/dock.rs` stop reasons.
- `src/i18n.rs`: new strings in all 16 languages.
- New dependencies, all pure Rust (no C toolchain on any release target): `tar`,
  `bzip2` on the `libbz2-rs-sys` backend, `lzma-rust2`, `ruzstd` (see design).
- README, CHANGELOG, `openspec/specs/stream-engine/spec.md` at archive time.
