# Design

## Context

See [proposal.md](proposal.md) for motivation. Previously on `v1`, `edf-http-diagnostics` built `/api/status` JSON using manual `core::fmt::Write` calls (`write_status_json` / `write_json_string`) and a `DiagnosticPayload` trait, while `intercom-adapter-http` implemented `IntercomPayload` in `router.rs` and served `/api/status` from `picoserve` in `server.rs`.

Under Hexagonal Architecture, `edf-core` and `intercom-core` must remain pure `no_std` domain crates with zero `serde` or HTTP transport coupling. All `serde::Serialize` DTOs and domain-to-DTO mappings belong in the HTTP adapter crates (`edf-http-diagnostics`, `intercom-adapter-http`).

## Goals / Non-Goals

**Goals:**
- Replace all manual JSON string concatenation (`IntercomPayload`, `DiagnosticPayload`) with `#[derive(serde::Serialize)]` DTOs serialized via `serde_json_core::to_slice` into a caller-provided `&mut [u8]`.
- Keep serialization 100% `no_std` and zero-heap-allocation (`serde-json-core` only; no `serde_json` in firmware).
- Supply required `MemorySnapshot` (`internal: { free_bytes, total_bytes }`, `psram: None`) and `CpuSnapshot::from_cores(&[])` (`{}`) from `server.rs` / `router.rs` without altering `intercom-firmware` or `HttpServer` constructors.
- Reorganize `/api/status` into five symmetrical top-level buckets (`system`, `network`, `firmware`, `ecosystems`, `device`), renaming `system.state` to `system.status` so `"state"` is reserved for object payloads (`device.state`).
- Bump `JSON_BUFFER_SIZE` from `2560` to `4096` bytes so maximal 5-fabric / 4-endpoint payloads fit safely.
- Update the Solid.js dashboard (`frontend/src/*`) and `scripts/ota.py` to consume the new schema.

**Non-Goals:**
- Adding `serde` derives to `edf-core` or `intercom-core` domain structs.
- Migrating `v1`'s `picoserve` handlers in `server.rs` to `edf_http_diagnostics::handle_diagnostic_route`.
- Maintaining backward compatibility with the old flat `/api/status` JSON layout.

## Decisions

1. **Five symmetrical top-level buckets (`system`, `network`, `firmware`, `ecosystems`, `device`)**
   - `system`: `{ status, uptime_sec, memory, cpu }` (renaming `SystemDiagnostics.state` to `status` in JSON, and removing redundant `free_heap_bytes`/`total_heap_bytes` already in `system.memory.internal`).
   - `network`: `{ ssid, rssi_dbm, ip, mac }`.
   - `firmware`: `{ version, git_commit, build_date, active_slot, slot_valid }`.
   - `ecosystems`: `{ available, active }`.
   - `device`: `{ type, state }` via `DeviceDto<'a, D>` (`D: Serialize`), avoiding `#[serde(flatten)]` (unsupported in `no_std` `serde-json-core`).
   - *Why:* Eliminates scattered root scalars, removes duplicate heap fields, and keeps `system.status` (string) distinct from `device.state` (object).

2. **Hexagonal boundary: 2-field `IntercomStatusDto` in `intercom-adapter-http::router`**
   - `edf-http-diagnostics` defines `StatusDto<'a, D, T>`, `SystemDto`, `NetworkDto<'a>`, `FirmwareDto<'a>`, `EcosystemsDto<'a, T>`, `DeviceDto<'a, D>`, `MemorySnapshot`, `MemoryRegion`, `CpuSnapshot`, and `CpuCoreLoad`, constructed from `edf_core::domain::diagnostics::SystemDiagnostics`.
   - `intercom-adapter-http` defines `IntercomStatusDto { door_locked: bool, door_unlock_duration_sec: u32 }` (`From<&IntercomState>`) in `router.rs` and passes it as `D` to `edf_http_diagnostics::format_status_json`.
   - *Why:* Keeps `edf-core` and `intercom-core` completely decoupled from serialization formats and `serde` feature flags.

3. **Required `memory` and `cpu` telemetry via `server.rs` & `router.rs`**
   - `router::format_status_json_with_telemetry` delegates to `edf_http_diagnostics::format_status_json` with explicit `MemorySnapshot` and `CpuSnapshot`.
   - `router::format_status_json` provides a convenience wrapper constructing `MemorySnapshot` (`internal` from `diag.free_heap_bytes` / `diag.total_heap_bytes`, `psram: None`) and `CpuSnapshot::from_cores(&[])` (serializing as `{}`).
   - `server.rs` constructs `MemorySnapshot` (`psram: None`) and `CpuSnapshot::from_cores(&[])` and invokes `crate::router::format_status_json_with_telemetry(&diag, memory, cpu, &mut json_buf)` inside `/api/status` without modifying `intercom-firmware` callers.

4. **Buffer sizing (`JSON_BUFFER_SIZE = 4096`)**
   - Maximal `EcosystemStatus` (6 properties, 5 fabrics with 4 subscriptions each, 4 endpoints with 6 clusters each) serializes to ~3.9 KiB under the five-bucket schema, exceeding the old `2560`-byte buffer. Increasing `JSON_BUFFER_SIZE` to `4096` bytes prevents truncation/fallback to `"{}"`.

## Risks / Trade-offs

- **[Flash / stack footprint of `serde` + `serde-json-core` + 4 KiB buffer]** → Both crates are already in `intercom-adapter-http/Cargo.toml`; verify both 8 MB `--release` and 4 MB `--release --features flash-4mb` firmware builds succeed cleanly.
- **[`scripts/ota.py` strictness]** → Reading `device.type`, `network.mac`, `firmware.version`, and `firmware.active_slot` assumes target devices also run the five-bucket `/api/status` schema.

## JSON & DTO Model Schema

### Shared Envelope (`edf-http-diagnostics` + `v1` `intercomDevice`)

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "type": "object",
  "required": ["system", "network", "firmware", "ecosystems", "device"],
  "properties": {
    "system": {
      "type": "object",
      "required": ["status", "uptime_sec", "memory", "cpu"],
      "properties": {
        "status": {
          "type": "string",
          "enum": [
            "Booting", "Starting", "Commissioning", "WifiConnecting",
            "EcosystemConnecting", "Operational", "Updating",
            "EcosystemReset", "FactoryReset", "Error"
          ]
        },
        "uptime_sec": { "type": "integer", "minimum": 0 },
        "memory": {
          "type": "object",
          "required": ["internal", "psram"],
          "properties": {
            "internal": { "$ref": "#/$defs/memoryRegion" },
            "psram": { "anyOf": [{ "$ref": "#/$defs/memoryRegion" }, { "type": "null" }] }
          }
        },
        "cpu": {
          "type": "object",
          "additionalProperties": {
            "type": "object",
            "required": ["load_pct"],
            "properties": { "load_pct": { "type": "integer", "minimum": 0, "maximum": 100 } }
          }
        }
      }
    },
    "network": {
      "type": "object",
      "required": ["ssid", "rssi_dbm", "ip", "mac"],
      "properties": {
        "ssid": { "type": ["string", "null"] },
        "rssi_dbm": { "type": ["integer", "null"] },
        "ip": { "type": ["string", "null"], "format": "ipv4" },
        "mac": { "type": "string", "pattern": "^[0-9A-F]{2}(:[0-9A-F]{2}){5}$" }
      }
    },
    "firmware": {
      "type": "object",
      "required": ["version", "git_commit", "build_date", "active_slot", "slot_valid"],
      "properties": {
        "version": { "type": "string" },
        "git_commit": { "type": "string" },
        "build_date": { "type": "string" },
        "active_slot": { "type": "string" },
        "slot_valid": { "type": "boolean" }
      }
    },
    "ecosystems": {
      "type": "object",
      "required": ["available", "active"],
      "properties": {
        "available": { "type": "array", "items": { "type": "string" } },
        "active": {
          "type": ["object", "null"],
          "required": ["type", "paired", "properties", "fabrics", "endpoints"]
        }
      }
    },
    "device": {
      "type": "object",
      "required": ["type", "state"],
      "properties": {
        "type": { "type": "string" },
        "state": { "$ref": "#/$defs/intercomDevice" }
      }
    }
  },
  "$defs": {
    "memoryRegion": {
      "type": "object",
      "required": ["free_bytes", "total_bytes"],
      "properties": {
        "free_bytes": { "type": "integer", "minimum": 0 },
        "total_bytes": { "type": "integer", "minimum": 0 }
      }
    },
    "intercomDevice": {
      "type": "object",
      "required": ["door_locked", "door_unlock_duration_sec"],
      "properties": {
        "door_locked": { "type": "boolean" },
        "door_unlock_duration_sec": { "type": "integer", "minimum": 0 }
      }
    }
  }
}
```

### Rust DTO Model (`intercom-adapter-http/src/router.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct IntercomStatusDto {
    pub door_locked: bool,
    pub door_unlock_duration_sec: u32,
}
```

### Sample JSON Payload (`intercom-matter` `v1`)

```json
{
  "system": {
    "status": "Operational",
    "uptime_sec": 3642,
    "memory": {
      "internal": { "free_bytes": 151552, "total_bytes": 215040 },
      "psram": null
    },
    "cpu": {}
  },
  "network": {
    "ssid": "Home_IoT_5G",
    "rssi_dbm": -58,
    "ip": "192.168.1.142",
    "mac": "98:A3:16:A6:78:44"
  },
  "firmware": {
    "version": "1.0.0",
    "git_commit": "a1b2c3d",
    "build_date": "2026-11-26",
    "active_slot": "ota_0",
    "slot_valid": true
  },
  "ecosystems": {
    "available": ["Matter"],
    "active": {
      "type": "Matter",
      "paired": true,
      "properties": { "door_lock_subscribed": true },
      "fabrics": [
        {
          "fab_idx": 1,
          "vendor_id": 4996,
          "fabric_id": "0x1B8246E1309A4D12",
          "node_id": "0x0000000000000002",
          "label": "Apple Home",
          "subscriptions": [{ "endpoint": 1, "cluster": "0x0101" }]
        }
      ],
      "endpoints": [
        {
          "id": 1,
          "name": "Door Lock",
          "device_type": "0x000A",
          "clusters": [{ "id": "0x0101", "name": "DoorLock" }]
        },
        {
          "id": 2,
          "name": "Chime / Ring",
          "device_type": "0x0107",
          "clusters": [{ "id": "0x0406", "name": "OccupancySensing" }]
        }
      ]
    }
  },
  "device": {
    "type": "intercom_bridge",
    "state": {
      "door_locked": true,
      "door_unlock_duration_sec": 5
    }
  }
}
```
