## Context

The sound alert hook in `poll_updates` already evaluates rules on appended lines and throttles playback; external tools bound to rules are the same hook with a process spawn. Row context menus do not exist yet; the stream menu does.

## Goals / Non-Goals

**Goals:** run a command with data from a row in one gesture, automate reactions to rule matches, keep it safe from runaway spawns.

**Non-Goals:** capturing tool output inside FastTail; a scripting language; running tools with elevated rights.

## Decisions

- **Placeholders are expanded per argument and passed as separate argv entries**, never through a shell, so a log line containing `; rm -rf` cannot be executed. A "run via shell" checkbox exists for users who want `cmd /c` or `sh -c`, off by default and clearly labelled.
- **`{match}`** applies the tool's own regex to the row and inserts the first capture group, which is how "open the ticket id" tools are built.
- **Rule-bound tools** run at most once per second per tool and never more than 10 concurrent children; excess matches are dropped and counted in the tool's status.
- **Spawn is fire-and-forget** (`Command::spawn`, handle dropped) on the UI thread; spawning costs milliseconds.

## Risks / Trade-offs

- [Users bind a tool to a rule that matches every line] → throttle and cap; a warning in the settings when the bound rule has no text pattern.
- [Windows console flash for console tools] → `CREATE_NO_WINDOW` on Windows for non-shell runs.
