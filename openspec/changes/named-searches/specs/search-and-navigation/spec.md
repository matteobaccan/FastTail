## ADDED Requirements

### Requirement: Named Searches
A stream SHALL keep up to eight named searches highlighted at once, each in its own colour, SHALL navigate across all of them or a focused one, and SHALL save them in the workspace and sessions.

#### Scenario: Two kept searches
- **WHEN** `timeout` and `refused` are kept searches
- **THEN** both are highlighted in their colours and `F3` goes to the next line holding either.
