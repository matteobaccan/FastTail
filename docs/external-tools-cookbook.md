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
| `{match}` | the first capture group of the tool's own regex applied to the row |

Arguments are split like a command line (quotes group words) and each one is passed to
the program as its own argv entry. A row containing `; rm -rf /` is therefore just text
— **unless** you switch "Run via shell" on, which hands the whole line to `cmd /c` or
`sh -c` and lets the shell parse it. Keep that off unless the log is trusted.

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
| Arguments | `ssh {match}` |
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

| Field | Value |
|---|---|
| Name | `jq` |
| Program | `cmd` (Windows) · `sh` (Linux/macOS) |
| Arguments | `/c echo {line} | jq . > %TEMP%\row.json && code %TEMP%\row.json` |
| Run via shell | **on** (a pipeline needs a shell) |

This one needs "Run via shell", so it belongs to trusted logs only: the row text is
parsed by the shell. On an untrusted log, prefer a small script that reads the line as
a single argument instead.

## 7. Copy a request id to the clipboard for the next query

| Field | Value |
|---|---|
| Name | `Copy trace id` |
| Program | `clip` (Windows) · `xclip` |
| Arguments | Windows: none (it reads stdin — use the shell form below) · Linux: `-selection clipboard` |
| `{match}` regex | `trace[_-]?id[=:]\s*([\w-]+)` |

Simplest portable form: a one-line script `copy-id.cmd` containing `echo %1| clip`, with
program `copy-id.cmd` and arguments `{match}` — no shell mode needed.

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
