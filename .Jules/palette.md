## 2026-04-18 - Egui Empty State Framing & Multi-file Dialog handling
**Learning:** In `egui`, drawing an empty state inside an existing `&mut egui::Ui` container should use `egui::Frame::new().show(ui, ...)` rather than `CentralPanel`, which expects `&Context`. Additionally, when using multi-file file pickers (`rfd::FileDialog::pick_files`), all returned paths must be opened in a loop rather than captured into a single scalar option.
**Action:** Use `egui::Frame` when styling empty states within existing `Ui` panels, and handle all paths returned from multi-file dialogs.
