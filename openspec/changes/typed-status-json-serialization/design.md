# Design

## Context

See [proposal.md](proposal.md) for motivation. Today, `edf-http-diagnostics` builds `/api/status` JSON using manual `core::fmt::Write` calls (`write_status_json` / `write_json_string`) and a `DiagnosticPayload` trait, while `intercom-adapter-http` (`format_status_json_with_telemetry`) strips the trailing `}` from `edf-http-diagnostics` output and uses a 256-byte `ByteWriter` to splice in `memory` and `cpu` JSON.

Under Hexagonal Architecture, `edf-core` and `intercom-core` must remain pure `no_std` domain crates with zero `serde` or HTTP transport coupling. All `serde::Serialize` DTOs and domain-to-DTO mappings belong in the HTTP adapter crates (`edf-http-diagnostics`, `intercom-adapter-http`).

## Goals / Non-Goals

**Goals:**
- Replace all manual JSON string concatenation and `}`-splicing with `#[derive(serde::Serialize)]` DTOs serialized via `serde_json_core::to_slice` into a caller-provided `&mut [u8]`.
- Keep serialization 100% `no_std` and zero-heap-allocation (`serde-json-core` only; no `serde_json` in firmware).
- Move `MemorySnapshot`, `MemoryRegion`, `CpuSnapshot`, and `CpuCoreLoad` into `edf-http-diagnostics` and make both `memory` and `cpu` required fields under `system`.
- Reorganize `/api/status` into five symmetrical top-level buckets (`system`, `network`, `firmware`, `ecosystems`, `device`), renaming `system.state` to `system.status` so `"state"` is reserved for object payloads (`device.state`).
- Update `dashboard-core`, the Solid.js dashboard, and Python scripts to consume the new schema.

**Non-Goals:**
- Adding `serde` derives to `edf-core` or `intercom-core` domain structs.
- Streaming JSON directly to sockets without `JSON_BUFFER_SIZE`.
- Maintaining backward compatibility with the old flat `/api/status` JSON layout.

## Decisions

1. **Five symmetrical top-level buckets (`system`, `network`, `firmware`, `ecosystems`, `device`)**
   - `system`: `{ status, uptime_sec, memory, cpu }` (renaming `SystemDiagnostics.state` to `status` in JSON, and removing redundant `free_heap_bytes`/`total_heap_bytes` already in `system.memory.internal`).
   - `network`: `{ ssid, rssi_dbm, ip, mac }`.
   - `firmware`: `{ version, git_commit, build_date, active_slot, slot_valid }`.
   - `ecosystems`: `{ available, active }`.
   - `device`: `{ type, state }` via `DeviceDto<'a, D>` (`D: Serialize`), avoiding `#[serde(flatten)]` (unsupported in `no_std` `serde-json-core`).
   - *Why:* Eliminates scattered root scalars, removes duplicate heap fields, and keeps `system.status` (string) distinct from `device.state` (object).

2. **Hexagonal boundary: DTOs in HTTP adapters, domain types untouched**
   - `edf-http-diagnostics` defines `StatusDto<'a, D, T>`, `SystemDto`, `NetworkDto<'a>`, `FirmwareDto<'a>`, `EcosystemsDto<'a, T>`, `DeviceDto<'a, D>`, `MemorySnapshot`, `MemoryRegion`, `CpuSnapshot`, and `CpuCoreLoad`, constructed from `edf_core::domain::diagnostics::SystemDiagnostics`.
   - `intercom-adapter-http` defines `IntercomStatusDto` (`From<&IntercomState>`) in `router.rs` and passes it as `D` to `edf_http_diagnostics::format_status_json`.
   - *Why:* Keeps `edf-core` and `intercom-core` completely decoupled from serialization formats and `serde` feature flags.

3. **Required `memory` and `cpu` telemetry in `edf-http-diagnostics`**
   - `format_status_json<D: Serialize, T: EcosystemType>(diag: &SystemDiagnostics<D, T>, memory: MemorySnapshot, cpu: CpuSnapshot, out: &mut [u8]) -> Result<usize, DiagnosticError>` replaces the separate `format_status_json_with_memory` / `format_status_json_with_telemetry` splicing wrappers.
   - `CpuSnapshot` serializes `core_<id>: { "load_pct": <u8> }` via `serialize_map` with a stack-allocated `heapless::String<12>`.
   - *Why:* Removes `intercom-adapter-http`'s trailing-`}` splicing hack and gives `hvac-bridge` identical memory and CPU telemetry.

## Risks / Trade-offs

- **[Flash / stack footprint of `serde` + `serde-json-core`]** → Both crates are already in `intercom-adapter-http/Cargo.toml`; verify `intercom-firmware --release` builds cleanly and `JSON_BUFFER_SIZE` (2560 bytes) still fits maximum fabric/endpoint/CPU payloads.
- **[Cross-repo coordination with `embedded-device-foundation`]** → `edf-http-diagnostics` and `dashboard-core` changes must land before or alongside `intercom-matter` and `hvac-bridge` updates.

## JSON & DTO Model Schema

### Shared Envelope (`edf-http-diagnostics`)

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
          "required": ["type", "paired", "properties", "fabrics", "endpoints"],
          "properties": {
            "type": { "type": "string" },
            "paired": { "type": "boolean" },
            "properties": { "type": "object", "additionalProperties": { "type": ["boolean", "integer", "string"] } },
            "fabrics": {
              "type": "array",
              "items": {
                "type": "object",
                "required": ["fab_idx", "vendor_id", "fabric_id", "node_id", "label", "subscriptions"],
                "properties": {
                  "fab_idx": { "type": "integer" },
                  "vendor_id": { "type": "integer" },
                  "fabric_id": { "type": "string" },
                  "node_id": { "type": "string" },
                  "label": { "type": "string" },
                  "subscriptions": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "required": ["endpoint", "cluster"],
                      "properties": {
                        "endpoint": { "type": "integer" },
                        "cluster": { "type": "string" }
                      }
                    }
                  }
                }
              }
            },
            "endpoints": {
              "type": "array",
              "items": {
                "type": "object",
                "required": ["id", "name", "device_type", "clusters"],
                "properties": {
                  "id": { "type": "integer" },
                  "name": { "type": "string" },
                  "device_type": { "type": "string" },
                  "clusters": {
                    "type": "array",
                    "items": {
                      "type": "object",
                      "required": ["id", "name"],
                      "properties": {
                        "id": { "type": "string" },
                        "name": { "type": "string" }
                      }
                    }
                  }
                }
              }
            }
          }
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
      "required": [
        "door_locked", "door_unlock_duration_sec", "bus_state", "call_state",
        "rx_rms_dbfs", "rx_peak_dbfs", "rx_gain_half_db", "tx_gain_half_db", "tone_active"
      ],
      "properties": {
        "door_locked": { "type": "boolean" },
        "door_unlock_duration_sec": { "type": "integer", "minimum": 0 },
        "bus_state": { "type": "string", "enum": ["idle", "listen", "talk", "door_unlock"] },
        "call_state": { "type": "string", "enum": ["idle", "listening", "talking", "settling", "unlocking"] },
        "rx_rms_dbfs": { "type": "integer" },
        "rx_peak_dbfs": { "type": "integer" },
        "rx_gain_half_db": { "type": "integer" },
        "tx_gain_half_db": { "type": "integer" },
        "tone_active": { "type": "boolean" }
      }
    }
  }
}
```

### Rust DTO Models (`edf-http-diagnostics` & `intercom-adapter-http`)

```rust
// edf-http-diagnostics (no_std, zero alloc)
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MemoryRegion {
    pub free_bytes: usize,
    pub total_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct MemorySnapshot {
    pub internal: MemoryRegion,
    pub psram: Option<MemoryRegion>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuCoreLoad {
    pub id: u8,
    pub load_pct: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CpuSnapshot {
    core_count: u8,
    cores: [CpuCoreLoad; MAX_CPU_CORES],
}

#[derive(serde::Serialize)]
pub struct SystemDto {
    pub status: &'static str,
    pub uptime_sec: u64,
    pub memory: MemorySnapshot,
    pub cpu: CpuSnapshot,
}

#[derive(serde::Serialize)]
pub struct NetworkDto<'a> {
    pub ssid: Option<&'a str>,
    pub rssi_dbm: Option<i8>,
    pub ip: Option<heapless::String<15>>,
    pub mac: heapless::String<17>,
}

#[derive(serde::Serialize)]
pub struct FirmwareDto<'a> {
    pub version: &'a str,
    pub git_commit: &'a str,
    pub build_date: &'a str,
    pub active_slot: &'a str,
    pub slot_valid: bool,
}

#[derive(serde::Serialize)]
pub struct EcosystemsDto<'a, T: EcosystemType> {
    pub available: AvailableEcosystemsDto<'a, T>,
    pub active: Option<ActiveEcosystemDto<'a>>,
}

#[derive(serde::Serialize)]
pub struct DeviceDto<'a, D> {
    #[serde(rename = "type")]
    pub device_type: &'a str,
    pub state: D,
}

#[derive(serde::Serialize)]
pub struct StatusDto<'a, D, T: EcosystemType> {
    pub system: SystemDto,
    pub network: NetworkDto<'a>,
    pub firmware: FirmwareDto<'a>,
    pub ecosystems: EcosystemsDto<'a, T>,
    pub device: DeviceDto<'a, D>,
}

// intercom-adapter-http
#[derive(serde::Serialize)]
pub struct IntercomStatusDto<'a> {
    pub door_locked: bool,
    pub door_unlock_duration_sec: u32,
    pub bus_state: &'a str,
    pub call_state: &'a str,
    pub rx_rms_dbfs: i8,
    pub rx_peak_dbfs: i8,
    pub rx_gain_half_db: u8,
    pub tx_gain_half_db: i8,
    pub tone_active: bool,
}
```

### Sample JSON Payload (`intercom-matter`)

```json
{
  "system": {
    "status": "Operational",
    "uptime_sec": 3642,
    "memory": {
      "internal": { "free_bytes": 12480, "total_bytes": 122880 },
      "psram": null
    },
    "cpu": {
      "core_0": { "load_pct": 48 }
    }
  },
  "network": {
    "ssid": "Home_IoT_5G",
    "rssi_dbm": -58,
    "ip": "192.168.1.142",
    "mac": "98:A3:16:A6:78:44"
  },
  "firmware": {
    "version": "2.1.0",
    "git_commit": "a1b2c3d",
    "build_date": "2026-11-26",
    "active_slot": "ota_0",
    "slot_valid": true
  },
  "ecosystems": {
    "available": ["matter"],
    "active": {
      "type": "matter",
      "paired": true,
      "properties": { "door_subscribed": true },
      "fabrics": [
        {
          "fab_idx": 1,
          "vendor_id": 4996,
          "fabric_id": "0x1B8246E1309A4D12",
          "node_id": "0x0000000000000002",
          "label": "Apple Home",
          "subscriptions": [{ "endpoint": 2, "cluster": "0x0101" }]
        }
      ],
      "endpoints": [
        {
          "id": 1,
          "name": "Audio Doorbell",
          "device_type": "0x0141",
          "clusters": [{ "id": "0x003B", "name": "Switch" }]
        }
      ]
    }
  },
  "device": {
    "type": "intercom_bridge",
    "state": {
      "door_locked": true,
      "door_unlock_duration_sec": 5,
      "bus_state": "idle",
      "call_state": "idle",
      "rx_rms_dbfs": -60,
      "rx_peak_dbfs": -48,
      "rx_gain_half_db": 20,
      "tx_gain_half_db": 0,
      "tone_active": false
    }
  }
}
```
