## Context

`src/compressed.rs` sniffs 4 bytes (`1f 8b` gzip, `PK\x03\x04` zip), classifies a path
into `Target::{Plain, Gzip, ZipArchive, EmptyZip, ZipEntry}` and runs a `DecompressJob`
thread that `pump`s a `Read` into a `SpoolFile` 1 MB at a time, enforcing the
`compressed_max_gb` cap and the 512 MB free-space margin (checked every 64 MB). A gzip
whose first 512 decompressed bytes carry `ustar` at offset 257 is stopped with
`StopReason::Tar`. Zip entries use the central directory (`list_zip_entries`) for the
picker (`src/ui/zip_picker.rs`) and an exact up-front space check. The stream path of an
entry is `<archive>/<entry>` (`entry_path` / `split_entry_path`), sessions store
`archive_entry`, restored streams re-extract and keep `pending_bookmarks`.

## Goals / Non-Goals

**Goals:** bzip2, xz, zstd single files; tar and compressed tar with an entry picker;
one level of nested compression in entries; same spool, limits, identity and session
model; pure-Rust dependencies only. **Non-goals:** see proposal.

## Decisions

### D1. Crates (pure Rust everywhere)

The release matrix (Windows x86_64, Linux x86_64 + ARM64, macOS ARM64) builds today with
no C code in the compression path (`flate2` on `miniz_oxide`). C backends (`zstd-sys`,
`lzma-sys`/`liblzma-sys`, `bzip2-sys`) would need a C cross-compiler for Linux ARM64 and
add build-script and static-link risk per target; decompression here is bounded by disk
and the 1 MB pump, not by codec speed.

| Format | Choice | Rejected | Why |
|---|---|---|---|
| tar | `tar` 0.4, `default-features = false` (drops `xattr`) | hand-written ustar parser | handles GNU long names, pax headers, base-256 sizes; MIT/Apache-2.0; streaming `Archive<R: Read>` |
| bzip2 | `bzip2` 0.6 with the `libbz2-rs-sys` backend, C `bzip2-sys` off | `bzip2-rs` (decoder only, less used); C `bzip2-sys` | pure-Rust port of libbzip2, same API incl. `MultiBzDecoder` (pbzip2 output); encoder available for test fixtures; MIT/Apache + bzip2 licence (BSD-style) |
| xz | `lzma-rust2` (`XzReader`, `Read`-based, has an encoder) | `xz2` / `liblzma` (C liblzma); `lzma-rs` (push-to-`Write` API, slower) | `Read` fits `pump` unchanged; Apache-2.0; if it lacks multi-stream or a memory limit at implementation time, fall back to `lzma-rs` behind a `Write`→spool adapter |
| zstd | `ruzstd` (`StreamingDecoder`, `Read`; has a compressor for fixtures) | `zstd` (C libzstd via `zstd-sys`) | pure Rust, MIT, ~3x smaller than linking libzstd; slower than C (hundreds of MB/s), acceptable against disk-bound spooling |

Binary size budget: the four crates together SHOULD add under 1 MB to the release
binary (checked in tasks; `cargo bloat` or size diff of `target/release/fasttail`).

### D2. Codec layer

`enum Codec { Gzip, Bzip2, Xz, Zstd }` and `fn open_decoder(codec, r: impl BufRead) ->
Box<dyn Read + Send>`; every decoder reads concatenated members/streams/frames
(`MultiGzDecoder`, `MultiBzDecoder`, xz multi-stream, a loop over zstd frames skipping
skippable frames). `sniff` reads 512 bytes (was 4):

- `BZh[1-9]` followed by `31 41 59 26 53 59` (block) or `17 72 45 38 50 90` (empty
  stream) — the 10-byte check keeps a text file starting with "BZh1" plain;
- `FD 37 7A 58 5A 00` xz; `28 B5 2F FD` or `5? 2A 4D 18` (skippable) zstd;
- `ustar` at 257 → `Format::Tar` (needs ≥ 262 bytes).

For a codec, `classify` decodes at most the first 512 output bytes on the calling (UI)
thread to see whether it holds a tar: at most a few ms and one decoder allocation (xz and
zstd allocate their window lazily up to D6's bound). `Target` becomes `Plain`,
`Compressed(Codec)`, `ZipArchive`, `EmptyZip`, `TarArchive(Option<Codec>)`,
`Entry { archive, entry, kind: ArchiveKind }`.

### D3. Tar scan (picker)

A compressed tar cannot be listed without decompressing it all. Opening a tar starts a
`TarScan` worker thread that walks headers with `tar::Archive::entries()`, skipping data
(plain tar: `seek` over it, so a 10 GB tar with 1 000 entries lists in 1 000 seeks;
compressed tar: decoded and discarded, nothing written to disk). It pushes
`ArchiveEntryInfo { name, size, kind, data_offset, refusal }` into an
`Arc<Mutex<Vec<_>>>` and publishes compressed bytes consumed (progress) in an atomic.
The UI thread only reads the snapshot each frame (picker rows appear as found, `scanning
N%` with a ✖). The user may open listed entries at any time; the scan keeps running until
it ends or the picker closes (cancel). When the scan ends with exactly one openable file
and nothing was opened yet, that entry opens and the picker closes.

Bounds: at most **100 000** entries listed (≈ 100 bytes each, ≤ 10 MB), and the scan
stops after **1 TB** decompressed (the `compressed_max_gb` maximum, 1024 GB) — past
either it stops and the picker says the list is partial. The zip picker becomes this
generic picker (zip rows arrive complete, with no scan).

The finished scan is cached in memory keyed by (archive path, length, mtime) for the
process lifetime, so reopening the picker does not rescan; a changed archive rescans.

### D4. Tar entry extraction

`JobSource::TarEntry { archive, codec, entry, data_offset }`, one job and one spool per
entry, as for zip:

- plain tar with a known offset: seek to `data_offset - 512`, verify the header names the
  entry (else fall back to a scan), copy `size` bytes; progress = bytes copied / size;
- compressed tar or no index (restored session): decode from the start, walk headers,
  discard data until the first header whose normalized name matches (`entry_key`), then
  pump that entry; progress = compressed bytes consumed. If the stream ends first:
  `StopReason::NoSuchEntry` ("the archive no longer holds this entry").

Opening k entries of a `.tar.gz` runs k independent passes on k threads — simple, each
reloadable on its own; a shared pass is a non-goal. When the size is known (index), the
same up-front space check as zip applies; otherwise the periodic check does.

Duplicate names: the **first** entry with a stream path opens and later ones are listed
disabled (`DuplicateName`), as in zip, so a streaming scan never has to revise a choice.
(`tar -r` "last wins" semantics are deliberately not followed.)

### D5. Nested compression and identity

`pump` peeks the first 512 bytes of an entry (tar or zip): if they sniff as a codec the
reader is wrapped once with `open_decoder`; a nested tar or zip is not unpacked. Spool
names strip `.gz/.gzip/.bz2/.xz/.zst/.zstd` so `notes.md.zst` still opens as Markdown.
Stream path `<archive>/<entry>`, title `bundle.tar.gz › logs/server.log`, session
`archive_entry` — unchanged scheme; `split_entry_path` accepts an ancestor that is a zip,
a tar, or a codec file holding a tar (sniffing the ancestor, ≤ 512 decoded bytes).

### D6. Security

- Entry names absolute, with a drive prefix, or containing `..` → `UnsafeName` (the
  existing zip check, shared). Names are only ever used as stream identity and spool-name
  suffix (sanitised by `spool_name`); nothing is created at an archive path.
- Tar types other than regular (`0`, `\0`, `7`) and directory: symlink, hard link,
  char/block device, FIFO → `LinkOrSpecial`; GNU sparse (`S`) → `Sparse`; all listed
  disabled. pax / GNU long-name headers are consumed by the `tar` crate.
- Decompression bombs: every output still stops at `compressed_max_gb`; the scan at 1 TB
  and 100 000 entries; decoder memory is capped at **256 MiB** per stream (xz dictionary
  from the LZMA2 properties, zstd `Window_Size` from the frame header) — above it the
  stream is refused before allocation. Nesting depth is 1.
- Malformed headers (bad checksum) end the scan with the entries found so far and a
  "damaged archive" note; they never panic.

### D7. Threads, memory, files

UI thread: `classify` (≤ 512 decoded bytes), picker rendering from snapshots. Workers:
one `TarScan` per open picker, one `DecompressJob` per stream. Memory: picker ≤ 10 MB;
per decoder ≤ 256 MiB window (typical: bzip2 ≈ 3.6 MB, xz -6 8 MB, zstd -19 8 MB) plus
the 1 MB pump buffer; nothing decompressed is held beyond that — the spool on disk holds
it and the engine indexes it as today (8 bytes per line). Archives are opened with
`open_file_shared` (Windows: read + write + delete sharing), are snapshots (not followed,
no watcher); an archive truncated or replaced during a job ends the job with `Failed`
and the partial content stays readable; ⟳ re-extracts.

## Risks / Trade-offs

- [Pure-Rust decoders slower than C] → decompression is disk/cap bound; measure a 1 GB
  xz and zst in tasks and record the throughput in the PR.
- [`lzma-rust2` is young] → fallback `lzma-rs` noted in D1; fixtures cover multi-stream
  and the memory cap.
- [k passes for k entries of a big `.tar.gz`] → CPU cost only, parallel threads;
  shared pass left for later if users ask.
- [UI-thread peek on a slow network share] → bounded to 512 decoded bytes, same as the
  gzip/zip sniff today.
- [First-wins duplicates differ from `tar -x`] → documented in the picker reason text.

## Open Questions

- Should a scan that ends with one entry auto-open it even if the user already typed in
  the picker filter? (Proposed: only if nothing was opened and no row was checked.)
- Is the 256 MiB decoder memory cap right, or should `--long=31` zstd (2 GiB window) be
  allowed?
