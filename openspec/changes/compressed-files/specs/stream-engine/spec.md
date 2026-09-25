## ADDED Requirements

### Requirement: Compressed Log Input
The engine SHALL open gzip files (magic bytes `1f 8b`, including multi-member files) and zip archives (magic bytes `50 4b 03 04`) read-only, whatever their extension, by decompressing them on a background thread into a temporary spool file that is then accessed like any other file, so that no decompressed copy is held in memory and every text, HEX and Markdown feature works on the result. Content SHALL become visible while decompression runs, and the stream bar SHALL show the decompression progress with a cancel action. A zip with one file entry SHALL open that entry directly; a zip with several SHALL show an entry picker from which one or more entries are opened, each as its own stream. Zip entries that are encrypted or use a method other than stored or deflate, and tar archives found inside a gzip file, SHALL be refused with a message naming the reason. A compressed stream SHALL NOT follow the archive on disk: follow mode is disabled for it with an explanation. The stream's identity in the tab, the footer, the workspace, sessions, recent files and bookmarks SHALL be the archive path (and the entry name for zip), never the spool path, and a restored compressed stream SHALL be decompressed again in the background.

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

### Requirement: Decompression Space Guard
Before decompressing a zip entry the engine SHALL check that the spool volume has room for the entry's uncompressed size plus a 512 MB margin and refuse otherwise. During any decompression it SHALL re-check the free space at least every 64 MB written and stop when less than 512 MB would remain, and it SHALL stop when the output reaches the cap `compressed_max_gb` (default 20 GB, configurable in `fasttail.ini` and Settings). When decompression stops early, the lines already written SHALL stay readable and the stream SHALL say that its content is partial and why.

#### Scenario: Not enough disk space
- **WHEN** the user opens a zip entry of 40 GB uncompressed and the spool volume has 10 GB free
- **THEN** the entry is refused with a message naming the volume and the required size, and no spool file is created.

#### Scenario: Archive larger than the cap
- **WHEN** a gzip file inflates past the 20 GB cap
- **THEN** decompression stops at the cap, the first 20 GB of lines remain browsable, and the stream bar says the content is partial because the cap was reached.

### Requirement: Temporary Spool Lifecycle
Spool files SHALL be created in the directory `spool_dir` from `fasttail.ini` when set, otherwise in `fasttail-spool` under the system temporary directory, named with the owning process id. A spool file SHALL be deleted when its stream is closed or reloaded, all spool files of the process SHALL be deleted at normal exit, and at startup the application SHALL delete spool files whose owning process is no longer running, so that a crash does not leave decompressed copies behind. Closing a stream while it is being decompressed SHALL cancel the decompression.

#### Scenario: Closing the tab
- **WHEN** the user closes the tab of a decompressed `app.log.1.gz`
- **THEN** the decompression, if still running, stops and the spool file is removed from disk.

#### Scenario: Restart after a crash
- **WHEN** FastTail crashed with two decompressed streams open and is started again
- **THEN** the two orphaned spool files are deleted at startup, and the restored streams are decompressed into new spool files.
