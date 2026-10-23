## ADDED Requirements

### Requirement: OTLP/HTTP Logs Receiver
A listener stream of kind `otlp://addr:port` SHALL accept OTLP/HTTP log exports (`POST /v1/logs` with `application/x-protobuf` or `application/json` bodies, optionally gzip-encoded) and SHALL write each log record as one line `<time> <LEVEL> <service.name> [<scope name>] <body> {<attributes>} trace=<trace id> span=<span id>`, with the time in ISO 8601 UTC with milliseconds (the observed time when the record's time is 0) and at most 32 attributes per line. It SHALL answer `200` with an empty export response on success, `415` for another content type, `413` for a body over 16 MB before or after decompression, and `429` with `Retry-After: 1` when the listener's rate limit drops the request. It SHALL follow the binding safety, connection cap, rate limit, spool bounds, stream bar state and persistence rules of listener streams, and OTLP/gRPC SHALL NOT be accepted.

#### Scenario: Local service exporting logs
- **WHEN** a service runs with `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318` and `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf` while an `otlp://127.0.0.1:4318` stream is open, and it logs an error "payment failed" with attribute `order=42`
- **THEN** within 1 second the stream shows a line with its time, `ERROR`, the service name, `payment failed`, `{order=42}` and the trace id.

#### Scenario: Oversized export
- **WHEN** a client posts a 20 MB body
- **THEN** it receives `413` and no line is written.

### Requirement: OTLP Severity Mapping
The record's severity number SHALL map to level words as 1–4 → `TRACE`, 5–8 → `DEBUG`, 9–12 → `INFO`, 13–16 → `WARN`, 17–20 → `ERROR`, 21–24 → `FATAL`; when the number is unset the severity text SHALL be written as sent, and when both are unset the level SHALL be `INFO`.

#### Scenario: Severity number only
- **WHEN** a record has severity number 18 and no severity text
- **THEN** its line carries `ERROR` and is counted as ERROR by level detection.
