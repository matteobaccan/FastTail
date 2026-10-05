## ADDED Requirements

### Requirement: Terminal Search All Streams
The terminal interface SHALL offer Search all streams with `Ctrl+Shift+F` where the terminal reports it, a key that every terminal delivers, and a command palette entry. It SHALL open a Find results window in the terminal dock with the focused stream's query, which is not saved in the layout. Searching SHALL match each open stream as the window's Search all streams does: the lines each stream shows under its own filters and the global filter, escape sequences removed where the stream removes them, HEX and ASM streams skipped, one background job per stream with at most four running. The results SHALL be grouped by stream with the stream name, the match count and the progress, each group collapsible, and walked with the arrows, the pages, Home and End. `Enter` on a result SHALL focus that stream's window with the cursor on the line and follow paused, without changing that stream's search. A refresh key SHALL run the query again, closing the window SHALL cancel the jobs, and a stream reloaded since the search SHALL be marked stale. Up to 100,000 hits SHALL be listed per stream with the true total counted.

#### Scenario: A request id across logs
- **WHEN** three streams are open and the user searches all streams for `req-8812`
- **THEN** the Find results window lists the matching lines of each stream under its name with the count.

#### Scenario: Jump to a result
- **WHEN** the user presses `Enter` on a result of `api.log`
- **THEN** the `api.log` window is focused with its cursor on that line and follow paused, and its own search is unchanged.

#### Scenario: Stale results
- **WHEN** a searched stream is truncated after the search
- **THEN** its group is marked stale and `Enter` on its results does not jump to a wrong line.
