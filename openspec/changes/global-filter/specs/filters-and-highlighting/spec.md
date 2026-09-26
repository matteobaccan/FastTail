## ADDED Requirements

### Requirement: Global Filter
The application SHALL offer one global filter of up to 8 include terms and up to 8 exclude terms, with case-sensitive and regex toggles of its own, edited in a global filter bar that is shown or hidden from the toolbar and with `CTRL + SHIFT + H`. While the global filter is switched on and has at least one non-empty term, a line of any stream SHALL be visible only when it passes that stream's own filters (terms, minimum level, time range) and also contains every non-empty global include term and none of the non-empty global exclude terms; a stack-trace continuation line SHALL follow its parent entry unless a stream or global exclude term matches it. The global filter SHALL apply to every open stream and to every stream opened afterwards, including standard input and compressed streams, and SHALL NOT change the HEX view. Switching it off SHALL restore each stream's own filtering without losing the global terms; hiding the bar SHALL NOT switch it off. While it is on, every stream bar SHALL show a global filter badge whose tooltip lists the global terms. The Find results, line counters, overview strip and search results pane SHALL see the lines as filtered by both. A change of the global terms SHALL be applied to the streams at most 300 ms after the last keystroke, and on a stream above 16 MB the recomputation SHALL run in the background as for a stream filter. The global filter SHALL be persisted in the `[global_filter]` section of `fasttail.ini` (`enabled`, `case_sensitive`, `regex`, `bar_open`, `include.1`…`include.8`, `exclude.1`…`exclude.8`) and SHALL NOT be written to session files or filter presets.

#### Scenario: Hiding the same noise everywhere
- **WHEN** three streams are open and the user adds the global exclude term `healthcheck`
- **THEN** no line containing `healthcheck` is visible in any of the three streams, each stream bar shows the global filter badge, and each stream keeps its own include and exclude fields unchanged.

#### Scenario: Combined with a stream's own terms
- **WHEN** the global include term is `req-7f3a` and a stream's own include term is `ERROR`
- **THEN** that stream shows only the lines containing both `req-7f3a` and `ERROR`, while a stream without terms of its own shows every line containing `req-7f3a`.

#### Scenario: A stream opened later
- **WHEN** the global exclude term `DEBUG` is on and the user opens a further log
- **THEN** the new stream opens with its `DEBUG` lines already hidden and shows the badge.

#### Scenario: Switched off, not lost
- **WHEN** the user switches the global filter off and restarts FastTail
- **THEN** every stream shows its own filtering only, and switching the global filter on again brings back the same terms.

#### Scenario: Find results under the global filter
- **WHEN** the global exclude term is `healthcheck` and the user searches `req-7f3a` across all streams
- **THEN** no result is a line containing `healthcheck`.

#### Scenario: Large file
- **WHEN** a 2 GB stream is open and the user adds a global include term
- **THEN** the interface stays responsive, the stream bar shows the background filter progress, and the stream shows the matching lines once the scan completes.
