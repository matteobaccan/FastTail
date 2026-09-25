# External tools cookbook

An external tool is a command FastTail runs **on a log row**. You pick the row (or a
selection of rows), FastTail expands the placeholders with what that row contains, and
starts the program — from the row's right-click menu, from the stream menu, from a
keyboard shortcut, or automatically when a highlight rule matches a new line.

That is the whole idea: a log line is rarely the end of the investigation. It names a
file, a host, a request id, a stack frame. An external tool is the shortest path from
"the log says X" to "I am looking at X".

Everything here is configured in **Settings → External tools**. The reference for the
fields and the placeholder list is in the [README](../README.md#external-tools); this
page is the practical half.

## The placeholders in one table

| Placeholder | What it expands to |
|---|---|
| `{line}` | the whole text of the row |
| `{file}` | the file being tailed (for a pattern stream, the file it currently follows) |
| `{dir}` | the directory of that file |
| `{lineno}` | the 1-based line number |
| `{selection}` | the selected rows as text, or the row itself when nothing is selected |
| `{match}` | the first capture group of the tool's own regex applied to the row (the whole match when the regex has no group, empty when it does not match) |

Arguments are split like a command line (quotes group words) and each one is passed to
the program as its own argv entry. A row containing `; rm -rf /` is therefore just text.

"Run via shell" wraps the program in `cmd /c` (Windows) or `sh -c` instead, with every
expanded argument quoted for that shell. The quoting covers what you write in the
arguments too, so `|`, `>` and `&&` reach the program as plain text rather than building
a pipeline: a pipeline belongs in a script (recipes 6 and 7). The Windows quoting is also
weaker than the POSIX one — a row containing a `"` can still reach `cmd`'s own parser —
so keep shell mode off unless the log is trusted.

Tools run with no standard input and their output discarded: a program that reads stdin
(`clip`, `xclip`, `jq`) gets nothing, so give it the value as an argument through a script.

---

## 1. Open the log file at this line in an editor

The most useful one, and the reason `{lineno}` exists. Tailing a file the application
writes, you spot the error, and you want the source of the log — or the log itself —
open where you are looking.

| Field | Value |
|---|---|
| Name | `Open in VS Code` |
| Program | `code` |
| Arguments | `-g "{file}:{lineno}"` |
| Shortcut | `Ctrl+Shift+E` |

Variants: `notepad++ "{file}" -n{lineno}`, `subl "{file}:{lineno}"`,
`idea --line {lineno} "{file}"`, `vim +{lineno} "{file}"` (in a terminal).

## 2. Open the folder the log lives in

For when the interesting thing is next to the log: a heap dump, a core file, yesterday's
rotated log.

| Field | Value |
|---|---|
| Name | `Open folder` |
| Program | `explorer` (Windows) · `xdg-open` (Linux) · `open` (macOS) |
| Arguments | `"{dir}"` |

## 3. Jump to the source file named in a stack trace

Java, Python and .NET stack frames carry a file and a line. A `{match}` regex pulls the
path out of the row and hands it to the editor.

| Field | Value |
|---|---|
| Name | `Open stack frame` |
| Program | `code` |
| Arguments | `-g "{match}"` |
| `{match}` regex | `at .*\((.*:\d+)\)` |

That regex captures `Service.java:214` out of
`at com.acme.Service.handle(Service.java:214)`. For Python tracebacks use
`File "(.*)", line (\d+)` and pass `-g "{match}"` — the first group is the path.

## 4. SSH to the host the line mentions

Infrastructure logs name the machine that failed. One shortcut and you are on it.

| Field | Value |
|---|---|
| Name | `SSH to host` |
| Program | `wt` (Windows Terminal) · `gnome-terminal` |
| Arguments | Windows Terminal: `ssh {match}` · gnome-terminal: `-- ssh {match}` |
| `{match}` regex | `host=([\w.-]+)` |

## 5. Open the URL or the ticket in the browser

A row that carries a URL, a trace id or an issue key becomes a link.

| Field | Value |
|---|---|
| Name | `Open URL` |
| Program | `explorer` (Windows) · `xdg-open` |
| Arguments | `{match}` |
| `{match}` regex | `https?://\S+` |

For an issue key, capture it and build the link in the arguments:
regex `([A-Z]+-\d+)`, arguments `https://jira.example.com/browse/{match}`.

## 6. Pretty-print the JSON payload of the row

Structured logs put a JSON document on one line. FastTail can expand JSON in place with
the `[+] JSON` toggle; this is for when you want it in a real tool.

`jq` reads standard input and the result has to land somewhere, so this is a pipeline —
and a pipeline lives in a script, not in the argument list (in shell mode the `|` would
be quoted like everything else). The script is handed the file and the line number
rather than the row text, so a JSON document full of quotes never has to survive a
command line.

Windows, `C:\tools\pretty-json.ps1`:

```powershell
param([string]$File, [int]$Line)
$row = Get-Content -LiteralPath $File -TotalCount $Line | Select-Object -Last 1
$out = Join-Path $env:TEMP 'row.json'
$row | jq . | Set-Content -LiteralPath $out
code $out
```

Linux / macOS, `~/bin/pretty-json.sh` (made executable with `chmod +x`):

```sh
#!/bin/sh
out="${TMPDIR:-/tmp}/row.json"
sed -n "${2}p" "$1" | jq . > "$out" && code "$out"
```

| Field | Value |
|---|---|
| Name | `jq` |
| Program | `powershell` (Windows) · `/home/you/bin/pretty-json.sh` (Linux/macOS) |
| Arguments | Windows: `-NoProfile -ExecutionPolicy Bypass -File "C:\tools\pretty-json.ps1" "{file}" {lineno}` · Linux/macOS: `"{file}" {lineno}` |
| Run via shell | off |

Quote Windows paths in the argument list: outside quotes a backslash escapes the next
character, so an unquoted `C:\tools\x` would arrive as `C:toolsx`.

## 7. Copy a request id to the clipboard for the next query

`clip` and `xclip` read standard input, which FastTail does not provide, so a
two-line script turns the captured id (its first argument) into their input.

Windows, `copy-id.cmd` (`set /p` prints the id without a trailing newline):

```bat
@echo off
<nul set /p "=%~1" | clip
```

Linux, `~/bin/copy-id.sh` (made executable with `chmod +x`):

```sh
#!/bin/sh
printf '%s' "$1" | xclip -selection clipboard
```

| Field | Value |
|---|---|
| Name | `Copy trace id` |
| Program | `C:\tools\copy-id.cmd` (Windows) · `/home/you/bin/copy-id.sh` (Linux) |
| Arguments | `{match}` |
| `{match}` regex | `trace[_-]?id[=:]\s*([\w-]+)` |
| Run via shell | off |

## 8. Search the whole file for what this row shows

The in-app search covers the buffer; `rg` covers the file on disk, including the parts
FastTail has not loaded.

| Field | Value |
|---|---|
| Name | `Grep this id` |
| Program | `rg` |
| Arguments | `-n "{match}" "{file}"` |
| `{match}` regex | `\b([0-9a-f]{8}-[0-9a-f-]{27})\b` |

## 9. Fire a webhook when a rule matches — the rule-bound tool

This is the one that runs **by itself**. Bind the tool to a highlight rule ("Run on
rule") and every appended line the rule matches starts it: at most once per second per
tool, with at most 10 children alive at a time; the excess is counted as dropped runs in
the settings row.

| Field | Value |
|---|---|
| Name | `Alert on FATAL` |
| Program | `curl` |
| Arguments | `-s -X POST -H "Content-Type: application/json" -d "{\"text\":\"{line}\"}" https://hooks.example.com/services/XXX` |
| Run on rule | your `FATAL` highlight rule |

Combine it with the rule's sound alert and the "flash the window on background alerts"
preference and a hidden tab can still reach you.

## 10. Restart the service that just died

The sharp one. Bind it to a *shortcut*, not to a rule, so it never fires on its own.

| Field | Value |
|---|---|
| Name | `Restart service` |
| Program | `sc` (Windows) · `systemctl` |
| Arguments | Windows: `start MyService` · Linux: `restart myservice` |
| Shortcut | `Ctrl+Shift+F12` |

---

## Habits that keep this safe

- **Leave "Run via shell" off.** Without it the row text can never become a command: each
  argument arrives at the program as one argv entry. Turn it on only for a pipeline you
  wrote, on a log you trust.
- **Bind to a rule only what is safe to run unattended.** A rule-bound tool runs on data
  the log produces; an attacker who can write to the log chooses when it runs.
- **Put the logic in a script.** When a recipe grows past a couple of arguments, write a
  `.cmd` or `.sh` and give it `{match}` — it is easier to read, easier to test, and it
  does not need shell mode.
- **Use `{match}` instead of `{line}` when you can.** A captured group is a value; the
  whole line is a sentence. Passing the value keeps the command short and the failure
  modes obvious.

Tools are stored as `[tool.N]` sections of `fasttail.ini`, so a working set can be copied
between machines, or shared with a teammate.
