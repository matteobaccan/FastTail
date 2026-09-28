## Context

A followed local file is a `TailEngine` over a `FileSource` (`src/file_source.rs`), polled
and woken by `notify`. Sources that cannot be read at random go through a spool: the stdin
stream (`src/stdin_source.rs`) runs a detached copier thread that reads 64 KB at a time,
writes and flushes each chunk into a `SpoolFile` (`src/spool.rs`), calls the engine's
`WakeFn`, and restarts the spool from empty at `stdin_spool_max_mb` or when the volume
keeps less than 512 MB (`Limits`, `RestartReason`); the engine is opened on the spool with
the pseudo-path `<stdin>` as identity and owns the stream, so closing the tab deletes the
spool. Pattern streams (`src/wildcard.rs`, `TailEngine::open_pattern`) rescan a directory
every 2 s and switch to the newest match. Sessions (`src/session.rs`) store each stream's
`path` (plus `rel` when under the session folder) and per-stream view state; external
tools already quote arguments for `sh` (`external_tools::quote_sh_arg`). There is no async
runtime and no network code in the binary today.

## Goals / Non-Goals

**Goals:** remote files as ordinary streams; the user's own SSH setup (keys, agent,
config, jump hosts) with no credential ever handled by FastTail; resumable follow across
dropped connections; globs; several hosts; persisted in sessions; bounded disk, process
and thread use. **Non-goals:** see proposal.

## Decisions

### D1. The system OpenSSH client, not an SSH library

FastTail spawns `ssh` (Windows 10 1809+ ships it in `C:\Windows\System32\OpenSSH`; every
Linux and macOS desktop has it) with a fixed option set and a remote `tail` command, and
reads its standard output.

| Option | Verdict | Why |
|---|---|---|
| System `ssh` | **chosen** | Reuses the user's `~/.ssh/config`, aliases, `ProxyJump`, certificates, `known_hosts`, the OpenSSH agent and on Windows the OpenSSH agent service; no crypto in FastTail, no new crate, security fixes come with the OS; the only thing users must already have is a working `ssh host` |
| `russh` (pure Rust) | rejected | Needs `tokio` (no async runtime in FastTail today, ~1–2 MB), and FastTail would have to re-implement `ssh_config` parsing, `known_hosts` handling, agent protocols per platform (Pageant, the Windows named pipe, `SSH_AUTH_SOCK`) and `ProxyJump`: every gap is a "works in my terminal, not in FastTail" bug |
| `ssh2` (libssh2) | rejected | C library plus OpenSSL on Linux: a C cross toolchain for Linux ARM64, which the project avoided for compression too; same re-implementation problem as `russh` |
| SFTP (`sftp` subsystem, polling reads) | rejected for phase 1 | Works on servers without a shell, but following means polling `stat` + ranged reads every second over a second protocol; `tail -F` pushes lines at once and handles rotation on the server |

Cost of the choice: a host reachable only with a password cannot be used (open question
1), and behaviour depends on the installed OpenSSH version (7.6+ required, checked once
with `ssh -V` and reported in Settings → Remote).

### D2. Command line and remote script

Spawned as `ssh_program -T -n -o BatchMode=yes -o ConnectTimeout=10
-o ServerAliveInterval=15 -o ServerAliveCountMax=3 [-l user] [-p port] -- host <script>`,
with `CREATE_NO_WINDOW` on Windows (FastTail is a GUI-subsystem binary; no console must
flash). On Linux and macOS `-o ControlMaster=auto -o ControlPath=<spool_dir>/ssh-%C
-o ControlPersist=60` is added so glob rescans and several files on one host share one
connection; Windows OpenSSH does not support multiplexing and pays one handshake each.

The URL is parsed by `SshSpec::parse`: host must not start with `-` and must match
`[A-Za-z0-9._:\[\]-]+` (an `ssh_config` alias is fine), so it can never become an option
(`-oProxyCommand=…`); the path is percent-decoded, must be absolute, and is passed to the
remote shell single-quoted with `quote_sh_arg`. The script, one line of POSIX `sh`:

```
f=<quoted path>; set -- $(ls -iLnd -- "$f" 2>/dev/null); printf 'FASTTAIL1 %s %s\n' "$1" "$6" >&2; exec tail -c <start> -F -- "$f"
```

The `FASTTAIL1 <inode> <size>` marker on standard error gives the identity and size
before any data; `<start>` is `+<received+1>` when resuming, `+1` for the whole file, or
`<remote_initial_mb × 2^20>` for the tail. GNU coreutils, BusyBox, macOS and the BSDs all
have `tail -c` and `tail -F`. A missing `tail`, an unreadable file or a non-POSIX shell
end the child with a message taken from the last 4 KB of standard error.

When the fetch starts mid-file, the copier drops the bytes up to the first `\n`, so the
first row is a whole line.

### D3. Spool feed shared with stdin

`stdin_source::Copier` becomes `spool_feed::SpoolFeed` (reader → spool, flush per chunk,
wake, cap and low-disk restart, `InputState`, `RestartReason`); `StdinStream` and
`SshStream` both wrap it. The SSH feed adds a supervisor loop on the same worker thread:
spawn the child, read the marker, copy standard output, and on exit decide:

- child ended with the connection lost (exit 255) or read error → `reconnecting (n)`,
  back-off 1, 2, 5, 10 s then every 30 s, forever until the tab closes or the user presses
  Disconnect;
- on reconnect, same inode and remote size ≥ bytes received → resume with
  `tail -c +<received+1>`: nothing duplicated, nothing lost;
- otherwise (rotated or truncated while disconnected) → the spool restarts from empty and
  the initial fetch is done again; the stream bar says the remote file was replaced;
- authentication, host key or "no such file" errors → `disconnected: <reason>`, no
  automatic retry (retrying a refused key only fills the server's auth log).

A second thread per stream drains standard error (the marker, then a 4 KB ring of the last
messages), so a chatty `ssh -v` in the user's config never blocks the child.

### D4. Globs

A path whose last component holds `*` or `?` is a remote pattern stream. The directory is
quoted; in the file-name pattern each literal run is single-quoted and the wildcards stay
bare, so only `*` and `?` are ever interpreted by the remote shell. Listing runs
`cd -- <dir> && ls -td -- <pattern>` in a short separate `ssh` call, every 10 s (not 2 s:
each call is a handshake on Windows); a newer first entry switches the stream exactly as
`TailEngine::open_pattern` does locally (filters, rules, search, wrap kept; buffer and
bookmarks reset; "switched to" notice).

### D5. Identity, sessions, several hosts

The stream identity is the normalized URL (`ssh://user@host:port/path`, default port and
user omitted as typed) held in `TailEngine.path` like `<stdin>` is today, recognised by
`is_remote_path` (shared `SourceUri` helper with `system-sources`: whichever lands first
adds it). Title `host:file-name`; footer and tab tooltip show the full URL; `{file}` in
external tools expands to the URL. Sessions write `path=<url>` with no `rel`, plus
`remote_initial=tail|whole`; `source_exists` treats a URL as present (a failure shows in
the stream, not as "missing file"). Bookmarks of a remote stream are kept while the stream
lives but not persisted unless it was fetched whole, because line numbers of a tail fetch
depend on where the fetch started. The Remote dialog's host list opens one stream per
host, each an independent `SshStream`.

### D6. Threads, memory, files and bounds

UI thread: parses URLs, reads atomics for the stream bar; never waits on `ssh`. Workers:
2 threads per remote stream (feed/supervisor, stderr), 1 short-lived thread per glob
rescan. At most **32** `ssh` children run at once across FastTail; further streams wait in
`queued` state. Memory per stream: 64 KB read buffer, 4 KB stderr ring, the engine's index
(8 bytes per line) — the content is on disk in the spool, bounded by `remote_spool_max_mb`
(default 2048 MB) and the 512 MB free-space margin, with the same restart-from-empty rule
as stdin. A spool over 16 MB runs filters and search on workers as any file does. Spools
are deleted on close, at exit, and by the startup sweep after a crash; the control socket
directory is swept with them. Windows file sharing is unchanged (the spool is written by
FastTail only).

## Risks / Trade-offs

- [Users with password-only servers cannot use it] → the error says so and links the
  README section on `ssh-copy-id` / the agent; see open question 1.
- [`ssh` output differs across versions and locales] → only the exit status and the
  `FASTTAIL1` marker are parsed; stderr text is shown verbatim, never interpreted.
- [One handshake per glob rescan on Windows] → 10 s interval; rescans only while the
  stream is open.
- [Remote `ls` output of odd file names (newlines)] → names with control characters are
  skipped with a notice.
- [Resume after a rotation that kept the inode (copytruncate)] → caught by the size check
  (remote size < bytes received → refetch).

## Open Questions

1. Password and passphrase entry: FastTail could act as its own `SSH_ASKPASS` program
   (the exe re-invoked with `--askpass`, talking to the running instance) and never store
   the answer. Worth it for a phase 2, or is "use a key or the agent" enough?
2. Should a remote stream be openable from the recent-files list while offline, showing
   `disconnected` at once, or be greyed out until the host answers?
3. Is SFTP polling worth a fallback for servers without a shell (chrooted SFTP-only
   accounts)?
