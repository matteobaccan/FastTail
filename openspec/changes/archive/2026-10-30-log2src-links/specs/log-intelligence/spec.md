## ADDED Requirements

### Requirement: Logging Statement Links
FastTail SHALL index the logging statements of the source folders and SHALL open the editor at the statement whose format string matches the cursor row's message.

#### Scenario: From message to code
- **WHEN** `src/pay.rs` holds `info!("payment {} declined", id)` and the row reads `payment 42 declined`
- **THEN** going to the logging statement opens `src/pay.rs` at that line.
