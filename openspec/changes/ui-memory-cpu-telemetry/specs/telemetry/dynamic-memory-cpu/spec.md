## ADDED Requirements

### Requirement: Dynamic Memory Pool Telemetry Reporting
The `/api/status` endpoint SHALL support reporting memory statistics as a structured `memory` object keyed by pool name (such as `internal` and `psram`), where each non-null pool entry provides `free_bytes` and `total_bytes`.

#### Scenario: Target with internal RAM and PSRAM
- **WHEN** a client requests `GET /api/status` from a target with external PSRAM enabled
- **THEN** the JSON payload SHALL include `memory.internal` and `memory.psram` objects each containing `free_bytes` and `total_bytes`

#### Scenario: Target without PSRAM
- **WHEN** a client requests `GET /api/status` from a target without PSRAM
- **THEN** `memory.internal` SHALL be populated and `memory.psram` SHALL be `null` or omitted

### Requirement: Dynamic Memory Pool Visualization
The dashboard Diagnostics tab SHALL render one labeled memory usage bar for each non-null pool in `status.memory`, and SHALL omit memory bars if `status.memory` is absent or has no non-null pools.

#### Scenario: Multiple memory pools reported
- **WHEN** `status.memory` contains non-null `internal` and `psram` entries
- **THEN** the Diagnostics tab SHALL render two memory rows with used/total KB and percentage progress bars for `Internal RAM` and `PSRAM`

#### Scenario: Only internal memory pool reported
- **WHEN** `status.memory` has `internal` populated and `psram` set to `null`
- **THEN** the Diagnostics tab SHALL render only the `Internal RAM` bar

#### Scenario: Memory object absent
- **WHEN** `status.memory` is undefined or has no non-null pools
- **THEN** the Diagnostics tab SHALL render without error and omit memory bars

### Requirement: Per-Core CPU Load Telemetry Reporting
The `/api/status` endpoint SHALL include a `cpu` object with a `cores` array of `{ id, load_pct }` entries representing each active CPU core's recent utilization percentage (`0` to `100`).

#### Scenario: Single-core target
- **WHEN** a client requests `GET /api/status` from a single-core target such as ESP32-C6
- **THEN** `status.cpu.cores` SHALL contain one entry for core `0` with `load_pct` clamped between `0` and `100`

#### Scenario: Dual-core target
- **WHEN** a client requests `GET /api/status` from a dual-core target such as ESP32-S3 or ESP32-S31
- **THEN** `status.cpu.cores` SHALL contain two entries (`id: 0` and `id: 1`) with independent `load_pct` values

### Requirement: Dynamic CPU Core Load Visualization
The dashboard Diagnostics tab SHALL render one labeled CPU load bar per entry in `status.cpu.cores` when `status.cpu` is present, and SHALL omit the CPU load section when `status.cpu` is absent.

#### Scenario: CPU core load present
- **WHEN** `status.cpu.cores` contains one or more core entries
- **THEN** the Diagnostics tab SHALL display each core's identifier (e.g., `Core 0`, `Core 1`) and a percentage progress bar reflecting `load_pct`

#### Scenario: CPU core load absent
- **WHEN** `status.cpu` is undefined or `cores` is empty
- **THEN** the Diagnostics tab SHALL render without error and omit CPU core bars
