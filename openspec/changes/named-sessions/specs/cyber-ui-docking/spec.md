## ADDED Requirements

### Requirement: Named Sessions
The application SHALL save the workspace (open files and patterns, dock layout, floating windows, per-stream filters, search queries, wrap, encoding, bookmarks) to a named session file and load it back, replacing the current workspace after confirmation. Global preferences SHALL NOT be part of a session. The title bar SHALL show the session name and a `*` when the workspace differs from the saved session. Paths SHALL be stored absolute and, when possible, relative to the session file so moved bundles still open. Missing files SHALL be skipped with a summary. Recent sessions SHALL be listed in a menu.

#### Scenario: Switching projects
- **WHEN** the user saves the current five tabs as `incident.fasttail-session.ini`, then loads `dev.fasttail-session.ini`
- **THEN** the five tabs are closed, the dev session's tabs and layout are restored, and the title bar shows `dev`.

#### Scenario: Session moved with its logs
- **WHEN** a session saved in `bundle/` referencing `bundle/app.log` is moved together with the folder to another drive
- **THEN** loading it opens `app.log` from the new location.
