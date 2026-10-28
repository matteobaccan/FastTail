# Plugins

A plugin adds a **source** (a program whose output becomes a stream) or **log formats**
to FastTail, without rebuilding it. A plugin is a folder in `plugins/`, next to
`fasttail.ini`, with a `plugin.toml` manifest:

```text
fasttail.ini
plugins/
  kafka-topic/
    plugin.toml
  acme/
    plugin.toml
    acme-server.fasttail-format.ini
```

Settings → **🔌 Plugins…** lists them with their state and errors, enables and disables
them, opens the folder and reloads it after a change (no restart needed).

## The manifest

```toml
api = 1                       # the manifest version this FastTail reads (required)
name = "kafka-topic"          # letters, digits, - _ . (required; names the streams)
version = "0.1.0"             # (required)
description = "A Kafka topic through kcat"
kind = "source"               # "source" or "formats" (required)

[source]                      # required for kind = "source"
command = "kcat"              # a file in the plugin folder, a name on PATH, or a path
args = ["-C", "-b", "{broker}", "-t", "{topic}", "-o", "end", "-u"]
restart = "on-exit"           # "never" (default) or "on-exit"

[[source.param]]
id = "broker"                 # the {broker} placeholder
label = "Broker"
type = "text"                 # text, number, choice or secret
default = "localhost:9092"

[[source.param]]
id = "topic"
label = "Topic"

[[source.param]]
id = "mode"
type = "choice"
choices = ["end", "beginning"]
```

- A manifest whose `api` is not `1`, that cannot be read, or that lacks a required key is
  not loaded: Settings lists it with the error. Unknown keys are listed as warnings and
  ignored, so a newer manifest with optional additions still loads.
- `{id}` in an argument is replaced by the parameter's value, `{plugin_dir}` by the
  plugin folder and `{config_dir}` by the folder of `fasttail.ini`. Each argument stays
  **one argument** whatever the value holds: no shell ever sees it (`orders; rm -rf /` is
  passed as that text). To use shell features, make the command a script of the plugin.
- `number` values must be numbers, `choice` values one of `choices`; values are one line.
- At most 16 parameters; a manifest is at most 64 KB.

## Source plugins

The 🔌 toolbar menu lists the enabled source plugins. Choosing one asks its parameters
(none: it opens at once) and opens a stream titled with the plugin name:

- the program runs with standard input closed, the plugin folder as working directory
  and `FASTTAIL_PLUGIN_API=1` in its environment;
- its standard output is the stream, copied into a temporary spool bounded like the
  standard input stream (`stdin_spool_max_mb`), so search, filters, rules, bookmarks and
  every other stream feature work on it;
- its standard error is not part of the stream: the last 4 KB are in the stream bar's
  tooltip, and the last line is shown when the program ends with an error;
- with `restart = "never"` an ended program leaves the stream open saying `source ended
  (exit code N)`, with **▶ Start…** to run it again; with `"on-exit"` it runs again after
  1, 2, 5, 10, then every 30 seconds (back to 1 second once it ran for a minute), the
  stream bar saying `restarting (n)`;
- closing the stream (or FastTail) ends the program and every process it started (its
  process group on Linux and macOS, its process tree on Windows).

The stream's identity is `plugin://<name>?<param>=<value>&…` (values percent-encoded):
it is kept in the workspace, the recent files and sessions, and `fasttail
"plugin://kafka-topic?broker=localhost:9092&topic=orders"` opens it from the command line.
**`secret` parameters are never written**: after a restore the stream opens saying that
values are needed, and **▶ Start…** asks them with the other values filled.

## Trust

A source plugin runs a program with your rights, so it runs only once you enabled it in
Settings ▸ Plugins, which shows the command line it runs and asks for a confirmation. The
SHA-256 of the manifest you approved is kept in `fasttail.ini` (`[plugins]
approved.<name>=…`); when the manifest changes the plugin shows **changed** and does not
run until you approve it again. Executables inside the plugin folder are not hashed (a
plugin may update its binary), the manifest's command and arguments are.

Opening a session, a workspace or a `plugin://` argument never approves a plugin: a stream
of a plugin that is missing, not enabled or changed opens empty, says so, and keeps its
identity, so enabling the plugin and pressing **▶ Start…** runs it. Read a plugin you got
from someone else before you enable it, as you would a script.

## Format plugins

A plugin folder may hold log format files (`*.fasttail-format.ini`, the format of
Settings → Log formats, see the README). They load as read-only formats named
`<plugin>/<format>`, detected and offered like your own formats; the Log formats window
marks them with 🔌 and **Duplicate** copies one into your own `formats` folder to change
it. A format whose sample lines do not match its patterns is not loaded (Settings ▸
Plugins shows why). A plugin of `kind = "formats"` runs nothing and needs no approval: its
formats load unless you disable it; a source plugin's formats load while it is enabled.

```toml
# plugins/acme/plugin.toml
api = 1
name = "acme"
version = "1.0"
kind = "formats"
```

```ini
# plugins/acme/acme-server.fasttail-format.ini
[format]
name = acme-server
regex = ^(?P<ts>\d\d:\d\d:\d\d) (?P<level>[A-Z]+) (?P<msg>.*)$
timestamp_field = ts
timestamp_format = %H:%M:%S
level_field = level
level_map = SEVERE=error,WARNING=warn,INFO=info
message_field = msg
sample.1 = 10:00:00 SEVERE disk full
```

A log whose first lines match it opens with the format `acme/acme-server`: its fields in
the column view, `SEVERE` lines counted as ERROR.

## The terminal interface

`fasttail-tui` opens `plugin://` streams of enabled plugins (from the command line, the
workspace or sessions) and shows their state in the stream bar; enabling a plugin and
asking secret values are done in the window.
