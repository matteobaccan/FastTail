## Context

`auto_highlight` finds paths (`TokenKind::Path`) per row for colouring; `external_tools`
expands `{file}` / `{lineno}` for the stream's own file and spawns programs without a shell.

## Goals / Non-Goals

**Goals:** one click from a stack frame to the editor at that line, with no per-language
setup beyond the editor and the source folders.

**Non-Goals:** source indexing, remote files.

## Decisions

### D1. Patterns, not a parser
A small ordered list of regexes (D1 table in the code) runs only on the rows drawn (or the
cursor row in the terminal), never during scans; matches are cached per row like the
automatic highlights.

### D2. Resolution
An absolute path that exists wins. Otherwise each source root is tried in order: the path
as written, then the Java form (`com.acme.Foo` + `Foo.java` → `com/acme/Foo.java`). Windows
paths and `/` paths both accepted; nothing outside the roots is searched.

### D3. Editor command
A template split like external tools' arguments, each placeholder expanded inside one
argument, spawned without a shell. Presets fill the field; an empty field means the system
opener (`xdg-open`, `open`, `start`) on the file without a line.

## Risks / Trade-offs

- [False positives such as `12:30:45` read as `file:line`] → a location needs a file name
  with an extension or a known frame shape; times never match.
- [A log can name any path] → only files under the source roots or absolute existing files
  are opened, and only by a user's click.

## Open Questions

- Should a location also open in FastTail itself (the file as a stream at that line)?
  Proposed: a second menu entry, "Open in FastTail".
