## Why

The timeline histogram (0.10.0) shows *that* something happened: a red burst at 14:02, a
volume spike at 03:10. It does not say *what*. The user drags a window over the spike,
reads the lines, and tries to guess which messages are new or more frequent than usual,
which on a spike of 40,000 lines is guesswork. Grafana Logs Drilldown and Kibana's log
rate analysis answer "what is different in this span" by comparing it with a baseline:
which levels, which message kinds and which field values are over-represented. The
post-0.12.0 scan lists "patterns and summaries" as a market signal; no desktop viewer
offers this, so it would set FastTail apart.

## What Changes

- **Explain a span** from the histogram: its right-click menu offers "Explain this bar"
  (the column under the pointer) and, while a time window is set, "Explain the time
  window". The time range popup gets the same "Explain" button for the window it holds.
- An **Explain** tab for the stream compares the **span** with a **baseline**: the rest of
  the log (default) or the span of equal length right before it.
- Sections, each ranked by how much more the span has than the baseline:
  - **Volume**: lines per minute in the span and in the baseline, and the ratio;
  - **Levels**: count and share per level on both sides and the change;
  - **Messages**: message templates (the line after its timestamp, numbers, hex values,
    ids and IPs masked, as collapse's Numbers mode) over-represented in the span, with
    counts, the ratio, and a **new** badge for templates absent from the baseline; top 20;
  - **Fields** (on streams with a `structured-fields` parser): `field=value` pairs
    over-represented in the span; top 20; fields with more than 1,000 distinct values in
    the span are skipped.
- Actions: click a message to go to its first line in the span; "Search this message"
  puts its longest fixed part into the stream search; click a field value to add the
  field term (`structured-fields`); "Set as time window" applies the span.
- **Scope**: every timed line, like the histogram; an "Only filtered lines" checkbox
  restricts both sides to the lines the filters show.
- Runs on a **worker** with progress; the baseline is sampled down to 1,000,000 entries
  with counts scaled. Not persisted; no new key in `fasttail.ini`.
- When `pattern-grouping` ships, messages use its Drain patterns and each row links to
  the Patterns tab.

Target release: **0.14.0** (structured logs and analysis), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **Medium**. Effort: **M**.

### Non-goals

- Automatic spike detection or alerts on anomalies.
- Comparing two streams, or two arbitrary spans chosen by the user (the baseline is the
  rest or the preceding span).
- Statistical significance tests or AI summaries.
- Explaining spans in the Find results tab or a merged view (a merged view is a stream
  once `merged-timeline-view` ships and gets the tab like any stream).

## Capabilities

### New Capabilities

- `spike-explanation`: explaining a histogram span against a baseline — the entry points,
  the Explain tab and its sections, scoring, actions, scope and limits.

### Modified Capabilities

None (the histogram's context menu is new; its existing clicks and drags are unchanged).

## Impact

- `src/spike.rs` (new): `SpikeSpec { span, baseline, filtered }`, template counting with
  `collapse::normalize_line` (Numbers mode) and IP masking, field-pair counting, the
  smoothed log-ratio score, merge of partial results.
- `src/scan_job.rs`: `JobSpec::Explain` selecting lines by the timestamp cache, sending
  partial counts; baseline sampling.
- `src/ui/timeline_strip.rs`: right-click menu on the strip; `src/ui/time_range.rs`:
  "Explain" button.
- `src/ui/explain_tab.rs` (new) and `src/ui/dock.rs` / `app.rs`: the tab, actions.
- With `structured-fields`: field scanning per line; with `pattern-grouping`: its
  snapshot for classification.
- `src/i18n.rs` (16 languages), help dialog, README, CHANGELOG, tests.
- **TUI (0.20.0, PR #132)**: `spike.rs` is UI-free; the TUI can show the sections as a
  text report from its histogram strip.
