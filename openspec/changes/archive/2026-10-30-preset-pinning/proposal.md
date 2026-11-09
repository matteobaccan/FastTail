## Why

Chipmunk 4.3 keeps recently used filter presets and lets the user pin some; FastTail's
presets are one list in creation order, long once a team shares many.

## What Changes

- Presets can be **pinned** (shown first, `📌`) and the list shows the **recently used** ones
  next (the last 5), then the rest by name.
- Saved as `pinned=true` and `last_used=` per preset; older builds ignore them. Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Low–Med**. Effort: **S**.

### Non-goals

- Preset folders.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `filters-and-highlighting`: the requirement below is added.

## Impact

`src/filter_preset.rs`, the presets dialogs of both interfaces, config round-trip; texts; docs.
