## MODIFIED Requirements

### Requirement: Renderer Setting
The Settings dialog SHALL offer the renderer choice `auto`, `glow`, `wgpu` and `software` (labelled as not recommended, see the software renderer requirement), persisted as `renderer` in `fasttail.ini`, with a note that it applies at the next start. The renderer choice, `--renderer` and `FASTTAIL_RENDERER` SHALL apply to the graphical interface only; the terminal interface SHALL ignore them and SHALL NOT create any graphics context.

#### Scenario: Pinning wgpu
- **WHEN** the user selects `wgpu` in Settings and restarts
- **THEN** the application starts directly on wgpu and the chip reads `WGPU`.

#### Scenario: Renderer ignored by the terminal interface
- **WHEN** `renderer = glow` is set and the user runs `fasttail --tui app.log` over SSH on a machine without OpenGL
- **THEN** the terminal interface opens `app.log` and no OpenGL or wgpu error is printed.

## ADDED Requirements

### Requirement: Interface Setting
The Settings dialog SHALL offer, next to the renderer choice, "Interface" with the entries "Graphical" and "Terminal (experimental)", persisted as `interface=gui|tui` in `[general]` of `fasttail.ini` (default `gui`), with a note that it applies at the next start and that `--gui` on the command line always opens the graphical interface. Choosing Terminal SHALL NOT close or change the running window. On Windows, when `fasttail-tui.exe` is not next to `fasttail.exe`, the Terminal entry SHALL stay selectable and show a warning that names the missing file. The interface choice and the renderer choice SHALL be independent: changing one SHALL NOT change the other.

#### Scenario: Switching to the terminal interface
- **WHEN** the user selects "Terminal (experimental)" in Settings, closes FastTail and double-clicks `fasttail.exe` on Windows
- **THEN** `fasttail.ini` holds `interface=tui`, the renderer key is unchanged, and a console window opens with the terminal interface and the same streams.

#### Scenario: Going back to the GUI
- **WHEN** `interface=tui` is set and the user runs `fasttail.exe --gui`, selects "Graphical" in Settings and restarts with a double click
- **THEN** the graphical window opens and `fasttail.ini` holds `interface=gui`.

#### Scenario: Terminal executable missing
- **WHEN** the Settings dialog is opened on Windows with no `fasttail-tui.exe` next to `fasttail.exe`
- **THEN** the "Terminal (experimental)" entry shows a warning naming `fasttail-tui.exe` and the directory it is expected in.
