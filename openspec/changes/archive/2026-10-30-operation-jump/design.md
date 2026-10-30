## Context

The rule walk (`start_rule_seek` / `step_rule_seek`) walks visible lines in the background; the field parser gives a line's fields.

## Goals / Non-Goals

**Goals:** what the proposal lists, over the existing engine and files.

**Non-Goals:** Grouping by several ids at once.

## Decisions

### D1. Reuse the rule walk
The walk matches `field == value` instead of a rule.

## Risks / Trade-offs

- [Lines without fields] → the message says the row has no id.

## Open Questions

- None.
