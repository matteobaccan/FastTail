## ADDED Requirements

### Requirement: Copy as Excerpt
Both interfaces SHALL copy the selected rows as a fenced block headed by the stream name, the line range and the time span, in Markdown, Jira or Slack flavour.

#### Scenario: To a ticket
- **WHEN** rows 1204 to 1210 are selected and the user copies them as an excerpt
- **THEN** the clipboard holds a header naming the file and lines, then the rows in a code block.
