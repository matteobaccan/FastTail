## Context

Standard input is copied into a spool by `StdinStream`; `resume` appends a new reader after
an end. The window reads the clipboard through egui (`arboard` in the terminal build).

## Goals / Non-Goals

**Goals:** clipboard to stream in one key; appends.

**Non-Goals:** clipboard watching, rich content.

## Decisions

### D1. A spooled buffer
The text becomes a reader over its bytes handed to the standard-input copier (same limits,
same encoding detection); the stream's path is `<clipboard N>` and is skipped by the
workspace, as `<stdin>` is.

### D2. Size
At most the standard-input spool cap; a larger clipboard is refused with a notice.

### Implementation notes (deviations)

- The window reads the clipboard through egui, not `arboard`: `Ctrl + Shift + V` reaches
  the app as egui's paste event (egui-winit turns any `Ctrl + V` into one), and the menu,
  palette and Append ask for one with `ViewportCommand::RequestPaste`. The terminal reads
  it with `arboard`, which it already links.
- The stream is `<clipboard N>`, its own engine field (`TailEngine::clipboard`) over the
  standard-input copier, so `is_stdin` stays true for standard input only.
- A refused paste is reported in the focused stream's notice.

## Risks / Trade-offs

- [Ctrl + Shift + V taken by a terminal emulator] → the terminal uses the palette entry; the
  window binding is remappable.

## Open Questions

- Name the stream after the first line instead of the time? Proposed: the time.
