## ADDED Requirements

### Requirement: Print Output Formats
`fasttail --print` SHALL accept `--output text|json|logfmt|csv|template` (default `text`) and `--template` for `template`, writing the same lines that `text` would print, each as a record: JSON objects with `file`, `line`, `level`, `time`, `text` and the parsed `fields`, one per line.

#### Scenario: JSON for jq
- **WHEN** the user runs `fasttail --print --output json --filter ERROR app.log | jq .fields.status`
- **THEN** each ERROR line is one JSON object and `jq` prints its `status` field.

#### Scenario: Template
- **WHEN** the user runs `fasttail --print --output template --template '{time} {msg}' app.json`
- **THEN** each line prints its time and message field, separated by a space.
