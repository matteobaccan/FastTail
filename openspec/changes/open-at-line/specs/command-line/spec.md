## ADDED Requirements

### Requirement: Open at a Line
The command line of `fasttail`, `fasttail --tui` and `fasttail-tui` SHALL accept a line target as a `path:line` or `path:line:column` argument and as the `--line N` option. A `:line` or `:line:column` suffix SHALL be taken as a line only when the whole argument is not an existing file and the part before the suffix is; otherwise the argument SHALL be opened as a path. `--line N` SHALL apply to the first path; a suffix on a path SHALL win over it. Lines SHALL be 1-based and the column SHALL be ignored. The stream SHALL open with that line selected and centred and follow paused, as go to line does; when the file is still being indexed the landing SHALL wait for the line, and a line past the end SHALL land on the last line with a notice. With `--print`, `--line N` SHALL start the output at line N.

#### Scenario: Compiler-style location
- **WHEN** the user runs `fasttail app.log:1204:7` and `app.log` has 5000 lines
- **THEN** `app.log` opens with line 1204 selected and centred, and follow is paused.

#### Scenario: Windows drive letter and real names
- **WHEN** the user runs `fasttail C:\logs\a.log` or opens an existing file named `trace:12`
- **THEN** each argument is opened as the path it is, with no line target.

#### Scenario: Past the end
- **WHEN** the user runs `fasttail --line 9000 app.log` and the file has 8123 lines
- **THEN** the view lands on line 8123 and a notice says line 9000 is past the end.

#### Scenario: Print from a line
- **WHEN** the user runs `fasttail --print --line 100 app.log`
- **THEN** the output starts with line 100.
