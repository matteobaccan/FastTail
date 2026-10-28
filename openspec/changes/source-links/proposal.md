## Why

Stack traces and compiler or test output name source locations (`at Foo.bar(Foo.java:42)`,
`File "app.py", line 12`, `src/main.rs:120:5`, `Program.cs:line 33`). Developers read them
in FastTail, then type the path into their editor. lnav's master gained clickable source
locations that open the editor (2026-10-24, listed in its 0.15.0 news); IDE consoles do it
for their own output. FastTail already recognises paths (automatic highlighting) and runs
external tools with `{file}` / `{lineno}`, but nothing ties a location in a line to the
editor. Gap 23 in `docs/competitor-analysis.md` (post-0.27.0 scan), value high.

## What Changes

- **Source locations** in a line are recognised: `path:line[:col]`, Java / Kotlin
  `(File.java:42)`, Python `File "x.py", line 12`, .NET `in X.cs:line 33`, Go `x.go:42`,
  Node `(/app/x.js:10:5)`, Rust panics and compiler output `--> src/x.rs:3:9`.
- They are underlined on hover; **Ctrl + click** (or the row menu "Open source location")
  runs the **editor command**: a template in Settings with `{file}`, `{line}`, `{col}`,
  preset for VS Code (`code -g {file}:{line}:{col}`), JetBrains (`idea --line {line}
  {file}`), Notepad++, Sublime, Vim in a terminal, or any program.
- **Source roots**: a list of folders; a relative path (`com/acme/Foo.java`, `src/x.rs`) is
  resolved against them (the first existing file wins, Java packages mapped to folders);
  a location whose file is not found says so instead of running the editor.
- The terminal interface: `o` on the cursor row opens its first location; with several, a
  small list to pick from.

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **High**. Effort: **M**.

### Non-goals

- Indexing source trees or mapping messages to log statements (lnav's `log2src`).
- Opening remote files (SSH streams): only local paths.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-and-navigation`: the requirements below are added.

## Impact

- New `src/source_links.rs`: location patterns, resolution against the roots, Java
  package mapping, editor command expansion (reusing `external_tools::split_args` and the
  no-shell spawn).
- `src/ui/dock.rs` hover and Ctrl + click on row spans; row menu entry; Settings fields.
- `src/tui/app.rs` `o` key and the pick list.
- `src/config.rs`: `editor_command=`, `source_roots=`; round trip.
- README, CHANGELOG, `docs/ui-design.md`, i18n in every language.
