## Context

Window: `src/ui/dock.rs` draws the stream bar with `toggle_button(ui, theme, label, on,
colour)`; the Follow and Monitor labels are literals (`"▶ Follow"`, `"■ Monitor"`), the
tooltips come from `i18n` (`tip_follow_tail`, `tip_monitor`). Terminal: `src/tui/bars.rs`
builds chips `{0} Follow` / `{0} Monitor` with `▶` / `■` (`>` / `#` in ASCII mode) and
`src/tui/app.rs` shows "Monitor on" / "Monitor off" when the chip is clicked.

Only labels and drawing change, on the UI thread; no engine, polling or memory change.

## Goals / Non-Goals

**Goals:** the two toggles read as independent options, Monitor is called Auto-update,
every label is translated.

**Non-Goals:** behaviour, tab-title markers, the terminal's FOLLOW / PAUSED badge.

## Decisions

1. **Check box mark in the label**, not a separate widget: `[x]` / `[ ]` in the terminal
   (ASCII-safe, the same in every terminal and at 16 colours); in the window `☑` / `☐`
   (both in the bundled fonts, to be verified with the glyph check) before the label, with
   the existing tinted fill and border kept, so the toolbar keeps its height and layout.
   Alternative: egui's `Checkbox` widget; rejected, it does not match the toolbar's toggle
   style and would change its height.
2. **Name: "Auto-update"** (maintainer's choice). Translations follow the meaning "updates
   by itself as the file grows", e.g. it "Aggiornamento automatico" (chip: "Auto-aggiorna"
   if the full form is too wide for the bar; the length is checked at 80 columns in the
   terminal), de "Auto-Aktualisierung", fr "Mise à jour auto", es "Actualización auto".
3. **Ids unchanged:** `is_watching`, `ActionId::Monitor`, `stream.monitor.toggle` and the
   i18n keys stay, so saved key bindings and files keep working; only the shown texts change.
4. **Status texts:** "Auto-update on" / "Auto-update off" in both interfaces, replacing the
   upper-case `MONITOR: ACTIVE` / `MONITOR: STOPPED`.
5. **Compressed streams:** Follow stays disabled as today, drawn `[ ] Follow` greyed.

## Risks / Trade-offs

- [Longer label in some languages pushes chips off a narrow terminal bar] → the chips
  already wrap or drop by width; a capture test at 80 columns in German and Japanese.
- [Users who learned "Monitor"] → the CHANGELOG entry names the rename; the palette entry
  keeps "monitor" as a search alias.

## Migration Plan

None: no saved data changes.

## Open Questions

- The window's tab title shows `▶` / `■` for the same state: switch it to a different mark
  (or drop it, since the toggle is one click away)? Proposed: leave it for now.
- The terminal's top border shows `FOLLOW` / `PAUSED`: keep it next to `[x] Follow`?
  Proposed: keep it, it is visible even when the stream bar is hidden in short windows.
