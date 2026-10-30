## ADDED Requirements

### Requirement: Remote Event Log
On Windows, FastTail SHALL follow a channel of another machine's Event Log given as `eventlog://host/Channel`, with the current user's credentials.

#### Scenario: A server's System log
- **WHEN** the user opens `eventlog://srv01/System` as a user allowed to read it
- **THEN** the stream shows srv01's System events as they arrive.
