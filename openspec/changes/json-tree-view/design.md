## Context

The GUI keeps `TailEngine::expanded_json_lines` (a set of line indices) and, for an
expanded line, parses it with `serde_json::Value` and prints `to_string_pretty` into a
frame below the row, in both the fixed-height and the wrapped row paths of
`src/ui/dock.rs`. `serde_json` is built without `preserve_order`, so keys come out sorted.
`fields::json_start` already finds an object after a leading timestamp.

## Goals / Non-Goals

**Goals:** a foldable tree in the line's order, node copy actions, the same tree in the
terminal, no new dependency, no cost for lines that are not expanded.

**Non-Goals:** multi-line trees, queries, editing.

## Decisions

### D1. Own reader into a flat node list
`json_tree::parse(text)` walks the bytes once and pushes one node per value, in document
order: kind, key and value as byte ranges into the payload, depth, parent and number of
children. Nothing is copied until a node is drawn or copied; strings are unescaped then.
*Alternative:* `serde_json` with `preserve_order` (pulls `indexmap` into the feature set
and still builds a full value tree) — rejected, a flat list is what both front ends walk.

### D2. Fold state
A map per engine (line → set of open node ids), created with the first level open when a
line is expanded. `Tree::rows(&open)` yields the rows to draw (node, depth, open or not),
skipping the children of folded nodes and cutting each container after 200 children with
a `… N more` row. A reload clears the map together with `expanded_json_lines`.

### D3. Cache
The parsed trees of the expanded lines are cached by line and reload generation, at most
64 at a time; a payload over 4 MB is refused before parsing.

### D4. Paths
`.key` for identifier keys (`[A-Za-z_][A-Za-z0-9_]*`), `["key"]` otherwise (quotes and
backslashes escaped), `[n]` for array items; the root step has no leading dot
(`http.status`).

### D5. Terminal
The tree goes in a dialog (a list over the form toolkit's scrolling), not inline:
inserting rows into a stream window would break the window's row arithmetic (cursor,
selection, mouse rows). The dialog is sized to the screen, titled `JSON - line N`.

## Risks / Trade-offs

- [A huge array] → 200 children per container, `… N more`; expand all stops at 5,000 rows
  and says so.
- [Malformed JSON] → the tree shows what was read before the error and a row saying where
  the payload stopped being valid.
