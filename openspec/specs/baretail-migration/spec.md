## ADDED Requirements

### Requirement: Windows Registry discovery of BareTail configuration
On Windows, the application SHALL query `HKEY_CURRENT_USER\Software\Bare Metal Software\BareTail` and `BareTailPro` on first startup to discover previously used file paths, tabs, and highlighting preferences.

#### Scenario: BareTail registry keys exist on first boot
- **WHEN** FastTail runs for the first time on a machine with BareTail installed
- **THEN** it detects the registry key and displays an interactive HUD prompt asking the operator whether to import existing settings.

#### Scenario: No BareTail registry keys exist
- **WHEN** FastTail runs on a machine without BareTail registry keys or on a non-Windows OS
- **THEN** it boots silently into default FastTail settings without displaying any migration prompt.

### Requirement: Interactive migration prompt
The application SHALL prompt the user to confirm or decline the import of discovered BareTail settings before applying them.

#### Scenario: User accepts BareTail import
- **WHEN** the operator clicks "Import Settings" on the first-run prompt
- **THEN** FastTail parses the recent file paths and highlight color rules from the registry, opens the files in new tabs, registers the highlight rules, and saves them to `fasttail.ini`.

#### Scenario: User declines BareTail import
- **WHEN** the operator clicks "Start Fresh" on the first-run prompt
- **THEN** FastTail dismisses the prompt, creates a default clean `fasttail.ini`, and does not prompt again on subsequent boots.

### Requirement: Cross-platform configuration persistence
The imported or created configuration SHALL be saved in a portable INI file (`fasttail.ini`), ensuring all settings are preserved and cross-platform portable across Windows, Linux, and macOS. The file is looked up in this order: the path in the `FASTTAIL_CONFIG` environment variable, the current working directory, the executable directory, the per-user configuration directory (`%APPDATA%\FastTail` on Windows, `$XDG_CONFIG_HOME/FastTail` or `~/.config/FastTail` elsewhere). New configurations are written next to the executable, falling back to the per-user directory when that location is read-only. A legacy `fasttail.toml` SHALL be migrated automatically. Cargo test binaries SHALL never read or write the real configuration.

#### Scenario: Read-only install directory
- **WHEN** FastTail runs from a directory the user cannot write to (e.g. Program Files)
- **THEN** settings are saved in the per-user configuration directory and loaded from there on the next start.

#### Scenario: Settings saved on exit
- **WHEN** the application closes
- **THEN** open tabs, highlight rules, and layout states are written to the local configuration file.
