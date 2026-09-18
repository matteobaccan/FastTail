## Why

FastTail is built with the `glow` (OpenGL) backend only. On machines without a usable OpenGL driver (Remote Desktop sessions, Hyper-V and other VMs, servers with basic display adapters, some corporate images) the OpenGL context cannot be created and the application does not start at all. eframe also supports `wgpu`, which on Windows uses Direct3D 12 and falls back to the WARP software rasteriser, so it works even without a GPU driver. There is also no way today to tell which backend the running instance uses.

## What Changes

- Enable eframe's `wgpu` feature next to `glow`.
- Startup tries the OpenGL backend first; if eframe returns an error, it retries with wgpu (`renderer = auto`, the default). The config key `renderer` and the environment variable `FASTTAIL_RENDERER` accept `auto`, `glow` or `wgpu` to force a backend.
- The status bar shows a small renderer chip: `GL` or `WGPU`, followed by `fallback` when the automatic retry happened; its tooltip and the About dialog show the adapter details (GPU name, API such as Dx12/Vulkan/Metal/GL, driver).
- A Settings selector exposes the same three values; a change takes effect at the next start.
- Startup failures of the first backend are logged to stderr with the eframe error so the fallback is explainable.

## Capabilities

### New Capabilities
- `rendering-backend`: backend selection, automatic fallback and the visible renderer indicator.

### Modified Capabilities
- (none)

## Impact

- `Cargo.toml`: `eframe` features gain `wgpu`. Cost measured at implementation time and recorded in the design: expected several MB more on every binary and a longer cold build.
- `src/main.rs`: renderer resolution (env, config, auto), first attempt with glow, retry with wgpu on `Err`, stderr log.
- `src/renderer.rs` (new): `RendererChoice`, `ActiveRenderer` description built from `CreationContext` (`gl` context strings or `wgpu_render_state.adapter.get_info()`).
- `src/ui/app.rs`: store `ActiveRenderer`, chip in the status bar, details in About, Settings selector; `src/config.rs`: `renderer` key; `src/i18n.rs`: keys.
- CI: no change; the Linux runners already have the GL/Vulkan development libraries wgpu needs at build time (none) and the release archives grow by the wgpu code.
