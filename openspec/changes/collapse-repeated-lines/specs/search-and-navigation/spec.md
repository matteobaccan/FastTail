## ADDED Requirements

### Requirement: Navigation in Collapsed Groups
While a stream's collapse mode is on, search hits and bookmarks SHALL keep referring to file lines and the match counter SHALL count every hit, hidden or not. A collapsed row SHALL show the match marker when any line it stands for is a hit and the bookmark marker when any of them is bookmarked. Match navigation (F3 and SHIFT + F3) SHALL land on the group row for the first hit inside a collapsed group and the next step SHALL move past the other hits of that group. Showing one exact line that is hidden in a collapsed group, from the search results pane, the Find results tab, go to line, bookmark navigation or a timeline click, SHALL expand that group and select the line. Toggling a bookmark on a collapsed row SHALL apply to its first line. The overview strip and the scrollbar SHALL be proportional to the collapsed rows, and marks of hidden lines SHALL be drawn at their group row. Line numbers SHALL show the number of each row's first line.

#### Scenario: Stepping over a collapsed group
- **WHEN** a collapsed `×200` group holds 200 hits of the search `timeout`, followed by one more hit on a later line, and the user presses F3 twice from the top
- **THEN** the first press selects the group row, the second selects the later line, and the counter shows 201 hits.

#### Scenario: Go to a hidden line
- **WHEN** lines 1,000 to 1,499 form a collapsed group and the user goes to line 1,250
- **THEN** the group is expanded and line 1,250 is selected and in view.

#### Scenario: Bookmark inside a group
- **WHEN** line 1,300 is bookmarked and lies inside a collapsed group
- **THEN** the group row shows the bookmark marker and the overview strip marks the group's position.
