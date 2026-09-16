# Telemetry and Audio Effects Specification

## Requirements

### Requirement: Real-time System Telemetry in Titlebar
The application SHALL display live CPU utilization percentage and resident memory usage (MB) directly in the top title bar header, updating once per second without requiring a separate tab.

### Requirement: Per-Stream Throughput Monitoring
Each log stream SHALL measure byte ingress rate in real-time and display throughput (in KB/s or MB/s) on the stream status bar when active writes are detected.

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
