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
- **THEN** FastTail parses the recent file paths and highlight color rules from the registry, opens the files in new tabs, registers the highlight rules, and saves them to `fasttail.toml`.

#### Scenario: User declines BareTail import
- **WHEN** the operator clicks "Start Fresh" on the first-run prompt
- **THEN** FastTail dismisses the prompt, creates a default clean `fasttail.toml`, and does not prompt again on subsequent boots.

### Requirement: Cross-platform configuration persistence
The imported or created configuration SHALL be saved in a portable format (`fasttail.toml`), ensuring all settings are preserved and cross-platform portable across Windows, Linux, and macOS.

#### Scenario: Settings saved on exit
- **WHEN** the application closes
- **THEN** open tabs, highlight rules, and layout states are written to the local configuration file.
