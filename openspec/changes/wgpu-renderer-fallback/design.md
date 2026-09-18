## Context

`main.rs` calls `eframe::run_native` once with the default renderer, which is `glow` because it is the only enabled backend. eframe 0.36 keeps its winit event loop in a thread-local and runs it with `run_app_on_demand`, so `run_native` can be called again in the same thread after a failed run. A failure to create the OpenGL context inside `GlowWinitApp` propagates as `Err(eframe::Error)` from `run_native` (glutin errors are returned, not panicked), which is the hook for the fallback.

`CreationContext` carries `gl: Option<Arc<glow::Context>>` when running on glow and `wgpu_render_state: Option<RenderState>` when running on wgpu; the latter exposes `adapter.get_info()` with name, backend API, device type and driver.

## Goals / Non-Goals

**Goals:** start on machines without OpenGL, keep OpenGL as the default where it works, let the user see and force the backend, keep the change small and testable.

**Non-Goals:** switching backends at runtime without restart; per-monitor or per-window backends; tuning wgpu (MSAA, present modes) beyond defaults.

## Decisions

**D1. Order: glow first, wgpu on error.** glow is the backend every existing user runs on today, so it stays the default; wgpu is the safety net. The reverse order would change the rendering path for everyone to fix a minority of machines.

**D2. Fallback only on `Err` from `run_native`, not on panics.** The glutin path returns errors; a panic during backend creation would already be caught by the crash handler and produce a report, which is the right behaviour for an unknown failure. If a real machine shows a panic instead of an error, the design is revisited with that report.

**D3. Resolution order `FASTTAIL_RENDERER` env > `renderer` in `fasttail.ini` > `auto`.** The env var makes the fallback testable on any machine (`FASTTAIL_RENDERER=wgpu`) without touching the config, and gives support a one-line instruction. `command-line-arguments` will add `--renderer` on top when it lands.

**D4. No automatic persistence of the fallback.** A failed glow attempt costs well under a second; persisting `renderer = wgpu` automatically would silently pin a machine to wgpu after a transient driver problem. The chip's `fallback` suffix tells the user what happened, and Settings lets them pin it deliberately.

**D5. Indicator = a chip in the status bar, details on hover and in About.** The status bar is always visible and already the home of the active file path; a two-to-five character chip (`GL`, `WGPU`, `WGPU fallback`) does not compete with it. The tooltip shows `<API> · <adapter> · <driver>`; About shows the same line under the build info so it lands in bug reports.

**D6. `ActiveRenderer` is built once in `FastTailApp::new`** from the `CreationContext`, since the glow context strings and the wgpu adapter info are available there and never change for the life of the window.

**D7. Cost accepted.** Measured on 2026-09-19 on the maintainer's workstation (Windows x86_64, thin LTO):

| Measure | Before (glow only) | After (glow + wgpu) |
|---|---|---|
| `fasttail.exe` release size | 11.14 MB | 18.02 MB (+6.9 MB) |
| Release build, cold dependencies | (not measured) | 224 s including the whole wgpu tree |
| Release rebuild of the crate only | 54 s | 76 s |
| `cargo test`, cold dev dependencies | (not measured) | 143 s |

Runtime check on the same machine (NVIDIA Quadro P1000): default start reports `GL (3.3.0 NVIDIA 582.08 · Quadro P1000/PCIe/SSE2)`; `FASTTAIL_RENDERER=wgpu` reports `WGPU (Vulkan · Quadro P1000 · 582.08)`, so wgpu picks Vulkan when a driver offers it and falls to Direct3D 12 (WARP without a GPU) otherwise; on Linux Vulkan then GL, on macOS Metal. The compressed Linux/Windows release archives will grow by a similar amount.

## Risks / Trade-offs

- [wgpu adds a large dependency tree] → measured; acceptable for a desktop app that already ships egui, and it is the only route to machines without OpenGL.
- [Second `run_native` call misbehaves on some platform] → covered by the event-loop reuse eframe itself relies on; if a platform fails, the fallback path logs and exits with the original error, which is no worse than today.
- [Different look between backends] → egui renders identically; only MSAA defaults may differ slightly.

## Migration Plan

1. Land the change; users on working OpenGL machines see a `GL` chip and nothing else changes.
2. Give the affected machine a test build; expect `WGPU fallback` in the chip and a stderr line with the glow error.
3. Rollback: revert the commit (removes the `wgpu` feature).

## Open Questions

- None until the affected machine reports back.
