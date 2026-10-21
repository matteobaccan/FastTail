## Why

The stream bar of the window shows the view modes as four buttons, `🔤 TXT`, `🔢 HEX`,
`⚙ ASM` and `📝 MD`, all of them always. On a file above the Markdown limit
(`markdown_max_mb`, 1 MB by default) `📝 MD` is still drawn and a click only shows a
notice, so the user sees a mode that cannot be used. Four buttons also take room in a
crowded bar. The maintainer asked on 2026-10-21 for a drop-down that lists only the modes
the stream can use.

## What Changes

- The four buttons become **one drop-down** in the same place of the stream bar. It shows
  the current mode (icon, name, in the mode's colour) and lists only the modes the stream
  can use now: TXT, HEX and ASM always, MD only while the file is within the Markdown
  limit. Each entry keeps its tooltip.
- When MD is not offered, a disabled line at the end of the list says why ("MD: file above
  the 1 MB Markdown limit"), so the limit stays discoverable.
- The command palette and the key bindings (`view.text`, `view.hex`, `view.asm`,
  `view.markdown`) work as before; MD from the palette on a large file still shows the
  notice.
- No change to the terminal interface (its `[TXT]` chip already cycles TXT, HEX and ASM;
  the terminal has no MD view), the engine, or saved files.

Target release: a **nightly patch (0.20.x)**. Priority: **medium**. Effort: **XS**.

### Non-goals

- New view modes, or changing when MD is available (the limit stays `markdown_max_mb`).
- Hiding ASM on text files: any file has bytes to decode.

## Capabilities

### Modified Capabilities

- `cyber-ui-docking`: Stream Toolbar Affordances (the view mode drop-down).

## Impact

- `src/ui/dock.rs` (the stream bar), `src/i18n.rs` (the "why no MD" line), README,
  `docs/ui-design.md`, CHANGELOG.
