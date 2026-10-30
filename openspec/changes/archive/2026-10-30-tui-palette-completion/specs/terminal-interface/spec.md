## ADDED Requirements

### Requirement: Terminal Palette Coverage
The terminal palette SHALL list every registry command that is not marked window-only, including select all, play and pause all, line numbers, auto-update, save session, save as default session, remove bookmark, open folder pattern and the recent files.

#### Scenario: Recent files
- **WHEN** the user opens *Recent files…* from the palette
- **THEN** the files of `[recent_files]` are listed and `Enter` opens the selected one.
