## 2026-04-18 - Enforce regular file validation on file open and reopen
**Vulnerability:** Opening non-regular files (e.g. directories, FIFOs/named pipes, device nodes, sockets) can lead to Denial of Service (DoS) via thread blocking, infinite loops, or excessive memory allocation when background scan threads attempt to stream data.
**Learning:** Checking file metadata prior to opening a file path leaves a potential Time-of-Check to Time-of-Use (TOCTOU) race condition if the path target changes between check and open. Validating `metadata.is_file()` on the opened `File` handle directly in `FileSource::open` and `FileSource::reopen` eliminates the race condition and ensures defense in depth across all read paths.
**Prevention:** Always inspect file metadata directly on the opened OS handle (`file.metadata()`) rather than checking paths on disk prior to `open`.

## 2026-04-18 - Quote expanded arguments when spawning external tools in shell mode
**Vulnerability:** Running user-configured external tools in shell mode (`sh -c` / `cmd /c`) without quoting expanded placeholder values (`{line}`, `{selection}`, `{file}`) allows arbitrary command injection if log file content contains shell control characters (e.g., `;`, `&`, `|`, `$()`, quotes).
**Learning:** Even when shell execution is explicitly requested for an external command line, individual argument placeholders derived from untrusted inputs must be escaped or quoted specifically for the target shell (`sh -c` or `cmd /c`) before concatenation.
**Prevention:** Always pass untrusted placeholder values through POSIX (`quote_sh_arg`) or Windows CMD (`quote_cmd_arg`) argument quoter helpers when joining command lines for shell execution.
