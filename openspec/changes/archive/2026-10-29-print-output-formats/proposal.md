## Why

`fasttail --print` writes the lines that pass the filters as text. Scripts and pipelines
that feed another tool (jq, a SIEM, a spreadsheet) want records: hl's requests for a
customisable output template (#611) and machine formats (#1481) show the need, and FastTail
already parses fields. Gap 27 (post-0.27.0 scan), low-medium, small.

## What Changes

- `--print --output <FORMAT>`: `text` (default, as today), `json` (one object per line:
  `file`, `line`, `level`, `time` (ISO 8601), `text`, and the parsed fields under `fields`),
  `logfmt`, `csv` (header from the columns of the parser), or `template` with
  `--template '{time} {level} {msg}'` (field names, `{line}`, `{file}`, `{text}`).
- The parser is the one the window would use (detection or `--format NAME` for a log
  format).
- Works with `--follow`, `--context` (context lines carry `"context": true` in JSON) and
  merged inputs.

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Writing formats other than these (Parquet, OTLP).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-line`: the requirements below are added.

## Impact

- `src/print_mode.rs`: a record writer per format, field parsing on the print path
  (the existing parsers, one line at a time, no per-line allocation beyond the output);
  `--output`, `--template`, `--format` in `src/cli.rs` and the usage.
- README (print mode), CHANGELOG.
