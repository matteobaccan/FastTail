## ADDED Requirements

### Requirement: Clipboard as a Stream
The application SHALL open the plain text of the clipboard as a new stream (`Ctrl + Shift + V` outside a text field, the command palette), spooled like standard input and up to its size cap, and SHALL append a later paste to that stream through its Append action. A clipboard stream SHALL NOT be restored by the workspace.

#### Scenario: Two pastes
- **WHEN** the user pastes 20 lines as a stream and then appends 5 more lines with Append
- **THEN** one stream shows the 25 lines in order, and the next start does not reopen it.
