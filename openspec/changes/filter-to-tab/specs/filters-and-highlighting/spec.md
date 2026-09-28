## ADDED Requirements

### Requirement: Filter Result Tab
While a stream in Text view has an active filter of its own (include or exclude terms, minimum level or time range), the stream menu and the row context menu SHALL offer "Open filter as new tab". It SHALL open a derived stream next to the source holding, in file order, the source lines that pass a copy of the source's filter taken at that moment, without the global filter. Later changes to the source's filter SHALL NOT change the derived stream. Lines appended to the source that pass the copied filter SHALL be appended to the derived stream while the source stream is open; a truncation or rotation of the source SHALL rebuild it. The derived stream SHALL support every stream feature, the global filter included, SHALL show the source line numbers in its gutter and in go-to line, and "Show in context" on one of its rows SHALL focus the source stream and show that source line in context. Its tab title SHALL name the source file and the first include term (or the level or time range when there is none), and its tooltip SHALL list the copied filter. When the source stream is closed, the derived stream SHALL stay open, stop following and say so in its stream bar. Its content SHALL be stored in a spool file bounded like the standard input spool.

#### Scenario: Two questions on one file
- **WHEN** `app.log` is filtered by the include term `ERROR`, the user opens the filter as a new tab and then changes the source filter to `req-7f3a`
- **THEN** the derived tab still shows the `ERROR` lines and the source tab shows the `req-7f3a` lines.

#### Scenario: Derived tab follows
- **WHEN** a derived tab of `ERROR` lines is open and the writer appends `ERROR db timeout` and `INFO ok` to the source
- **THEN** the derived tab gains `ERROR db timeout` only, numbered with its source line number.

#### Scenario: Back to the source
- **WHEN** the user picks "Show in context" on the derived row for source line 48,211
- **THEN** the source stream is focused, showing line 48,211 in context.

### Requirement: Derived Stream Persistence
A derived stream SHALL be saved in the workspace and in session files as its source path, its copied filter and its own view settings, SHALL NOT be added to the recent files, and SHALL be rebuilt from the source file when restored. Its bookmarks SHALL be stored by source line number.

#### Scenario: Restart
- **WHEN** FastTail is closed with a derived `ERROR` tab of `app.log` bookmarked at source line 900 and started again
- **THEN** the derived tab is rebuilt from `app.log` with the `ERROR` lines and the bookmark on source line 900.
