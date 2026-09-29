## Why

FastTail opens gzip, bzip2, xz, zstd, zip entries and tar entries (plain or compressed),
but a `.7z` file opens in the HEX view. 7-Zip is the default archiver of many Windows
users and support teams ("zip the logs" often means "7z the logs"), and klogg opens 7z
archives. The post-0.12.0 competitor scan lists 7z among the small gaps (gap 20). The
spool, the entry picker, the space guard and the entry-path identity already exist for zip
and tar, so 7z is one more archive kind behind the same interface.

## What Changes

- **7z recognised by content** (`37 7A BC AF 27 1C`), whatever the extension, and opened
  like a zip: one file entry opens directly, several show the **entry picker** (name,
  size, reason when an entry cannot be opened), each chosen entry opens as its own stream
  titled `<archive> › <entry>`.
- Supported coders: copy, LZMA, LZMA2, BZip2, Deflate, PPMd, with the BCJ / BCJ2 / ARM
  and delta filters. **Solid** archives are supported: opening an entry decodes its solid
  block from the start and discards the entries before it, with progress over the block.
- **Refused with a reason**: encrypted entries or an encrypted header (no password
  prompt), other coders, unsafe names, duplicate stream paths; an LZMA / LZMA2 / PPMd
  dictionary above 256 MiB (the existing decoder window limit).
- The existing rules apply unchanged: no follow, re-extract button, space guard and
  output cap, spool lifecycle, nested codec entry decompressed once more, identity
  `<archive>/<entry>` in workspace, sessions (`entry=`), recent files and bookmarks.
- Listing is bounded: a header whose decoded size exceeds 64 MB, or more than 100 000
  entries, is listed partially with a notice.

Target release: **0.13.0** (basics and quick wins), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Password prompts and AES decryption.
- Multi-volume archives (`.7z.001`, `.7z.002`, ...).
- Creating or updating 7z archives; RAR or other formats.
- Following a 7z archive on disk (same rule as every compressed stream).

## Capabilities

### New Capabilities

- `archive-7z`: 7z detection, entry listing and picker, solid-block extraction, refusal
  rules and limits.

### Modified Capabilities

None. The zip / tar rules of `stream-engine` (Compressed Log Input, Decompression Space
Guard, Temporary Spool Lifecycle) apply as written; the new capability refers to them.

## Impact

- `Cargo.toml`: `sevenz-rust2` with default features off (no AES, no zstd / brotli / lz4
  coders); it decodes LZMA through `lzma-rust2`, already a dependency. MSRV 1.88 to check.
- `src/compressed.rs`: `Format::SevenZ`, `ArchiveKind::SevenZ`, `Target::SevenZArchive`,
  `list_7z_entries` (refusals, bounded header), `JobSource::SevenZEntry` in
  `DecompressJob`, `entry_ancestor_kind` for session and config loads, dictionary check.
- `src/ui/zip_picker.rs`: the picker lists 7z entries like zip entries (directory read at
  once, no scan thread).
- `src/ui/app.rs`: open dispatch for the new target; `src/session.rs` unchanged
  (`entry=` is already generic).
- `src/i18n.rs` (new refusal reasons in 16 languages), README (formats table), CHANGELOG.
- **TUI (0.20.0, PR #132)**: listing and extraction are UI-free in `compressed.rs`; the
  TUI needs only its own entry picker list.
