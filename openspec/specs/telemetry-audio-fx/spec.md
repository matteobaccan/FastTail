# Telemetry and Audio Effects Specification

## Purpose
Surfaces live system telemetry and per-stream throughput in the UI and defines the built-in sound alert presets used by highlight rules and search.

## Requirements

### Requirement: Real-time System Telemetry in Titlebar
The application SHALL display live CPU utilization percentage and resident memory usage (MB) directly in the top title bar header, updating once per second without requiring a separate tab.

#### Scenario: Telemetry refresh while streaming
- **WHEN** the application is running with several streams following files
- **THEN** the title bar CPU percentage and memory figure refresh about once per second without disturbing the stream rendering.

### Requirement: Per-Stream Throughput Monitoring
Each log stream SHALL measure byte ingress rate in real-time and display throughput (in KB/s or MB/s) on the stream status bar when active writes are detected.

#### Scenario: Burst of writes on one stream
- **WHEN** an external process writes a few megabytes per second to the file of stream A while stream B is idle
- **THEN** the status bar of stream A shows the ingress rate in MB/s and stream B shows no throughput figure.

### Requirement: Sound Alert Presets
The application SHALL provide built-in sound alert presets:
- None: Silent.
- Beep: Standard system tone (0x0).
- Chime: Asterisk / information chime (0x40).
- Warning: Exclamation alert tone (0x30).
- Critical: Hand / critical stop alert tone (0x10).

#### Scenario: Rule audio trigger on streaming lines
- **WHEN** a background process writes lines matching a rule configured with a sound alert preset
- **THEN** FastTail plays the selected sound alert if Sound FX is enabled in settings, throttled to prevent audio flooding.
