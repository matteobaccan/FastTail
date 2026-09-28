## MODIFIED Requirements

### Requirement: Combined Filter Terms
A stream SHALL accept up to 8 include terms and up to 8 exclude terms. The stream bar SHALL show the first term of each side in its include and exclude fields and SHALL indicate how many further terms are active; all terms SHALL be editable in the Filters window. A line SHALL pass the text filter when it matches every non-empty include term and none of the non-empty exclude terms; the minimum-level and time filters SHALL then apply as before, and a stack-trace continuation line SHALL still follow its parent entry unless an exclude term matches it. Every term SHALL be plain text or a regular expression according to the stream's case-sensitive and regex toggles, with no operator syntax: text typed in a term is matched as typed. The one exception SHALL be a field term, which exists only on a stream with an active field parser and is defined by the Field Filter Terms requirement of the structured-fields capability; on a stream without an active field parser, and for a term wrapped in double quotes, no term SHALL be read as a field term. A term whose regular expression does not compile SHALL be flagged in its row. Include and exclude terms SHALL be persisted per stream in the workspace and in session files, the first term of each side under the existing keys.

#### Scenario: Two conditions on the same line
- **WHEN** the include terms are `payment` and `timeout` and the exclude terms are `healthcheck` and `retry=0`
- **THEN** only lines containing both `payment` and `timeout` and containing neither `healthcheck` nor `retry=0` are visible.

#### Scenario: Operator characters are literal
- **WHEN** the include term is the plain text `a && !b`
- **THEN** only lines containing the literal text `a && !b` are visible.

#### Scenario: Extra terms are never hidden
- **WHEN** a stream has three include terms
- **THEN** the stream bar shows the first term in the include field with a `+2` indicator.

#### Scenario: Field syntax without a parser is text
- **WHEN** a stream has no active field parser and the exclude term is `retry=0`
- **THEN** the lines containing the text `retry=0` are hidden, exactly as before field terms existed.
