## Why

The stream bar shows two toggles, `▶ Follow` / `■ Follow` and `▶ Monitor` / `■ Monitor`,
in the window and in the terminal. The play / stop icons make them look like the Play and
Pause buttons of a player, as if one excluded the other, while they are independent:
Follow keeps the view at the end of the file, Monitor reads the lines appended to it.
"Monitor" does not say what it does either. The maintainer asked on 2026-10-06 for the
name **Auto-update** and for check boxes that say whether each feature is on. In the
window both labels are also hard-coded English strings, outside `i18n`.

## What Changes

- **Monitor is renamed Auto-update** everywhere the user sees it: the stream bar chip /
  button, its tooltip, the command palette entry, the status messages ("Auto-update on" /
  "Auto-update off", replacing "Monitor on" / "MONITOR: ACTIVE"), README and docs. The
  behaviour does not change: on, new lines are read as the file grows (a truncated or
  rotated file is reopened); off, the stream is a frozen snapshot with no disk reads.
- **Follow and Auto-update are drawn as check boxes**: `[x] Follow` / `[ ] Follow` and
  `[x] Auto-update` / `[ ] Auto-update` in the terminal; a check box plus the label, keeping
  the tinted fill of an active toggle, in the window. `▶` / `■` are no longer used for them.
- Both labels go through `i18n` in the window (today `"▶ Follow"` and `"▶ Monitor"` are
  literals) and are translated in every language.
- Internal names stay: `is_watching`, `ActionId::Monitor`, the action id
  `stream.monitor.toggle` (used by key bindings and the palette), the i18n keys. No key of
  `fasttail.ini` or session files changes, so older builds read the files as before.

Target release: the next **nightly patch (0.20.x)**. Priority: **medium**. Effort: **S**.

### Non-goals

- Merging Follow and Auto-update into one control: the combination "auto-update on,
  follow off" (read while the file grows, stay where you are) is a real use.
- The `▶` / `■` marker in the window's tab title and the `FOLLOW` / `PAUSED` badge in the
  terminal's top border (open question in the design).
- Any change to polling or to what follow does.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `cyber-ui-docking`: Stream Toolbar Affordances (Follow and Auto-update as check boxes,
  the Monitor toggle renamed Auto-update).
- `terminal-interface`: a new requirement for the Follow and Auto-update chips of the
  terminal stream bar.

## Impact

- `src/ui/dock.rs`: the Follow and Monitor buttons of the stream bar (labels through
  `i18n`, check box state).
- `src/tui/bars.rs`: the two chips; `src/tui/app.rs`: the status messages and tests that
  match `[▶ Monitor]`.
- `src/i18n.rs` (`tip_monitor`, `monitor_on`, `monitor_off`, `act_monitor`, new labels),
  `src/i18n_tui.rs` (`{0} Monitor`, `Monitor on`, `Monitor off`) in every language.
- README, `docs/ui-design.md` (stream bar diagram, toggle list, icon table), `docs/tui.md`,
  CHANGELOG.
