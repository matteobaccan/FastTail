## 2026-04-18 - Validate regular file metadata before truncating export target
**Vulnerability:** Calling `File::create` directly when exporting logs truncates files at open time. If the export target path points to a non-regular file (such as a FIFO/named pipe or device node), opening in write-only/truncate mode can block the GUI thread indefinitely or truncate non-regular files.
**Learning:** `File::create` opens with `create(true).write(true).truncate(true)`. To safely export to a file, open the target with `OpenOptions` without `truncate(true)`, verify `file.metadata()?.is_file()`, and only then truncate with `file.set_len(0)`.
**Prevention:** Avoid opening write targets directly with truncation flags (`File::create`) when export targets can be non-regular files; validate `file.metadata().is_file()` on the opened OS handle before calling `set_len(0)`.

## 2026-04-18 - Enforce regular file validation on file open and reopen
**Vulnerability:** Opening non-regular files (e.g. directories, FIFOs/named pipes, device nodes, sockets) can lead to Denial of Service (DoS) via thread blocking, infinite loops, or excessive memory allocation when background scan threads attempt to stream data.
**Learning:** Checking file metadata prior to opening a file path leaves a potential Time-of-Check to Time-of-Use (TOCTOU) race condition if the path target changes between check and open. Validating `metadata.is_file()` on the opened `File` handle directly in `FileSource::open` and `FileSource::reopen` eliminates the race condition and ensures defense in depth across all read paths.
**Prevention:** Always inspect file metadata directly on the opened OS handle (`file.metadata()`) rather than checking paths on disk prior to `open`.
