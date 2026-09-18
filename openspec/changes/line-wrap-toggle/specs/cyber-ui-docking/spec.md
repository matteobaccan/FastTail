## ADDED Requirements

### Requirement: Line Wrap Mode
Each text stream SHALL offer a wrap toggle (stream bar button and Alt+W) that soft-wraps rows at the viewport width. Wrapped rows SHALL keep their line number, marker and highlight, navigation by line index (search, bookmarks, go-to, paging) SHALL keep working, and the toggle state SHALL be persisted per stream in the workspace.

#### Scenario: Wrapping a long JSON line
- **WHEN** wrap is enabled on a stream containing a 3,000-character line
- **THEN** the line is displayed on several rows within the viewport width, with no horizontal scrollbar, and the line number is shown once.

#### Scenario: Search jump in wrap mode
- **WHEN** wrap is enabled and the user presses F3
- **THEN** the viewport scrolls so that the matching line is fully visible.
