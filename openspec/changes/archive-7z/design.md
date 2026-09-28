## Context

`compressed.rs` sniffs the first 512 bytes, lists zip entries from the central directory
(`list_zip_entries`) and tar entries with a scan thread, and extracts one entry per
`DecompressJob` into a spool in 1 MB chunks, with cancel, output cap and free-space
checks. `ArchiveEntryInfo` / `EntryRefusal` feed the picker in `src/ui/zip_picker.rs`.
Entry paths `<archive>/<entry>` are split by `split_entry_path`, which asks
`entry_ancestor_kind` for the archive kind from its magic bytes.

A 7z file starts with a 32-byte signature header pointing to a header at the end of the
file, usually LZMA-compressed ("encoded header"). File data is stored in folders (solid
blocks): one folder may hold many files and is decodable only from its start.

## Goals / Non-Goals

**Goals:** open 7z entries with the behaviour, limits and identity of zip entries;
pure-Rust decoding (no C toolchain on the release targets); bounded memory.

**Non-Goals:** passwords, multi-volume archives, writing archives.

## Decisions

### D1. `sevenz-rust2`, default features off
Pure Rust, maintained, shares `lzma-rust2` with the xz reader. Features limited to the
coders listed in the proposal. *Alternative:* shelling out to `7z` — rejected: not
installed on most machines, and a process per entry breaks progress and cancel.

### D2. Listing like zip
The header is read at open time, as for zip (`list_7z_entries`). The encoded header is
decoded into memory with a 64 MB bound and 100 000 entries at most; beyond, the list is
partial with a notice. Refusals reuse `EntryRefusal`: `Encrypted` (entry in an AES
folder; an encrypted header refuses the whole archive), `Method(name)` for other coders,
`UnsafeName`, `DuplicateName`. Directories, anti-items and empty-stream entries are not
listed as files.

### D3. Solid blocks
An entry is extracted by decoding its folder from the start and skipping the bytes of the
earlier entries without writing them. Progress is the fraction of the folder's unpacked
size processed, so the percentage moves while skipping. Opening several entries of the
same folder runs one job each (each decodes the folder prefix again): simple, and the
common case is one or two entries. *Alternative:* one job writing several spools —
deferred, see open questions.

### D4. Limits reused
The dictionary check reuses `MAX_DECODER_WINDOW` (256 MiB) on every LZMA, LZMA2 and PPMd
coder of the entry's folder before any allocation. The space guard uses the entry size
from the header, as for zip.

### D5. Nested codec entries
An entry whose first bytes are gzip, bzip2, xz or zstd is decompressed once more through
`open_decoder`, exactly like a zip entry; a zip, tar or 7z inside an entry is not
unpacked.

## Risks / Trade-offs

- [Decoding a large solid block to reach its last entry is slow] → progress over the
  block and cancel; the picker tooltip shows the size of the entry's block.
- [A crafted header claims huge sizes or counts] → 64 MB and 100 000 entry bounds, space
  guard before extraction.
- [The new dependency's MSRV is above 1.88] → pin a compatible version or raise
  `rust-version` in the same PR.

## Open Questions

- Should opening several entries of one solid folder share one decode pass? Proposed: not
  in v1; revisit if users report slow multi-entry opens.
- Should the picker offer a password field for encrypted archives? Proposed: no, until
  requested.
