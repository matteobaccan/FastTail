# Rendering Backend Specification

## Purpose
Defines how FastTail chooses between the wgpu and OpenGL rendering backends, the automatic fallback when wgpu cannot start, and how the active backend is shown to the user.

## Requirements

### Requirement: Renderer Selection with Automatic Fallback
The application SHALL support both the `wgpu` and the OpenGL (`glow`) rendering backends. The backend SHALL be resolved from the `FASTTAIL_RENDERER` environment variable, then the `renderer` key in `fasttail.ini`, then the default `auto`; accepted values are `auto`, `glow`, `wgpu` and `software`. With `auto` the application SHALL start with wgpu and, if eframe returns an error before the window runs, SHALL retry with OpenGL and log the wgpu error to stderr. With `glow`, `wgpu` or `software` no fallback SHALL happen except the software-to-OpenGL retry defined below. Both backends SHALL run without vsync, because some graphics drivers (including NVIDIA on Windows) busy-wait for the vertical blank and turn every continuous repaint into a full core of CPU.

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
- **THEN** the process uses in the order of 12% of one core on wgpu and no more than about 40% on OpenGL without vsync, instead of a full core with vsync.

### Requirement: Software (CPU) Renderer Fallback
The application SHALL accept `software` (alias `cpu`) as a renderer value in the CLI, the `FASTTAIL_RENDERER` environment variable and the `renderer` ini key. With `software` the wgpu path SHALL select a CPU adapter (WARP on Windows, llvmpipe on Linux) regardless of the GPUs present, and SHALL fall back to OpenGL if no CPU adapter can be created. Because WARP distributes rasterization across every logical core, software rendering SHALL be treated as an unoptimized last-resort fallback: while it is active a persistent banner SHALL warn that a GPU is required for optimal performance. The idle CPU cost of WARP is inherent (it is independent of present mode, frame pacing, focus and window occlusion, and only stops when the window is minimized), so the application SHALL NOT attempt to hide it.

#### Scenario: Forcing software rendering
- **WHEN** the user runs FastTail with `--renderer software`
- **THEN** the window opens on the wgpu CPU adapter (WARP/llvmpipe), the status chip reads `WGPU`, and a banner warns that a GPU is required for optimal performance.

#### Scenario: No CPU adapter available
- **WHEN** `renderer = software` is set and wgpu cannot create a CPU adapter
- **THEN** the application retries with OpenGL and, if that also fails, exits with the OpenGL error.

### Requirement: Visible Renderer Indicator
The status bar SHALL show a chip reading `WGPU` or `GL`, with the suffix `fallback` when the automatic retry took place. Its tooltip and the About dialog SHALL show the adapter details: graphics API (for wgpu: Dx12, Vulkan, Metal or GL; for OpenGL: the GL version) and the adapter or renderer name, plus the driver string when available.

#### Scenario: Running on OpenGL
- **WHEN** the application runs on the OpenGL backend
- **THEN** the chip reads `GL` and its tooltip names the OpenGL renderer string and version.

#### Scenario: Running on OpenGL after fallback
- **WHEN** the automatic retry started the OpenGL backend on a Mesa software renderer
- **THEN** the chip reads `GL fallback` and the tooltip names the OpenGL renderer string and version.

### Requirement: Renderer Setting
The Settings dialog SHALL offer the renderer choice `auto`, `glow`, `wgpu`, persisted as `renderer` in `fasttail.ini`, with a note that it applies at the next start.

#### Scenario: Pinning wgpu
- **WHEN** the user selects `wgpu` in Settings and restarts
- **THEN** the application starts directly on wgpu and the chip reads `WGPU`.
