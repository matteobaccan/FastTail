## ADDED Requirements

### Requirement: Terminal Web View, MCP Server and Plugins
The terminal interface SHALL start the web view and the MCP server from its own streams when they are enabled in `fasttail.ini`, and SHALL offer a Plugins dialog to enable, disable and approve plugins and set a source plugin's parameters.

#### Scenario: Web view on a server
- **WHEN** `[web] enabled=true` and the terminal interface is running with two streams
- **THEN** a browser on the configured address lists the two streams and shows their lines.

#### Scenario: Approve a plugin
- **WHEN** a new plugin is installed and the user opens the Plugins dialog in the terminal
- **THEN** its hash is shown and `y` approves it, after which its streams open.
