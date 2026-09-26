## 2026-04-20 - Disabled Control Tooltips in egui
**Learning:** In `egui`, standard `.on_hover_text(...)` tooltips are suppressed when a widget is disabled (e.g. `add_enabled(false, ...)`). For interactive controls like "Open" or reordering buttons that can be disabled based on state, users need explicit feedback on why the control is disabled.
**Action:** Always chain `.on_disabled_hover_text(...)` when disabling interactive buttons in `egui` so users receive clear contextual guidance.

## 2026-04-19 - Log Stream Empty States Differentiation
**Learning:** In log viewing UIs with active tab interfaces, empty states when no lines are visible must distinguish between an empty source file (`total_lines == 0`) and filtered-out lines (`total_lines > 0`). Reusing "no file open" messages inside an open tab confuses users into thinking file loading failed.
**Action:** Always check total vs visible line counts in stream renderers to show accurate context-specific empty state messages (`file_empty` vs `no_matching_lines`).

## 2026-04-18 - Egui Empty State Framing & Multi-file Dialog handling
**Learning:** In `egui`, drawing an empty state inside an existing `&mut egui::Ui` container should use `egui::Frame::new().show(ui, ...)` rather than `CentralPanel`, which expects `&Context`. Additionally, when using multi-file file pickers (`rfd::FileDialog::pick_files`), all returned paths must be opened in a loop rather than captured into a single scalar option.
**Action:** Use `egui::Frame` when styling empty states within existing `Ui` panels, and handle all paths returned from multi-file dialogs.
