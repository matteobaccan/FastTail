## 1. Receiver

- [ ] 1.1 Depends on `network-listener` being merged; add the `Otlp` listener kind and `otlp://` identity
- [ ] 1.2 Minimal HTTP/1.1 reader: POST `/v1/logs`, Content-Length and chunked, keep-alive, gzip; `200`, `404`, `413`, `415`, `429` + `Retry-After`
- [ ] 1.3 `prost` structs for `ExportLogsServiceRequest`; JSON mapping; unit tests with checked-in protobuf and JSON fixtures from an OTel SDK
- [ ] 1.4 Rendering and severity mapping (design D2); tests against the level and timestamp detectors
- [ ] 1.5 Integration test: POST to an ephemeral loopback port, lines appear; oversized body refused; rate limit answers 429

## 2. UI, texts, documentation

- [ ] 2.1 OTLP entry in the Listen dialog (default port 4318) and in `--listen`
- [ ] 2.2 New i18n keys in all 16 languages; exhaustive i18n test updated
- [ ] 2.3 README: OTLP section with `OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4318` and `OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf`; CHANGELOG `[Unreleased]`

## 3. Wrap-up

- [ ] 3.1 `cargo fmt`, `cargo clippy --all-targets`, `cargo test` green on Linux and Windows CI
- [ ] 3.2 After the release, archive the change so `otlp-receiver` becomes a spec
