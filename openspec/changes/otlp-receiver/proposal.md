## Why

Services instrumented with OpenTelemetry export logs over OTLP, usually to a collector and
a backend. During local development there is often no backend: the developer wants to see
the service's logs live, with filters, without running a collector stack. gonzo and
otel-tui accept OTLP directly; FastTail cannot. Low priority (part of gap 20's "various"
in the post-0.12.0 competitor scan), but cheap once `network-listener` exists.

## What Changes

- A new listener kind in the Listen dialog, `otlp://127.0.0.1:4318`: an **OTLP/HTTP logs
  receiver** (`POST /v1/logs`, protobuf or JSON body, optional gzip) that turns each log
  record into one line
  `<time> <LEVEL> <service.name> [<scope>] <body> {attr=value …} trace=<id>`.
- Severity numbers map to FastTail's level words (1–4 `TRACE`, 5–8 `DEBUG`, 9–12
  `INFO`, 13–16 `WARN`, 17–20 `ERROR`, 21–24 `FATAL`; unset → the severity text, else
  `INFO`).
- Built as **phase 2 of `network-listener`**: same loopback-by-default binding and
  warning, 64-connection cap, rate limit, spool, stream bar state, persistence and
  `--listen` command line; it is written as a separate change only because it adds
  dependencies and a small HTTP layer.
- Answers `200` with an empty `ExportLogsServiceResponse`, `415` for other content
  types, `413` above 16 MB per request, `429` with `Retry-After: 1` when rate-limited.

Target release: **0.15.0** (sources and integrations), per the release plan in
`docs/competitor-analysis.md` section 8. Priority: **low**. Effort: **S (under a week on top of `network-listener`)**.

### Non-goals

- OTLP/gRPC (port 4317): needs HTTP/2 and an async runtime (`tonic` + `tokio`); open
  question 1.
- Traces and metrics; TLS; authentication headers.

## Capabilities

### New Capabilities

- `otlp-receiver`: OTLP/HTTP logs as a listener stream.

### Modified Capabilities

_None_ (it extends `network-listener`, which is not yet a spec; the requirements here
reference its behaviour).

## Impact

- New `src/otlp.rs`: minimal HTTP/1.1 request reader on the listener's TCP connections
  (Content-Length and chunked bodies, keep-alive), protobuf decoding with `prost` over
  hand-written message structs for `ExportLogsServiceRequest` (no `protoc`, no build
  script), JSON via `serde_json`, gzip via the existing `flate2`.
- `Cargo.toml`: `prost` (Apache-2.0, pure Rust, `default-features = false` + `std`,
  `derive`).
- `src/net_listener.rs`: a listener kind `Otlp`; i18n strings in all 16 languages; README
  and CHANGELOG.
