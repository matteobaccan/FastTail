## ADDED Requirements

### Requirement: Operation Jump
Both interfaces SHALL move to the next or previous line carrying the cursor row's operation id, using the Operations source or the first of the usual id fields.

#### Scenario: Follow a request
- **WHEN** the cursor row has `request_id=abc` and the user asks for the next line of this operation
- **THEN** the cursor moves to the next line with `request_id=abc`.
