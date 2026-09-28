## Context

`network-listener` provides listener streams: bind policy, an acceptor with at most 64
connection threads, one writer thread per listener with a token-bucket rate limit, and a
spool tailed like stdin. OTLP/HTTP is a POST of an `ExportLogsServiceRequest` (resource →
scope → log records) as protobuf or JSON, optionally gzip-compressed.

## Goals / Non-Goals

**Goals:** see OTLP/HTTP logs of a local service live, with no collector; reuse every
bound of `network-listener`. **Non-goals:** gRPC, traces, metrics, TLS, auth.

## Decisions

### D1. A phase of `network-listener`, HTTP only

OTLP is a listener kind, not a new source type: the connection threads parse HTTP
requests instead of lines and hand rendered lines to the same writer. Rejected: gRPC with
`tonic` — it brings `tokio`, `hyper`, `h2` (several MB and an async runtime used nowhere
else) for a feature of low priority; every OTel SDK and the collector can export
OTLP/HTTP (`OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf`). Rejected: `tiny_http` — its own
thread pool would bypass the listener's connection cap and rate limit; the needed HTTP
subset (one path, POST, Content-Length or chunked, keep-alive) is about 200 lines.

### D2. Decoding and rendering

`prost` derives on hand-written structs for the handful of messages used (fields by
number from `opentelemetry/proto/logs/v1/logs.proto`, unknown fields skipped), so no
`protoc` or build script is needed; JSON through `serde_json` with the OTLP JSON mapping
(hex trace ids, string int64). Bodies over 16 MB, compressed or not, are refused with
`413`; decompression stops at 16 MB. One line per record:
`<time_unix_nano as ISO 8601 UTC ms> <LEVEL> <service.name> [<scope.name>] <body>
{k=v …} trace=<trace_id> span=<span_id>`, `observed_time` used when `time` is 0, a body
that is a map or array written as compact JSON, attributes limited to 32 per line. Lines
over 64 KB are cut with the long-line marker.

### D3. Threads, memory

No new thread: HTTP runs on the listener's connection threads (≤ 64). Memory per request
≤ 16 MB body + its decoded records, freed after rendering; the content lives in the
spool. The UI thread only reads counters.

## Risks / Trade-offs

- [Hand-written protobuf structs drift from the spec] → fixtures produced by the official
  OTel Python / Go SDK exporters checked in as bytes.

## Open Questions

1. Is OTLP/gRPC worth an async runtime if users ask for it, or should the README point to
   a collector converting gRPC to HTTP?
