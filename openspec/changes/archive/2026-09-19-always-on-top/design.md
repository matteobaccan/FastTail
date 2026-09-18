## Context

eframe exposes `egui::ViewportCommand::WindowLevel`, supported on Windows, macOS and X11/Wayland through winit. The custom title bar already hosts window control buttons.

## Goals / Non-Goals

**Goals:** one click to pin, persisted, keyboard toggle.

**Non-Goals:** per-floating-window pinning; transparency/opacity (a different feature).

## Decisions

- **Apply at startup after the first frame** (`ctx.send_viewport_cmd` in the first `update`) rather than in `ViewportBuilder`, because the builder has no window-level option in the current eframe.
- **Pin icon in the title bar** next to the minimise button, highlighted with the theme accent when active; the Settings checkbox mirrors it.

## Risks / Trade-offs

- [Wayland compositors may ignore window levels] → best effort, documented.
