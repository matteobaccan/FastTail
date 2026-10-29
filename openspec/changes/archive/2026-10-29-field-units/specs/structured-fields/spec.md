## ADDED Requirements

### Requirement: Values with Units
A numeric field value with a duration, size or rate suffix SHALL be read in its family's base unit (milliseconds, bytes, per second), the family of a field being detected from its values or set by its log format (`unit.<field>`). Field statistics, sparklines, the query language and field filter terms SHALL compare and aggregate in the base unit and show results in the most readable unit.

#### Scenario: Durations compared
- **WHEN** a field `latency` holds `900ms`, `1.2s` and `45ms`
- **THEN** its maximum is shown as `1.2 s`, and the filter term `latency>=1s` keeps only the line with `1.2s`.

#### Scenario: Mixed families
- **WHEN** a query compares the size field `bytes` with the literal `2s`
- **THEN** the query is refused with a message naming the two units.
