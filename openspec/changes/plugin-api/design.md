## Context

Sources today: files, pattern streams, compressed files and archive entries, standard
input. Standard input shows the model for anything that is not a file: a thread copies
bytes into a spool that the engine tails (`stdin_source.rs`), and `ssh-sources` design D3
splits that copier into a shared `spool_feed`. External tools (`external_tools.rs`)
already expand placeholders per argument and start programs without a shell.
`structured-fields` (0.14.0) introduces field parsers with detection and a column view;
`custom-log-formats` (0.14.0) adds named `*.fasttail-format.ini` files in a `formats`
folder, with level maps, timestamp formats, entry start patterns and samples.

## Goals / Non-Goals

**Goals:** let users add sources and formats without rebuilding FastTail, in any
language, without risking the app's stability or running code the user did not approve;
a small, versioned contract.

**Non-Goals:** in-process code, UI extensions, a registry.

## Decisions

### D1. Out-of-process sources
A source plugin is a program whose standard output is the log. This is the most portable
contract (any language, crash isolation, no ABI) and reuses the spool path of standard
input, so every stream feature works. *Alternatives:* dynamic libraries — rejected (no
stable Rust ABI; C ABI plugins are hard to write safely and crash the app); WASM with
`wasmtime` — rejected for v1 (large dependency, a host API to design for I/O and
networking); `wasmi` could revisit it (open question).

### D2. Manifest `api = 1`
```toml
api = 1
name = "kafka-topic"
version = "0.1.0"
kind = "source"
[source]
command = "kcat"
args = ["-C", "-b", "{broker}", "-t", "{topic}", "-o", "end", "-u"]
restart = "on-exit"
[[source.param]]
id = "broker"; label = "Broker"; type = "text"; default = "localhost:9092"
[[source.param]]
id = "topic"; label = "Topic"; type = "text"
```
Placeholders are the parameter ids, expanded once per argument as external tools do;
`{config_dir}` and `{plugin_dir}` are also available. The command is resolved relative
to the plugin folder first, then `PATH`. Unknown keys are warnings, unknown `api` is an
error.

### D3. Formats are `custom-log-formats` files, bundled
A plugin folder's `*.fasttail-format.ini` files are loaded by the `custom-log-formats`
loader as read-only formats named `<plugin>/<format>`, with the same validation (samples
must match) and detection; the Log formats dialog shows them as bundled and allows
"Duplicate" into the user's own `formats` folder to edit. A format bundle loads when its
plugin is enabled; `kind = "formats"` plugins need no approval (no code), but can be
disabled. *Rejected:* a second, TOML-based format syntax in the manifest — two syntaxes
for the same thing. *Rejected:* code-running parsers — a process round trip per line (or
per batch) cannot keep up with a 10 GB filter scan, and fields are computed on the scan
workers.

### D4. Trust by hash
`[plugins]` in `fasttail.ini` stores `approved.<name> = <sha256 of plugin.toml>`. A
source plugin runs only when approved and the hash matches; the approval dialog shows the
command line with placeholders. Executables inside the plugin folder are not hashed (a
plugin may update its binary); the manifest's command path is. Sessions and the command
line never approve a plugin.

### D5. Lifecycle
The child is started with stdin closed and stdout / stderr piped, in the plugin folder as
working directory, with `FASTTAIL_PLUGIN_API=1` in its environment. Closing the tab kills
it (the process tree on Windows through a job object). `on-exit` restarts after 1, 2, 5,
10, then every 30 s, and the stream bar shows `restarting (n)`; `never` leaves the stream
open with "source ended" and the exit code.

### D6. Persistence
`plugin://<name>?broker=localhost:9092&topic=orders` (percent-encoded values). `secret`
parameters are not written; on restore the parameter dialog opens with the others filled.
A missing or disabled plugin restores as a stream that says so, with the identity kept so
enabling the plugin and reopening works.

## Risks / Trade-offs

- [Users run a shared plugin without reading it] → explicit approval showing the
  command, re-approval on change, docs on the risk (same stance as external tools).
- [Contract churn] → `api = 1` frozen; additions only as optional keys.
- [Bundled formats slow down detection for everyone] → they are one more set of user
  formats, under the limits of `custom-log-formats`; a disabled plugin's formats are not
  loaded.
- [`custom-log-formats` slips] → source plugins ship without format bundles; the bundle
  part lands with or after it.

## Open Questions

- Is a WASM (`wasmi`, pure Rust) in-process parser API worth a v2 for formats a regex
  cannot express (multi-line records, binary framing)? Proposed: revisit after v1 usage.
- Should source plugins also receive the stream's current filter (to filter at the
  source, as journald `-g` would)? Proposed: no in v1.
- Where should shared, read-only plugins live for all users of a machine (for example
  `%ProgramData%\FastTail\plugins`)? Proposed: add it in v1 if cheap, approval still per
  user.
