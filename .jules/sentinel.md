## 2026-04-18 - Validate regular file metadata before truncating export target
**Vulnerability:** Calling `File::create` directly when exporting logs truncates files at open time. If the export target path points to a non-regular file (such as a FIFO/named pipe or device node), opening in write-only/truncate mode can block the GUI thread indefinitely or truncate non-regular files.
**Learning:** `File::create` opens with `create(true).write(true).truncate(true)`. To safely export to a file, open the target with `OpenOptions` without `truncate(true)`, verify `file.metadata()?.is_file()`, and only then truncate with `file.set_len(0)`.
**Prevention:** Avoid opening write targets directly with truncation flags (`File::create`) when export targets can be non-regular files; validate `file.metadata().is_file()` on the opened OS handle before calling `set_len(0)`.

## 2026-04-18 - Enforce regular file validation on file open and reopen
**Vulnerability:** Opening non-regular files (e.g. directories, FIFOs/named pipes, device nodes, sockets) can lead to Denial of Service (DoS) via thread blocking, infinite loops, or excessive memory allocation when background scan threads attempt to stream data.
**Learning:** Checking file metadata prior to opening a file path leaves a potential Time-of-Check to Time-of-Use (TOCTOU) race condition if the path target changes between check and open. Validating `metadata.is_file()` on the opened `File` handle directly in `FileSource::open` and `FileSource::reopen` eliminates the race condition and ensures defense in depth across all read paths.
**Prevention:** Always inspect file metadata directly on the opened OS handle (`file.metadata()`) rather than checking paths on disk prior to `open`.

## 2026-04-18 - Quote expanded arguments when spawning external tools in shell mode
**Vulnerability:** Running user-configured external tools in shell mode (`sh -c` / `cmd /c`) without quoting expanded placeholder values (`{line}`, `{selection}`, `{file}`) allows arbitrary command injection if log file content contains shell control characters (e.g., `;`, `&`, `|`, `$()`, quotes).
**Learning:** Even when shell execution is explicitly requested for an external command line, individual argument placeholders derived from untrusted inputs must be escaped or quoted specifically for the target shell (`sh -c` or `cmd /c`) before concatenation.
**Prevention:** Always pass untrusted placeholder values through POSIX (`quote_sh_arg`) or Windows CMD (`quote_cmd_arg`) argument quoter helpers when joining command lines for shell execution.

## 2026-04-18 - Always enforce restricted directory permissions on spool directory
**Vulnerability:** Bypassing permission updates (`set_permissions`) when a directory already exists (`dir.is_dir()`) allows temporary spool files containing log content to be exposed if the directory pre-existed with permissive access rights (e.g. 0755 or 0777 in `/tmp`).
**Learning:** `create_dir_all` in Rust returns `Ok(())` if the directory already exists. Bypassing `set_permissions(dir, 0o700)` via an early `if dir.is_dir()` check leaves existing or pre-created directories insecurely open to local users on multi-user systems.
**Prevention:** Do not early-return on directory existence when ensuring secure directory permissions; always apply `set_permissions(dir, 0o700)` on Unix.
