# Rendering Backend Specification

## Purpose
Defines how FastTail chooses between the OpenGL and wgpu rendering backends, the automatic fallback when OpenGL cannot start, and how the active backend is shown to the user.

## Requirements

### Requirement: Renderer Selection with Automatic Fallback
The application SHALL support both the OpenGL (`glow`) and the `wgpu` rendering backends. The backend SHALL be resolved from the `FASTTAIL_RENDERER` environment variable, then the `renderer` key in `fasttail.ini`, then the default `auto`; accepted values are `auto`, `glow` and `wgpu`. With `auto` the application SHALL start with OpenGL and, if eframe returns an error before the window runs, SHALL retry with wgpu and log the OpenGL error to stderr. With `glow` or `wgpu` no fallback SHALL happen.

#### Scenario: Machine without a usable OpenGL driver
- **WHEN** FastTail starts with `renderer = auto` on a Remote Desktop session where the OpenGL context cannot be created
- **THEN** the window opens on wgpu, and stderr contains the OpenGL error and the line `renderer: falling back to wgpu`.

#### Scenario: Forcing wgpu on a working machine
- **WHEN** the user runs FastTail with `FASTTAIL_RENDERER=wgpu`
- **THEN** the window opens on wgpu without trying OpenGL, and the indicator shows `WGPU` without the fallback suffix.

#### Scenario: Forced backend fails
- **WHEN** `renderer = glow` is set and the OpenGL context cannot be created
- **THEN** the application exits with the OpenGL error and does not try wgpu.

### Requirement: Visible Renderer Indicator
The status bar SHALL show a chip reading `GL` or `WGPU`, with the suffix `fallback` when the automatic retry took place. Its tooltip and the About dialog SHALL show the adapter details: graphics API (for wgpu: Dx12, Vulkan, Metal or GL; for OpenGL: the GL version) and the adapter or renderer name, plus the driver string when available.

#### Scenario: Running on OpenGL
- **WHEN** the application runs on the OpenGL backend
- **THEN** the chip reads `GL` and its tooltip names the OpenGL renderer string and version.

#### Scenario: Running on wgpu after fallback
- **WHEN** the automatic retry started the wgpu backend on Direct3D 12 with the WARP adapter
- **THEN** the chip reads `WGPU fallback` and the tooltip reads `Dx12 · Microsoft Basic Render Driver` (or the actual adapter name).

### Requirement: Renderer Setting
The Settings dialog SHALL offer the renderer choice `auto`, `glow`, `wgpu`, persisted as `renderer` in `fasttail.ini`, with a note that it applies at the next start.

#### Scenario: Pinning wgpu
- **WHEN** the user selects `wgpu` in Settings and restarts
- **THEN** the application starts directly on wgpu and the chip reads `WGPU`.
