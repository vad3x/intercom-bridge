## ADDED Requirements

### Requirement: Typed Status JSON Response Schema
The `/api/status` endpoint SHALL serialize system diagnostics as valid JSON organized into five top-level objects: `system`, `network`, `firmware`, `ecosystems`, and `device`.

#### Scenario: Intercom status includes bucketed telemetry and nested device state
- **WHEN** a client requests `GET /api/status` while the intercom device is operational
- **THEN** the response is HTTP 200 with `Content-Type: application/json`
- **AND** the JSON root contains `system` (`status`, `uptime_sec`, `memory`, `cpu`), `network` (`ssid`, `rssi_dbm`, `ip`, `mac`), `firmware` (`version`, `git_commit`, `build_date`, `active_slot`, `slot_valid`), `ecosystems` (`available`, `active`), and `device` (`type`, `state`) with zero legacy flat root keys
- **AND** `device.type` is `"intercom_bridge"` and `device.state` contains only `door_locked` and `door_unlock_duration_sec`

#### Scenario: Control characters and quotes in strings are escaped
- **WHEN** any string field (such as Wi-Fi SSID, fabric label, or endpoint name) contains quotes, backslashes, control characters, or UTF-8 characters (e.g. `"雪"`)
- **THEN** the serialized output is valid JSON that decodes back to the exact original UTF-8 strings

### Requirement: Required Memory and CPU Telemetry
Every `/api/status` response SHALL include both `system.memory` (with `internal` and nullable `psram` regions) and `system.cpu` (with per-core load entries, or `{}` when empty).

#### Scenario: Memory and CPU telemetry serialize without optional omissions
- **WHEN** `/api/status` is serialized on a single-core, dual-core, or empty-CPU target with or without PSRAM
- **THEN** `system.memory.internal` includes `free_bytes` and `total_bytes`, `system.memory.psram` is either an object or `null`, and `system.cpu` is always present (`{}` when no cores are supplied, or `core_<id>: { load_pct }` when cores are supplied)

### Requirement: Fixed-Buffer Overflow Protection
Status serialization SHALL fail cleanly without truncating JSON or panicking when the caller buffer is smaller than the serialized payload, and `JSON_BUFFER_SIZE` (`4096` bytes) SHALL fit maximal ecosystem telemetry.

#### Scenario: Small buffer returns an error
- **WHEN** status serialization is invoked with a 64-byte buffer smaller than the required JSON length
- **THEN** serialization returns `Err(())` without writing partial or invalid JSON as a success result

#### Scenario: Maximal capacity ecosystem telemetry fits within JSON_BUFFER_SIZE
- **WHEN** `SystemDiagnostics` carries 6 properties, 5 fabrics with 4 subscriptions each, and 4 endpoints with 6 clusters each
- **THEN** serialization succeeds within `JSON_BUFFER_SIZE` (`4096` bytes)

### Requirement: Dashboard Consumes Bucketed Status Schema
The embedded web dashboard and `scripts/ota.py` SHALL read status fields from `system`, `network`, `firmware`, `ecosystems`, and `device`.

#### Scenario: Dashboard renders intercom state from nested device object
- **WHEN** the dashboard polls `/api/status`
- **THEN** system health reads `system.status`, `system.uptime_sec`, `system.memory`, and `system.cpu`, and door lock & unlock duration controls reflect `device.state.door_locked` and `device.state.door_unlock_duration_sec`
- **AND** `scripts/ota.py` reads `device.type`, `network.mac`, `firmware.version`, and `firmware.active_slot`
