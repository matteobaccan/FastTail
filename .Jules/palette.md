## 2025-03-08 - Contextual Disabled Tooltips in egui
**Learning:** In egui, standard `.on_hover_text(...)` tooltips are completely suppressed on disabled controls (`add_enabled` with `false`). Chaining `.on_disabled_hover_text(...)` is required to display contextual guidance explaining why a control is disabled (e.g. "Select a row first" or "Single-line JSON comparison unavailable").
**Action:** Whenever adding hover tooltips to controls that can be disabled using `ui.add_enabled(...)`, always chain `.on_disabled_hover_text(...)` with a localized message describing the requirement.
