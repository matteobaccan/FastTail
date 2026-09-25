## ADDED Requirements

### Requirement: Combined Filter Terms
A stream SHALL accept up to 8 include terms and up to 8 exclude terms. The stream bar SHALL show the first term of each side in its include and exclude fields and SHALL indicate how many further terms are active; all terms SHALL be editable in the Filters window. A line SHALL pass the text filter when it matches every non-empty include term and none of the non-empty exclude terms; the minimum-level and time filters SHALL then apply as before, and a stack-trace continuation line SHALL still follow its parent entry unless an exclude term matches it. Every term SHALL be plain text or a regular expression according to the stream's case-sensitive and regex toggles, with no operator syntax: text typed in a term is matched as typed. A term whose regular expression does not compile SHALL be flagged in its row. Include and exclude terms SHALL be persisted per stream in the workspace and in session files, the first term of each side under the existing keys.

#### Scenario: Two conditions on the same line
- **WHEN** the include terms are `payment` and `timeout` and the exclude terms are `healthcheck` and `retry=0`
- **THEN** only lines containing both `payment` and `timeout` and containing neither `healthcheck` nor `retry=0` are visible.

#### Scenario: Operator characters are literal
- **WHEN** the include term is the plain text `a && !b`
- **THEN** only lines containing the literal text `a && !b` are visible.

#### Scenario: Extra terms are never hidden
- **WHEN** a stream has three include terms
- **THEN** the stream bar shows the first term in the include field with a `+2` indicator.

### Requirement: Filter Presets
The user SHALL be able to save the filter state of a stream as a named preset holding its include and exclude terms, the case-sensitive and regex toggles, the minimum level and the unknown-level toggle, and, when chosen at save time, the time range as typed. Presets SHALL be stored in `fasttail.ini`, SHALL NOT be part of session files, and SHALL have names unique without regard to case. A presets drop-down in the stream bar SHALL apply a preset to that stream or to all open streams, replacing their filter state in one recomputation; a preset without a time range SHALL leave the stream's time range unchanged. The drop-down SHALL show the name of the preset the stream's filter state equals, marked as modified once the state has been edited after applying it. Presets SHALL be renamed, deleted after confirmation, reordered and updated from a stream in the Filters window.

#### Scenario: Applying a saved preset
- **WHEN** the user saved `payment errors` as include `payment`, exclude `healthcheck`, `>= WARN`, and applies it to another stream
- **THEN** that stream's include field reads `payment`, its exclude field `healthcheck`, its level selector `>= WARN`, and the drop-down shows `payment errors`.

#### Scenario: Editing after applying
- **WHEN** the user applies `payment errors` and then adds the include term `timeout`
- **THEN** the drop-down shows `payment errors *` and offers to update the preset from the stream.

#### Scenario: Preset with a bare time range
- **WHEN** a preset saved with the time range from `14:02` to `14:05` is applied to a log whose first entry is dated 2026-09-20
- **THEN** the window covers 2026-09-20 14:02:00.000 to 14:05:59.999 on that log.

#### Scenario: Presets survive a restart
- **WHEN** the user saves two presets and restarts FastTail
- **THEN** both presets are listed in the drop-down in the same order.
