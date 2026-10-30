## Why

Kubernetes logs reach FastTail through `kubectl logs -f | fasttail -` today; gonzo and
lazyjournal read pods directly. A `docker://`-like `k8s://` source (FastTail's system
sources) would follow a pod, reconnect on restarts and name the stream after it.

## What Changes

- `k8s://<namespace>/<pod>[/<container>]` (and `k8s://<context>@<namespace>/<pod>`) opens a
  pod's log through `kubectl logs -f --timestamps`, as `docker://` runs `docker logs`;
  `k8s://<namespace>/deploy/<name>` follows a deployment.
- The command is configurable (`[system] kubectl_command`), the stream ends and offers
  *Reconnect* when the pod goes, the open dialogs list it. Both interfaces: the window and the terminal (`fasttail-tui`) get it in the same change.

Target release: **0.32.0** (plan of 2026-10-30, after the competitor scan against 0.30.0; see `docs/competitor-analysis.md` section 8). Priority: **Medium**. Effort: **S**.

### Non-goals

- Talking to the Kubernetes API without kubectl.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `stream-engine`: the requirement below is added.

## Impact

`src/system_source.rs`, config key, open dialogs; texts; docs.
