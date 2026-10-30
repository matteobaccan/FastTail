## ADDED Requirements

### Requirement: Kubernetes Source
FastTail SHALL open `k8s://` URIs as streams of a pod's or deployment's log through the configured kubectl command, and SHALL offer reconnecting when the command ends.

#### Scenario: A pod
- **WHEN** the user opens `k8s://prod/api-7f9c`
- **THEN** the stream shows the pod's log lines with their timestamps and follows them.
