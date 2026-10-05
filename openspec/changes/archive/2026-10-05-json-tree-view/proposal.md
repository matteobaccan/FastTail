## Why

`[+] JSON` turns a JSON line into a pretty-printed block: every key of every level at once,
sorted alphabetically instead of in the order the line has them. A payload of a few
hundred fields is a wall of text with no way to fold the parts that do not matter, to copy
one value, or to copy the path of a field to use elsewhere (a filter, `jq`, a ticket). A
JSON payload after a timestamp (`2026-10-05T10:00:00Z {"level":"error",...}`) is not
detected at all, though the field parser reads it. The terminal interface has nothing for
JSON. lnav, klogg (plugin) and Kibana's document view show JSON as a foldable tree.

## What Changes

- **Detection** follows the field parser: a line is JSON when it is an object or an
  array, or when an object follows a leading timestamp. The `[+] JSON` toggle stays.
- The expanded block becomes a **tree**, in the order of the line: objects and arrays are
  foldable nodes showing their key and a summary (`{4 keys}`, `[12 items]`); the first level
  is open, deeper levels folded. Scalars are coloured by type (string, number, boolean,
  null). Long strings are cut at 500 characters with `…`; an array or object shows its
  first 200 children, then a `… N more` row.
- **Node actions** (GUI context menu, terminal keys): copy the value (a scalar as text, an
  object or array as indented JSON), copy the **path** (`http.status`, `items[3].id`,
  `["x-request-id"]`), expand all below, collapse all below.
- The fold state of each expanded line is kept while the stream is open (not saved); a
  reload of the stream clears it.
- **Terminal interface**: `J` on the cursor row opens a JSON dialog with the same tree:
  `↑` `↓` move, `→` / `Enter` / `Space` unfold, `←` folds or goes to the parent, `*` expands
  all, `-` collapses all, `y` copies the value, `Y` copies the path, `Esc` closes; a click
  on a node toggles it. A row without JSON says so in the status bar.
- **Bounds**: a payload over 4 MB is not parsed (the tree says so); parsing runs only when
  a line is expanded, its result cached per line.

Target: a nightly patch of 0.16.x (maintainer request, 2026-10-05), ahead of the 0.21.0
analysis work. Priority: medium. Effort: S–M.

### Non-goals

- A tree spanning several lines (grouping lines by `trace_id`): proposed for 0.22.0.
- Editing JSON, JSONPath / jq queries, filtering on a node (field terms are
  `structured-field-terms`).
- JSON inside a string value (escaped JSON) is shown as a string.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `log-intelligence`: JSON detection after a leading timestamp; the expanded block is a
  foldable tree with node actions, in both interfaces.

## Impact

- `src/json_tree.rs` (new): an order-preserving JSON reader into a flat node list (kind,
  key, depth, parent, value span, child count), the path of a node, the visible rows under
  a fold state, the value text to copy. No new dependency (`serde_json` sorts keys without
  its `preserve_order` feature).
- `src/tail_engine.rs`: `is_json_line` follows `fields::json_start`; per-line fold state.
- `src/ui/dock.rs`: the tree replaces the pretty-printed block (both row paths), with its
  context menu.
- `src/tui/`: `J`, the JSON dialog, help and palette entries.
- `src/i18n.rs`: menu items and summaries in all 16 languages.
- README, `docs/ui-design.md`, CHANGELOG.
