## ADDED Requirements

### Requirement: Typed Status JSON Response Schema
The `/api/status` endpoint SHALL serialize system diagnostics as valid JSON organized into five top-level objects: `system`, `network`, `firmware`, `ecosystems`, and `device`.

#### Scenario: Intercom status includes bucketed telemetry and nested device state
- **WHEN** a client requests `GET /api/status` while the intercom device is operational
- **THEN** the response is HTTP 200 with `Content-Type: application/json`
- **AND** the JSON root contains `system` (`status`, `uptime_sec`, `memory`, `cpu`), `network` (`ssid`, `rssi_dbm`, `ip`, `mac`), `firmware` (`version`, `git_commit`, `build_date`, `active_slot`, `slot_valid`), `ecosystems` (`available`, `active`), and `device` (`type`, `state`)
- **AND** `device.type` is `"intercom_bridge"` and `device.state` contains `door_locked`, `door_unlock_duration_sec`, `bus_state`, `call_state`, `rx_rms_dbfs`, `rx_peak_dbfs`, `rx_gain_half_db`, `tx_gain_half_db`, and `tone_active`

#### Scenario: Control characters and quotes in strings are escaped
- **WHEN** any string field (such as Wi-Fi SSID, fabric label, or endpoint name) contains quotes, backslashes, or control characters
- **THEN** the serialized output is valid JSON that decodes back to the exact original UTF-8 strings

### Requirement: Required Memory and CPU Telemetry
Every `/api/status` response SHALL include both `system.memory` (with `internal` and nullable `psram` regions) and `system.cpu` (with per-core load entries).

#### Scenario: Memory and CPU telemetry serialize without optional omissions
- **WHEN** `/api/status` is serialized on a single-core or dual-core target with or without PSRAM
- **THEN** `system.memory.internal` includes `free_bytes` and `total_bytes`, `system.memory.psram` is either an object or `null`, and `system.cpu` is always present

### Requirement: Fixed-Buffer Overflow Protection
Status serialization SHALL fail cleanly without truncating JSON or panicking when the caller buffer is smaller than the serialized payload.

#### Scenario: Small buffer returns an error
- **WHEN** status serialization is invoked with a buffer smaller than the required JSON length
- **THEN** serialization returns an error without writing partial or invalid JSON as a success result

### Requirement: Dashboard Consumes Bucketed Status Schema
The embedded web dashboard and Python tooling SHALL read status fields from `system`, `network`, `firmware`, `ecosystems`, and `device`.

#### Scenario: Dashboard renders intercom state from nested device object
- **WHEN** the dashboard polls `/api/status`
- **THEN** system health reads `system.status`, `system.uptime_sec`, `system.memory`, and `system.cpu`, and door/bus/audio controls reflect `device.state.*`
