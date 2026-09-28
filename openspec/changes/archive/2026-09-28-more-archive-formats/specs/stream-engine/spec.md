## MODIFIED Requirements

### Requirement: Compressed Log Input
The engine SHALL open, read-only and whatever their extension, gzip files (magic bytes `1f 8b`, including multi-member files), bzip2 files (`BZh` and a digit `1`–`9` followed by the block magic `31 41 59 26 53 59` or the end-of-stream magic `17 72 45 38 50 90`, including concatenated streams), xz files (`FD 37 7A 58 5A 00`, including concatenated streams), zstd files (`28 B5 2F FD`, or a skippable frame `50`–`5F 2A 4D 18`, including several frames), zip archives (magic bytes `50 4b 03 04`) and tar archives (`ustar` at byte offset 257), plain or inside a gzip, bzip2, xz or zstd file, by decompressing them on a background thread into a temporary spool file that is then accessed like any other file, so that no decompressed copy is held in memory and every text, HEX and Markdown feature works on the result. The format SHALL be decided from at most the first 512 bytes of the file and, for a gzip, bzip2, xz or zstd file, at most the first 512 decompressed bytes. Content SHALL become visible while decompression runs, and the stream bar SHALL show the decompression progress with a cancel action. A zip with one file entry SHALL open that entry directly; a zip with several SHALL show an entry picker from which one or more entries are opened, each as its own stream; a tar archive SHALL show the entry picker described in Tar Archive Entry Picker; a zip without file entries SHALL be reported with a notice, and a file starting with the zip magic bytes whose central directory cannot be read SHALL open as a plain file. An archive entry (zip or tar) whose first bytes are gzip, bzip2, xz or zstd SHALL be decompressed once more into the spool; a zip or tar inside an entry SHALL NOT be unpacked. Zip entries that are encrypted or use a method other than stored or deflate, tar entries that are symbolic links, hard links, character or block devices, FIFOs or sparse files, and entries of either kind whose name is absolute, has a drive prefix or climbs out of the archive (`../`), SHALL be refused with a message naming the reason, and no file SHALL ever be created at a path taken from an archive. When several entries have the same stream path, the first in archive order SHALL open and the others SHALL be refused. A compressed stream SHALL NOT follow the archive on disk: follow mode is disabled for it with an explanation, and neither `--follow` nor a restored follow state turns it on; a re-extract button in the stream bar SHALL decompress the archive again. The stream's identity in the tab, the footer, the workspace, sessions, recent files and bookmarks SHALL be the archive path for a single compressed file and the entry path `<archive>/<entry>` for a zip or tar entry (a session stores it as the archive path plus `entry=`), never the spool path; the tab title of an entry SHALL be `<archive name> › <entry>`. A restored compressed stream SHALL be decompressed again in the background, its bookmarks being applied once the index covers them; a restored tar entry that the archive no longer holds SHALL stop with a message saying so.

#### Scenario: Rotated gzip log
- **WHEN** the user opens `app.log.1.gz`, a 300 MB gzip of a 3 GB text log
- **THEN** the first lines appear within a second, the stream bar shows `decompressing` with a rising percentage, filters and search work on the lines already available, and the follow toggle is disabled with a tooltip explaining that the file is a compressed snapshot.

#### Scenario: Support bundle with several logs
- **WHEN** the user opens `bundle.zip` holding `server.log`, `worker.log` and `config/`
- **THEN** an entry picker lists `server.log` and `worker.log` with their sizes, and choosing both opens two streams titled `bundle.zip › server.log` and `bundle.zip › worker.log`.

#### Scenario: Unsupported zip entry
- **WHEN** a zip entry is compressed with zstd or is encrypted
- **THEN** the entry is shown disabled in the picker with the reason, and nothing is written to the spool for it.

#### Scenario: Compressed file without the usual extension
- **WHEN** a gzip file named `trace.dat` is opened
- **THEN** it is recognised by its magic bytes and opened decompressed instead of in HEX view.

#### Scenario: xz and zstd rotated logs
- **WHEN** the user opens `syslog.2.xz` and `journal.zst`, the second made of 3 concatenated zstd frames
- **THEN** both open decompressed with progress, and the zstd stream contains the lines of all 3 frames in order.

#### Scenario: Text that looks like bzip2
- **WHEN** a text file starting with `BZh1 is a label` is opened
- **THEN** it opens as a plain text file, because the bytes after `BZh1` are not a bzip2 block magic.

#### Scenario: Nested gzip in a tarball
- **WHEN** the user opens the entry `logs/app.log.1.gz` of `bundle.tgz`
- **THEN** the stream titled `bundle.tgz › logs/app.log.1.gz` shows the decompressed text of that gzip, not its binary bytes.

#### Scenario: Tar entry session round trip
- **WHEN** a session saved with the stream `bundle.tar.gz › var/log/server.log` and 2 bookmarks is loaded
- **THEN** the session stores the archive path plus `entry=var/log/server.log`, the entry is extracted again in the background without showing the picker, and both bookmarks appear once the lines they point to are indexed.

#### Scenario: Unsafe and special tar entries
- **WHEN** a tar holds `../../etc/passwd`, `/abs.log`, a symbolic link `current.log` and a regular file `app.log`
- **THEN** only `app.log` can be opened, the other 3 entries are shown disabled with their reason, and no file other than the spool is created.

### Requirement: Decompression Space Guard
Before decompressing a zip entry, or a tar entry whose size the picker's scan already read, the engine SHALL check that the spool volume has room for the entry's uncompressed size (at most the output cap) plus a 512 MB margin and refuse otherwise. During any decompression it SHALL re-check the free space at least every 64 MB written and stop when less than 512 MB would remain, and it SHALL stop when the output reaches the cap `compressed_max_gb` (default 20 GB, 1 to 1024, configurable in `fasttail.ini` and Settings). An xz or zstd stream whose declared dictionary or window exceeds 256 MiB SHALL be refused before that memory is allocated. When decompression stops early, the lines already written SHALL stay readable and the stream SHALL say that its content is partial and why.

#### Scenario: Not enough disk space
- **WHEN** the user opens a zip entry of 40 GB uncompressed and the spool volume has 10 GB free
- **THEN** the entry is refused with a message naming the volume and the required size, and no spool file is created.

#### Scenario: Archive larger than the cap
- **WHEN** a gzip, bzip2, xz or zstd file inflates past the 20 GB cap
- **THEN** decompression stops at the cap, the first 20 GB of lines remain browsable, and the stream bar says the content is partial because the cap was reached.

#### Scenario: Oversized decoder window
- **WHEN** a zstd file declares a 2 GiB window (`--long=31`)
- **THEN** it is refused with a message naming the 256 MiB limit, and no spool file is written beyond the empty one.

## ADDED Requirements

### Requirement: Tar Archive Entry Picker
Opening a tar archive, plain or inside a gzip, bzip2, xz or zstd file, SHALL show the entry picker at once while a background thread reads the entry headers, without writing any decompressed data to disk; the picker SHALL list each entry as it is found (name, size, and the reason when it cannot be opened), show the scan progress as a percentage of the archive bytes read with a stop action, and allow opening the listed entries before the scan ends. Directories SHALL NOT be listed. The scan SHALL stop after 100 000 entries or after 1024 GB of decompressed data, whichever comes first, and the picker SHALL then say that the list is partial; a damaged header SHALL end the scan with the entries found so far and a notice. When the scan ends with exactly one entry that can be opened and the user has neither opened nor checked an entry, that entry SHALL open directly and the picker SHALL close; when it ends with none, the picker SHALL say that the archive holds no file that can be opened. Closing the picker SHALL stop the scan. A finished scan SHALL be reused while the archive's size and modification time are unchanged, for the lifetime of the process. Each chosen entry SHALL be extracted by its own background job into its own spool, reading a compressed tar from the start and skipping the data of the entries before it.

#### Scenario: Large compressed support bundle
- **WHEN** the user opens `sosreport.tar.xz`, 800 MB compressed with 4 000 entries
- **THEN** the picker appears within 500 ms with `scanning` and a rising percentage, entries appear as they are found, and choosing `var/log/messages` while the scan runs opens a stream titled `sosreport.tar.xz › var/log/messages`.

#### Scenario: Plain tar lists by seeking
- **WHEN** the user opens `logs.tar`, 10 GB uncompressed with 1 000 entries
- **THEN** the list is complete after reading only the 1 000 headers (the entry data is skipped by seeking, not read), and an opened entry is copied into its spool with progress as a percentage of its size.

#### Scenario: Tarball with one log
- **WHEN** the user opens `app.tgz` holding the directory `logs/` and the single file `logs/app.log`
- **THEN** once the scan ends, `logs/app.log` opens directly as `app.tgz › logs/app.log` and the picker closes.

#### Scenario: Scan bound on an archive bomb
- **WHEN** a 10 MB `.tar.gz` would decompress to 2 TB
- **THEN** the scan stops after 1024 GB decompressed, the picker says the list is partial and keeps the entries found, and nothing of that data is written to disk.
