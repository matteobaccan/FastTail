## ADDED Requirements

### Requirement: Merged Timeline View
The user SHALL be able to create a merged view from two or more open streams that carry detected timestamps. The merged view SHALL present the union of their lines ordered by timestamp, SHALL update live as any source grows, and SHALL attribute each row to its source with a colour chip and a tooltip showing the file name. Filters, search, highlight rules, bookmarks and export SHALL work on the merged view as on a stream. Sources MAY be toggled on and off without recreating the view. Merging a stream without recognised timestamps SHALL be refused with a hint.

#### Scenario: Two services, one request
- **WHEN** `gateway.log` and `payment.log` are merged and each receives lines for request 42 at 14:02:05.100 and 14:02:05.250
- **THEN** the merged view shows the gateway line first, then the payment line, each with its source chip, and follow mode keeps both at the bottom as they grow.

#### Scenario: Source without timestamps
- **WHEN** the user tries to add a file whose lines carry no recognised timestamp
- **THEN** the file is refused and the dialog explains that timestamps are required.

### Requirement: Merged View Persistence
Merged views SHALL be persisted in the workspace by their source paths and recreated at startup once every source is open.

#### Scenario: Restart
- **WHEN** FastTail restarts with a saved merged view of two files that still exist
- **THEN** the merged view tab is restored with the same sources.
