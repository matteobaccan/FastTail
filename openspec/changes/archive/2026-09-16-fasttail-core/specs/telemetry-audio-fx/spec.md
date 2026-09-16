## ADDED Requirements

### Requirement: Real-time telemetry monitor
The application SHALL provide a telemetry HUD panel displaying current stream metrics including process CPU usage, resident RAM, stream throughput in KB/sec, and total indexed lines.

#### Scenario: Stream throughput measurement
- **WHEN** lines are actively appended to a monitored file
- **THEN** the telemetry panel updates the throughput gauge every 500 milliseconds reflecting incoming data rates.

#### Scenario: Disabling telemetry monitor
- **WHEN** the user toggles telemetry off via settings or the panel close button
- **THEN** background telemetry polling stops and the panel is removed from the active layout.

### Requirement: Optional cyber-terminal audio sound effects
The application SHALL include retro-futuristic cyber-terminal sound effects for interaction events (such as error detection, file attachment, and keystrokes), configured as DISABLED by default.

#### Scenario: Default audio state on clean install
- **WHEN** FastTail boots for the first time
- **THEN** audio output is completely muted and disabled.

#### Scenario: User enables audio effects
- **WHEN** the user toggles audio effects to enabled in settings and an `ERROR` level line appears in the stream
- **THEN** the application plays a short audio chirp without blocking the UI thread.
