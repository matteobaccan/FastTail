## ADDED Requirements

### Requirement: Plugin Discovery and Manifest
The application SHALL load plugins from the `plugins` folder next to `fasttail.ini`, each plugin being a subfolder holding a `plugin.toml` manifest with `api`, `name`, `version`, `kind` (`source` or `formats`) and the section of its kind. A manifest whose `api` is not `1`, or that cannot be parsed or lacks a required key, SHALL NOT be loaded and SHALL be listed in Settings ▸ Plugins with its error; unknown keys SHALL be reported as warnings. Settings ▸ Plugins SHALL list every plugin with its name, version, kind, state and error, and SHALL offer enable, disable, open the plugins folder and reload.

#### Scenario: Unsupported API version
- **WHEN** the plugins folder holds a manifest with `api = 2`
- **THEN** the plugin is not loaded and Settings ▸ Plugins lists it with an error naming the unsupported API version.

#### Scenario: Reload after editing
- **WHEN** the user adds a new plugin folder and presses reload in Settings ▸ Plugins
- **THEN** the new plugin is listed without restarting FastTail.

### Requirement: Source Plugins
A source plugin SHALL declare a command, its arguments and parameters of type `text`, `number`, `choice` or `secret`; choosing it from the open menu's Plugins submenu SHALL ask its parameters, expand each `{parameter}` placeholder once per argument, and start the command without a shell, with standard input closed and the plugin folder as working directory. Its standard output SHALL become a followed stream through a temporary spool bounded like the standard input stream, titled with the plugin name; when the process ends with an error the stream SHALL show the last 4 KB of its standard error. With `restart = "on-exit"` the process SHALL be restarted after 1, 2, 5, 10 and then every 30 seconds, the stream bar saying so; with `never` the stream SHALL stay open saying that the source ended and its exit code. Closing the stream SHALL end the process and its children. The stream SHALL be stored in the workspace and sessions as `plugin://<name>?<parameter>=<value>…` without the `secret` parameters, which SHALL be asked again on restore; the same identity SHALL be accepted on the command line.

#### Scenario: Kafka topic through a CLI
- **WHEN** an approved plugin runs `kcat -C -b {broker} -t {topic}` and the user opens it with the broker `localhost:9092` and the topic `orders`
- **THEN** `kcat` is started with `localhost:9092` and `orders` as separate arguments, and each message it prints appears as a line of the stream.

#### Scenario: Hostile parameter value
- **WHEN** the user enters `orders; rm -rf /` as the topic
- **THEN** the value reaches the program as one argument and no shell command is executed.

#### Scenario: Secret not stored
- **WHEN** a stream of a plugin with a `secret` parameter `token` is saved in a session and the session is loaded
- **THEN** the session file holds no token, and the parameter dialog opens with the other values filled and the token empty.

### Requirement: Plugin Trust
A source plugin SHALL NOT start until the user has enabled it in Settings ▸ Plugins through a confirmation that shows its command and arguments. The SHA-256 of the approved manifest SHALL be stored in `fasttail.ini`, and a plugin whose manifest has changed since SHALL be disabled until approved again. Opening a session, a workspace or a `plugin://` command-line argument SHALL NOT approve a plugin; a stream of a plugin that is missing or not approved SHALL open saying so and keep its identity.

#### Scenario: Plugin from a colleague
- **WHEN** the user copies a source plugin into the plugins folder and opens a session that uses it
- **THEN** the plugin does not run, its stream says that the plugin must be enabled in Settings, and after enabling it reopening the stream starts it.

#### Scenario: Manifest changed
- **WHEN** an approved plugin's `plugin.toml` is edited to run another command
- **THEN** the plugin is shown disabled as changed and does not run until approved again.

### Requirement: Plugin Format Bundles
A plugin folder MAY hold log format files (`*.fasttail-format.ini`, as defined by the log-formats capability); when the plugin is enabled they SHALL be loaded as read-only formats named `<plugin name>/<format name>`, validated, detected and offered exactly like the user's own formats, and the Log formats dialog SHALL mark them with their plugin and allow duplicating one into the user's formats folder. A plugin of kind `formats` runs no code and SHALL NOT need the approval required for source plugins. A bundled format that fails validation SHALL NOT be loaded and SHALL be listed in Settings ▸ Plugins with its error; the formats of a disabled plugin SHALL NOT be loaded.

#### Scenario: Vendor format shared with the team
- **WHEN** the plugin `acme` bundles `acme-server.fasttail-format.ini`, which maps `SEVERE` to ERROR, and the user opens a log whose first lines match it
- **THEN** the stream detects the format `acme/acme-server`, shows its fields, and counts `SEVERE` lines as ERROR.

#### Scenario: Broken samples
- **WHEN** a bundled format's sample line does not match its patterns
- **THEN** that format is not loaded and Settings ▸ Plugins lists the plugin with an error saying that its samples do not match.
