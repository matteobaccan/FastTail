# Rendering Backend Specification

## Purpose
Defines how FastTail chooses between the wgpu and OpenGL rendering backends, the automatic fallback when wgpu cannot start, and how the active backend is shown to the user.
## Requirements
### Requirement: Renderer Selection with Automatic Fallback
The application SHALL support both the `wgpu` and the OpenGL (`glow`) rendering backends. The backend SHALL be resolved from the `FASTTAIL_RENDERER` environment variable, then the `renderer` key in `fasttail.ini`, then the default `auto`; accepted values are `auto`, `glow`, `wgpu` and `software`. With `auto` the application SHALL start with wgpu and, if eframe returns an error before the window runs, SHALL retry with OpenGL and log the wgpu error to stderr. With `glow`, `wgpu` or `software` no fallback SHALL happen except the software-to-OpenGL retry defined below. OpenGL SHALL run without vsync, because some graphics drivers (including NVIDIA on Windows) busy-wait for the vertical blank and turn every continuous repaint into a full core of CPU. wgpu SHALL present with `AutoVsync` on a GPU adapter and with `Immediate` on the software (CPU) adapter, where there is no real vertical blank to wait for and egui's own frame pacing throttles the work.

#### Scenario: Machine without a usable wgpu backend
- **WHEN** FastTail starts with `renderer = auto` on a machine where neither Direct3D 12 nor Vulkan can create a device
- **THEN** the window opens on OpenGL, and stderr contains the wgpu error and the line `renderer: falling back to OpenGL`.

#### Scenario: Forcing wgpu on a working machine
- **WHEN** the user runs FastTail with `FASTTAIL_RENDERER=wgpu`
- **THEN** the window opens on wgpu without trying OpenGL, and the indicator shows `WGPU` without the fallback suffix.

#### Scenario: Forced backend fails
- **WHEN** `renderer = glow` is set and the OpenGL context cannot be created
- **THEN** the application exits with the OpenGL error and does not try wgpu.

#### Scenario: Continuous repaint cost
- **WHEN** the pointer moves over the window continuously for 15 seconds on an NVIDIA Windows machine
- **THEN** the OpenGL backend, running without vsync, uses no more than about 40% of one core instead of the full core it used with vsync.

### Requirement: Software (CPU) Renderer Fallback
The application SHALL accept `software` (alias `cpu`) as a renderer value in the CLI, the `FASTTAIL_RENDERER` environment variable and the `renderer` ini key. With `software` the wgpu path SHALL select a CPU adapter (WARP on Windows, llvmpipe on Linux) regardless of the GPUs present, and SHALL fall back to OpenGL if no CPU adapter can be created. Because WARP distributes rasterization across every logical core, software rendering SHALL be treated as an unoptimized last-resort fallback: while it is active a persistent banner SHALL warn that a GPU is required for optimal performance. The idle CPU cost of WARP is inherent (it is independent of present mode, frame pacing, focus and window occlusion, and only stops when the window is minimized), so the application SHALL NOT attempt to hide it.

#### Scenario: Forcing software rendering
- **WHEN** the user runs FastTail with `--renderer software`
- **THEN** the window opens on the wgpu CPU adapter (WARP/llvmpipe), the status chip reads `WGPU`, and a banner warns that a GPU is required for optimal performance.

#### Scenario: No CPU adapter available
- **WHEN** `renderer = software` is set and wgpu cannot create a CPU adapter
- **THEN** the application retries with OpenGL and, if that also fails, exits with the OpenGL error.

#### Scenario: The settings warn before the renderer is chosen
- **WHEN** the user opens the renderer drop-down in the settings
- **THEN** the software entry is labelled as not recommended, and selecting it shows a warning explaining that it rasterizes on the CPU and keeps consuming CPU even while the window is idle.

### Requirement: Visible Renderer Indicator
The status bar SHALL show a chip reading `WGPU` or `GL`, with the suffix `fallback` when the automatic retry took place. Its tooltip and the About dialog SHALL show the adapter details: graphics API (for wgpu: Dx12, Vulkan, Metal or GL; for OpenGL: the GL version) and the adapter or renderer name, plus the driver string when available.

#### Scenario: Running on OpenGL
- **WHEN** the application runs on the OpenGL backend
- **THEN** the chip reads `GL` and its tooltip names the OpenGL renderer string and version.

#### Scenario: Running on OpenGL after fallback
- **WHEN** the automatic retry started the OpenGL backend on a Mesa software renderer
- **THEN** the chip reads `GL fallback` and the tooltip names the OpenGL renderer string and version.

### Requirement: Renderer Setting
The Settings dialog of the graphical interface SHALL offer the renderer choice `auto`, `glow`, `wgpu` and `software` (labelled as not recommended, see the software renderer requirement), persisted as `renderer` in `fasttail.ini`, with a note that it applies at the next start. The renderer choice, `--renderer` and `FASTTAIL_RENDERER` SHALL apply to the graphical interface only; the terminal interface SHALL NOT show the renderer choice, SHALL keep the `renderer` key as it read it, and SHALL NOT create any graphics context.

#### Scenario: Pinning wgpu
- **WHEN** the user selects `wgpu` in Settings and restarts
- **THEN** the application starts directly on wgpu and the chip reads `WGPU`.

#### Scenario: Renderer ignored by the terminal interface
- **WHEN** `renderer = glow` is set and the user runs `fasttail --tui app.log` over SSH on a machine without OpenGL
- **THEN** the terminal interface opens `app.log`, no OpenGL or wgpu error is printed, and `renderer = glow` is still in `fasttail.ini` after it saves.

### Requirement: Interface Setting
The Settings dialog of both interfaces SHALL offer "Interface" with the entries "Graphical" and "Terminal", next to the renderer choice in the graphical interface, persisted as `interface=gui|tui` in `[general]` of `fasttail.ini` (default `gui`). Changing it SHALL save the key and offer "Switch now" or "At next start", as the terminal-interface capability defines, and SHALL say that `--gui` and `--tui` on the command line override it for one start. On Windows, when the other executable (`fasttail-tui.exe` or `fasttail.exe`) is not next to the running one, the other entry SHALL stay selectable and show a warning naming the missing file and the directory. In a build without the graphical interface the Graphical entry SHALL be disabled with a note that the GUI is not in this build. The interface choice and the renderer choice SHALL be independent: changing one SHALL NOT change the other.

#### Scenario: Choosing the terminal at next start
- **WHEN** the user selects "Terminal" in the GUI Settings, picks "At next start", closes FastTail and double-clicks `fasttail.exe` on Windows
- **THEN** `fasttail.ini` holds `interface=tui`, the renderer key is unchanged, and a console window opens with the terminal interface and the same streams.

#### Scenario: Going back to the GUI
- **WHEN** `interface=tui` is set and the user runs `fasttail.exe --gui`, selects "Graphical" in Settings, picks "At next start" and restarts with a double click
- **THEN** the graphical window opens and `fasttail.ini` holds `interface=gui`.

#### Scenario: Terminal executable missing
- **WHEN** the GUI Settings dialog is opened on Windows with no `fasttail-tui.exe` next to `fasttail.exe`
- **THEN** the "Terminal" entry shows a warning naming `fasttail-tui.exe` and the directory it is expected in.

#### Scenario: No experimental label
- **WHEN** the interface choice is shown in either interface, in any of the 16 languages
- **THEN** its entries read "Graphical" and "Terminal" (translated) with no "experimental" or similar qualifier.

