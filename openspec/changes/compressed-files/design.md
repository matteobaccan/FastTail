## Context

The tail engine reads a regular file through `FileSource` (one shared read handle, an LRU cache of 64 × 64 KB blocks) and keeps only an 8-byte offset per line. Every feature — `get_line`, background `scan_job` threads that `read_direct` 1 MB chunks, HEX rows, rotation and rewrite detection — assumes random access to bytes at stable offsets. A gzip or deflate stream offers only sequential access: reaching offset N means inflating everything before it. `open_impl` also refuses anything that is not a regular file, and a `.gz` currently trips the binary heuristic and opens in HEX view.

## Goals / Non-Goals

**Goals:**
- Read `.gz` and `.zip` logs with every existing feature, without holding the decompressed data in memory.
- Show content early on big archives, with progress and cancellation.
- Never leave decompressed copies behind, even after a crash.
- Refuse cleanly what is not supported, and never fill the disk.

**Non-Goals:**
- Following a compressed file as it grows (a `.gz` being written is not a readable stream until it is closed).
- tar archives, bzip2, xz, zstd, 7z, rar; nested archives; encrypted zip entries.
- Writing or re-compressing anything.
- Random-access inflate (seek points / index of deflate state).

## Decisions

### D1. Decompress to a temporary spool file, then treat it as a normal file
The decompression job writes the inflated bytes to a spool file and the engine opens that file through the unchanged `FileSource`. Offsets, the block cache, scan jobs, HEX and export all keep working because the spool is a regular file.

*Alternatives:* (a) inflate into memory — breaks the core "never hold the file" rule and fails on multi-GB archives; (b) a seekable-deflate index (zran-style checkpoints every N MB, ~32 KB window each) — true random access without disk use, but every scan and every cache miss would inflate from a checkpoint, the scan jobs would need a second source type, and the work is far larger than the feature. The spool costs disk space equal to the uncompressed size, which the guard (D5) bounds.

### D2. The engine tails the spool while the job fills it
The engine is opened on the spool as soon as the job has created it. The job appends and flushes in 1 MB chunks; the engine's normal poll path indexes the appended bytes incrementally (cost proportional to the new bytes), so the first screen appears within the first chunk. Filters and search applied during extraction are evaluated on the appended lines exactly as for a growing log. The stream is marked `origin = Compressed { archive, entry, complete: bool }`; while `complete` is false the stream bar shows `decompressing N%`, computed from compressed bytes consumed over compressed size (the uncompressed size is unknown for gzip, see D5).

Because the spool starts empty, the encoding/binary sniff done in `open_impl` sees no sample. The engine therefore re-runs `detect_encoding` and `initial_view_mode` once, when a stream that was opened empty first holds 512 bytes (or the job completes with fewer). This is a small general fix: any log opened while empty benefits from it.

### D3. Follow mode is off and locked for compressed streams
Once `complete` is true nothing will ever be appended, so follow has no meaning; while extraction runs, jumping to the bottom on every chunk would make the view unreadable. The follow toggle is disabled with a tooltip ("compressed file: read-only snapshot"). The archive on disk is not watched: if it is replaced, the user reopens it (the tab context menu keeps "Reload", which re-extracts).

### D4. Sniff by magic bytes, pick entries in a dialog
`compressed::sniff(path)` reads the first 4 bytes: `1f 8b` → gzip, `50 4b 03 04` → zip (also `50 4b 05 06` for an empty zip, refused as empty). Extensions are not required, so `app.log.1.gz` and a `.zip` renamed `.dat` both work. For gzip, `MultiGzDecoder` handles concatenated members (what `cat a.gz b.gz` and some rotators produce). After the first decompressed 512 bytes, a `ustar` magic at offset 257 means a tar archive: the job stops and the stream shows "tar archives are not supported".

For zip, the central directory is read once (`zip::ZipArchive`), listing file entries (directories skipped) with their uncompressed size and method. One file entry opens directly. Several open the entry picker: a modal with a filter box, a sortable list (name, size), multi-select, "Open" — each chosen entry becomes its own stream and its own spool. Entries using an unsupported method or encryption are listed but disabled with the reason.

*Alternative:* open every entry of a zip as a tab — noisy for a support bundle with 200 files.

### D5. Guard: free space and an output cap
- Zip gives exact uncompressed sizes up front: before starting, the job checks that the spool volume has `size + 512 MB` free, else it refuses with "not enough space on <volume> for <size>".
- Gzip's trailer ISIZE is the size modulo 2^32, so it is only a hint. During any extraction, every 64 MB written the job re-checks free space (via `sysinfo::Disks`, already a dependency) and aborts when less than 512 MB would remain.
- Output cap `compressed_max_gb` in `fasttail.ini` (default 20, range 1–1024): extraction stops at the cap with a message. This bounds archives with extreme ratios (zip bombs).
- On abort the lines already spooled stay readable and the stream bar says the content is partial; the spool is deleted with the tab as usual.

### D6. Spool location, naming and cleanup
- Directory: `spool_dir` from `fasttail.ini` if set, else `std::env::temp_dir()/fasttail-spool/`. The setting exists because `%TEMP%` is often on a small system drive.
- Name: `<pid>-<counter>-<sanitised entry name>`; the pid identifies the owner.
- Deleted when the stream is closed (after the engine and the job have dropped their handles), when the stream is reloaded, and for all own spools at normal exit.
- At startup, `spool::sweep()` deletes spool files whose pid is not a running process (checked with `sysinfo`), which covers crashes and kills. A pid reused by an unrelated process only delays the cleanup to a later start.
- The spool is opened for writing with the same share mode as `open_file_shared` (read | write | delete) so the reader handle and deletion work on Windows.

*Alternative:* `FILE_FLAG_DELETE_ON_CLOSE` on Windows and unlink-after-open on Unix give crash-proof deletion, but the engine re-opens by path for rotation checks (`is_same_file_as_path`, `reopen`), which an unlinked file breaks on Unix; one sweep-based mechanism on all platforms is simpler to reason about and test.

### D7. Identity and persistence use the archive, not the spool
`TailEngine.path` stays the archive path (the stream's identity for the footer, recent files, bookmarks, external tools' `{file}` placeholder, the workspace and sessions); `current_file` is the spool. `StreamEntry` gains `archive_entry: Option<String>` for zip entries. Restoring a workspace with a compressed stream starts a new extraction in the background; bookmarks are keyed by archive path + entry and re-applied once the index covers their lines. The tab title shows the archive name (`app.log.1.gz`, `bundle.zip › server.log`).

### D8. Crates and size
- `flate2` with its default `rust_backend` (`miniz_oxide`): pure Rust, no zlib to link.
- `zip` with `default-features = false, features = ["deflate"]`, which inflates through `flate2`; no bzip2/zstd/lzma/aes code is pulled in, which is also why those methods are refused.
- The release profile uses thin LTO; the added code is expected to be a few hundred KB at most. The PR measures `fasttail.exe` before and after and records the delta.

## Risks / Trade-offs

- [Disk usage equals the uncompressed size] → free-space check, the output cap, `spool_dir` to move it, deletion on close.
- [Spool left behind if the machine loses power] → swept at the next start; a pid reused by a live process delays it.
- [Temp directory readable by other users on shared Unix hosts] → the spool directory is created with mode 0700 on Unix; on Windows `%TEMP%` is per-user.
- [A huge gzip takes minutes to inflate] → content is visible from the first chunk, progress is shown, cancel stops the job and keeps what was spooled.
- [Crate updates / supply chain] → both crates are widely used; Renovate already tracks dependencies.
- [Magic-byte sniffing misreads a text file starting with `PK\x03\x04`] → practically impossible for a text log; if the zip parse fails the file falls back to the normal open path.
