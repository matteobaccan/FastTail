## Context

The parsers, detection, row cache and column view are in place (`src/fields.rs`,
`TailEngine::row_fields`, `render_field_header`, `draw_field_cells`); see the design of
`structured-fields` in the archive for the decisions they follow. This change keeps those
decisions and adds the parts that did not ship in 0.14.0.

## Decisions

1. **Field terms in the existing pipeline** (structured-fields design, decision 5):
   `FilterTerm` gains `field: Option<FieldTerm>`, built by `FilterSpec::build` when the
   spec has a parser; the grammar
   `^([A-Za-z_@][A-Za-z0-9_.@-]*)\s*(=|!=|~=|>=|<=|>|<)\s*(.*)$`, quoted terms literal.
   `included` / `excluded` scan a line's fields at most once per call, only when a field
   term exists, with a text pre-check for `=` and `~=`. The global filter's terms are
   compiled per stream with that stream's parser.
2. **OR / NOT** by alternatives and the exclude side (decision 6); a boolean language is
   `boolean-filter-expressions`.
3. **Level and timestamp from fields** (decision 7): first level field through
   `log_level::token_level`, first time field through the existing parsers; `JobSpec`
   carries the parser; a parser change resets the level and timestamp caches.
4. **Cells** (decision 8): hit tints and rule spans mapped to cells by byte range, split
   at cell boundaries, within the 64-span budget; whole-row styles colour the row. Copy as
   shown writes the cells with tabs. The character selection is clipped to the cell it
   starts in. The last column wraps using the wrap layout of the Text view, the other
   cells drawn on the first line of the row.

## Risks / Trade-offs

- [`level=error` typed as text now means a field term on a stream with a parser] → the
  `ƒ` badge shows it, quoting keeps the old meaning, and on logfmt it matches the same lines.
- [Field filter slower than a text filter on a huge JSON log] → background job with
  progress; text pre-check; the parser bench (`cargo bench --bench fields`) measures
  ~500 MB/s JSON and ~700 MB/s logfmt per core for the scan alone.
