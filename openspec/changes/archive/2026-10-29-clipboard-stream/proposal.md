## Why

Log text often arrives in the clipboard: copied from a chat, a CI page, a ticket or a
remote terminal. Today it has to be saved to a file first, or piped into `fasttail -` from a
terminal. Chipmunk's request to paste text as a source (#495) shows the need; editors open
an untitled buffer for it. Gap 26 (post-0.27.0 scan), low-medium, small.

## What Changes

- **Paste as stream** (`Ctrl + Shift + V` outside a text field, the command palette, the
  📁 menu): the clipboard text opens as a new stream `clipboard 14:02`, spooled like
  standard input, with every stream feature.
- Pasting again into a clipboard stream (its stream bar's **Append**) adds the text at the
  end, so several pastes build one log.
- The terminal interface has the same action (its palette entry), reading the clipboard
  through the terminal clipboard support it already has.
- A clipboard stream is not restored on the next start (the text is not kept), like stdin.

Target release: **0.28.0** (proposed after the post-0.27.0 scan of 2026-10-28; planned when the maintainer agrees), per the release plan in `docs/competitor-analysis.md` section 8. Priority: **Low**. Effort: **S**.

### Non-goals

- Images or rich text; only plain text.
- Watching the clipboard for changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stream-engine`: the requirements below are added.

## Impact

- `src/stdin_source.rs`: a copier started from a byte buffer (`start_named` with a slice
  reader), Append through `resume`.
- `src/ui/app.rs` action, palette entry, stream bar Append; `src/actions.rs` id
  `stream.paste`; `src/tui` palette entry.
- README, CHANGELOG, `docs/ui-design.md`, i18n.
