# Rendering Backend Specification

## Purpose
Defines how FastTail chooses between the wgpu and OpenGL rendering backends, the automatic fallback when wgpu cannot start, and how the active backend is shown to the user.

## Requirements

### Requirement: Renderer Selection with Automatic Fallback
The application SHALL support both the `wgpu` and the OpenGL (`glow`) rendering backends. The backend SHALL be resolved from the `FASTTAIL_RENDERER` environment variable, then the `renderer` key in `fasttail.ini`, then the default `auto`; accepted values are `auto`, `glow` and `wgpu`. With `auto` the application SHALL start with wgpu and, if eframe returns an error before the window runs, SHALL retry with OpenGL and log the wgpu error to stderr. With `glow` or `wgpu` no fallback SHALL happen. The OpenGL backend SHALL run without vsync, because some OpenGL drivers busy-wait for the vertical blank and turn every continuous repaint into a full core of CPU.

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
