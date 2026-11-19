## ADDED Requirements

### Requirement: WebSocket Status Stream Endpoint
When enabled by target concurrency configuration, the HTTP server SHALL expose `GET /api/ws/status` as a WebSocket endpoint that pushes full status JSON snapshots immediately upon connection, on state transitions, and on a periodic telemetry interval.

#### Scenario: Initial snapshot and periodic push
- **WHEN** a dashboard client connects to `GET /api/ws/status` and upgrades to WebSocket
- **THEN** the server SHALL send the current status JSON text frame immediately and continue sending updated snapshots every 1–2 seconds

#### Scenario: State transition push
- **WHEN** door lock state, bus line state, call state, or system state changes while a `/api/ws/status` session is open
- **THEN** the server SHALL push an updated status JSON frame without waiting for the next periodic tick

### Requirement: Target Socket and SRAM Budget Gating
The `/api/ws/status` endpoint SHALL only be enabled on targets whose `HttpServer` runs at least 3 concurrent accept workers and whose `embassy-net` TCP socket pool and SRAM budget can sustain simultaneous `/api/ws/status`, `/api/ws/audio`, and REST/OTA requests without starvation.

#### Scenario: Constrained single-core target (ESP32-C6)
- **WHEN** the target's SRAM or TCP socket budget cannot accommodate a 3rd accept worker
- **THEN** `/api/ws/status` SHALL be gated off (returning HTTP 404/503 or omitting the route) so `/api/ws/audio` and REST/OTA remain responsive on the 2 existing workers

#### Scenario: Multi-core / PSRAM target (ESP32-S3 / ESP32-S31)
- **WHEN** the target provides sufficient TCP sockets and SRAM for 3+ accept workers
- **THEN** `/api/ws/status`, `/api/ws/audio`, and REST/OTA requests SHALL operate concurrently without blocking each other

### Requirement: Frontend WebSocket Client with HTTP Polling Fallback
The dashboard store SHALL attempt to subscribe to `/api/ws/status` for real-time status updates and SHALL transparently fall back to 2.5s `GET /api/status` HTTP polling whenever WebSocket connection fails, is rejected, or disconnects.

#### Scenario: WebSocket available
- **WHEN** the dashboard mounts and `/api/ws/status` connects successfully
- **THEN** the store SHALL update `status` from incoming WebSocket JSON frames and suspend periodic `GET /api/status` polling

#### Scenario: WebSocket unavailable or disconnected
- **WHEN** `/api/ws/status` fails to upgrade (e.g., on a gated target) or closes unexpectedly
- **THEN** the store SHALL fall back to 2.5s `GET /api/status` polling and optionally retry WebSocket with backoff without surfacing errors to the user
