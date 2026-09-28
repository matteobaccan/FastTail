## Why

Every source and format FastTail reads is built in. Users with an in-house log format,
a proprietary service to read from (a message queue, a vendor CLI, a cloud log API) or a
niche system can only pipe a command into `fasttail -`, which loses the stream's identity
at the next start and needs a terminal. Tailviewer and LogExpert ship plugin APIs
(LogExpert's columnizers and its SFTP plugin are plugins); lnav's user-defined formats
are declarative files. The post-0.12.0 competitor scan lists a plugin API among the
remaining gaps (gap 20). The backlog already holds many single-purpose sources (journald,
Docker, SSH, Event Log, network, OTLP); a plugin interface lets the long tail be served
without growing the core, and lets users share formats.

## What Changes

- **Plugins folder**: a plugin is a folder with a `plugin.toml` manifest in `plugins/`
  next to `fasttail.ini` (and in `--config`'s folder). Manifest fields: `api = 1`, `name`,
  `version`, `description`, `kind` (`source` or `formats`), and the kind's section.
  Unknown `api` versions and malformed manifests are listed in Settings with the error and
  not loaded.
- **Source plugins** (out of process): a command and its arguments, with declared
  **parameters** (`text`, `number`, `choice`, `secret`) expanded into the arguments like
  external-tool placeholders (one argv entry each, no shell). The open menu gets a
  **Plugins ▸** submenu; choosing a plugin asks its parameters, starts the command, and its
  standard output becomes a followed stream (spooled like standard input); standard
  error's last 4 KB is shown if it exits with an error. A restart policy (`never`,
  `on-exit` with back-off) is declared in the manifest. Identity
  `plugin://<name>?<param>=<value>...` in workspace, sessions and the command line;
  `secret` parameters are never stored and are asked again on restore.
- **Format bundles** (declarative, no code): a plugin folder may ship log format files
  (`*.fasttail-format.ini`, the format defined by the pending `custom-log-formats`
  change). They are loaded as read-only formats named `<plugin>/<format>`, detected and
  offered like the user's own formats, so a vendor or a team can share a source and the
  format of what it prints as one folder. A plugin with `kind = "formats"` holds only
  formats.
- **Trust**: a source plugin runs programs, so each one is **disabled until the user
  enables it** in Settings ▸ Plugins, which shows its command; the manifest's hash is
  stored and a changed manifest is disabled again until re-approved. Format files run no
  code and are loaded with their plugin's manifest, with the validation of
  `custom-log-formats`.
- **Settings ▸ Plugins**: list (name, version, kind, state, error), enable / disable,
  open the plugins folder, reload.

Target release: **0.15.0** (sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **L**.

### Non-goals

- In-process native plugins (dynamic libraries: no stable Rust ABI, a crash takes the
  app down) and WASM plugins in v1 (see design, open question).
- Plugins that add UI panels, menu actions on rows, renderers or themes.
- A plugin registry, download or auto-update.
- Parser plugins that run code per line (a process round trip per line is too slow for
  multi-GB files; see design D3), and a second format syntax beside
  `custom-log-formats`.

## Capabilities

### New Capabilities

- `plugin-api`: plugin discovery and manifest, source plugins, format bundles, trust
  and management.

### Modified Capabilities

None. Bundled formats use the `log-formats` capability of the pending
`custom-log-formats` change unchanged; only where they are loaded from is new.

## Impact

- New `src/plugins.rs`: manifest parsing with `toml` (already a dependency), discovery,
  hash and approval state, parameter model.
- New `src/plugin_source.rs`: child process (no shell, like `external_tools::build_command`
  without the shell flag), stdout into `spool_feed`, stderr ring, restart policy; closing
  the tab kills the child.
- `custom-log-formats` (`src/log_format.rs`): load formats from each enabled plugin
  folder as well as from `formats/`, read-only, named `<plugin>/<format>`.
- `src/cli.rs`: `plugin://` arguments; `src/ui/app.rs`: Plugins submenu, parameter
  dialog, Settings ▸ Plugins; `src/session.rs` and workspace: `plugin://` identity with
  non-secret parameters.
- `src/config.rs`: `[plugins]` section with approved manifest hashes.
- `docs/plugins.md` (new, the manifest reference with two examples), README, CHANGELOG,
  `src/i18n.rs` (16 languages).
- **TUI (0.20.0, PR #132)**: discovery, sources and format bundles are UI-free; the TUI needs a
  parameter prompt and must not start unapproved source plugins either.
