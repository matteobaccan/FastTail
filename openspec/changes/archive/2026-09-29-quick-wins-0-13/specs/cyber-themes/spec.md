## ADDED Requirements

### Requirement: Token Colours per Theme
Each theme SHALL define a foreground colour for each automatic highlighting kind (IP address, UUID, URL, duration, file path), each with a contrast ratio of at least 4.5:1 against the theme's row background, and switching the theme SHALL repaint the automatic token colours with the new theme's colours.

#### Scenario: Switching theme
- **WHEN** automatic highlighting is on and the user switches from Tron to Light
- **THEN** the IP addresses in the drawn rows change to the Light theme's IP colour.

#### Scenario: Readable on every theme
- **WHEN** the contrast of every token colour is measured against its theme's background
- **THEN** every ratio is at least 4.5:1.
