## ADDED Requirements

### Requirement: DLT Files
FastTail SHALL open DLT storage files as streams whose rows are the messages with their time, ECU, application, context, level and decoded payload as fields.

#### Scenario: A DLT file
- **WHEN** the user opens `trace.dlt`
- **THEN** each DLT message is a row with its ECU, APID, CTID and level as columns.
