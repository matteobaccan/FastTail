## MODIFIED Requirements

### Requirement: Stream Toolbar Affordances
The stream toolbar SHALL show the state of its toggles (follow, auto-update, line numbers, wrap, TXT/HEX/MD) with a tinted fill and a border, not with the label colour alone, so the active state is readable on the light theme as well. Follow and Auto-update SHALL be drawn as check boxes, a checked mark (`☑`) when on and an empty one (`☐`) when off before the translated label, and SHALL NOT use the `▶` / `■` play and stop icons. The toggle that reads appended lines from disk SHALL be labelled "Auto-update" (translated) in the toolbar, its tooltip, the command palette and status texts; its action id, its i18n keys and the saved stream state SHALL stay unchanged. The TXT/HEX/MD switcher SHALL keep a fixed position in the toolbar, before the controls that appear and disappear with the view mode, so it does not move under the pointer when the mode changes.

#### Scenario: Active toggle on the light theme
- **WHEN** the light theme is active and line wrap is enabled
- **THEN** the wrap button is drawn filled and outlined in the accent colour, clearly distinct from the inactive buttons next to it.

#### Scenario: Switching to HEX does not move the switcher
- **WHEN** the user switches a stream from text to HEX, and the line-number and wrap buttons disappear
- **THEN** the TXT/HEX/MD buttons stay where they were.

#### Scenario: Two independent check boxes
- **WHEN** a stream has follow off and auto-update on
- **THEN** the toolbar shows `☐ Follow` and `☑ Auto-update`, and clicking `☐ Follow` turns follow on without changing auto-update.

#### Scenario: Translated label
- **WHEN** the language is Italian
- **THEN** the toggle reads the Italian translation of "Auto-update" and no toolbar toggle shows the word "Monitor".
