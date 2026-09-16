## ADDED Requirements

### Requirement: Multi-language interface support
The application SHALL support 5 languages for all UI labels, menus, settings, migration prompts, and tooltips: English (`en`), Italian (`it`), French (`fr`), Spanish (`es`), and Chinese (`zh` / Simplified Chinese).

#### Scenario: Displaying UI in Italian
- **WHEN** the language is set to Italian
- **THEN** navigation menus, buttons, status indicators, and settings are displayed in Italian.

#### Scenario: Displaying UI in Chinese
- **WHEN** the language is set to Chinese
- **THEN** all UI labels are rendered in Simplified Chinese glyphs using an embedded unicode-compliant font.

### Requirement: Language detection and fallback
The application SHALL automatically detect the host operating system language on first launch, and SHALL fall back gracefully to English whenever a translation key is missing or an unsupported locale is encountered.

#### Scenario: First launch on Italian OS
- **WHEN** FastTail launches for the first time on a machine with Italian locale (`it-IT` or `it`)
- **THEN** the interface initializes automatically in Italian.

#### Scenario: Missing translation or unknown locale
- **WHEN** FastTail runs on a system with an unsupported locale or encounters an undefined translation key
- **THEN** it displays the standard English text without crashing or showing blank labels.

### Requirement: Runtime language switcher
The application SHALL provide an explicit language selector dropdown in the settings/menu, allowing users to switch languages instantly and persisting their choice in `fasttail.toml`.

#### Scenario: Switching language at runtime
- **WHEN** the user selects "Español" in the language dropdown
- **THEN** the entire UI re-renders immediately in Spanish without requiring an application restart.
