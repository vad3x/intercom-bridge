# Multi-Hub & Fabric Telemetry in Dashboard Design Specification

**Date:** 2026-10-17  
**Status:** Approved  
**Author:** Antigravity  

## 1. Overview & Goals

The Intercom Bridge supports Matter over Wi-Fi with multi-admin/multi-fabric commissioning. A single device can be paired concurrently with multiple smart home ecosystems (e.g. Apple Home, Home Assistant, Google Home, Amazon Alexa, SmartThings).

Currently, the embedded web dashboard (`/` and `/api/status`) reports only a binary `paired: true/false` status with Door Lock and Ring Sensor subscription states. It does not expose:
1. Which hubs or fabrics are connected to the device.
2. How many fabric slots are in use out of the maximum capacity.
3. Ecosystem vendor identities, fabric indices, user labels, and operational identifiers.

**Goal:** Surface connected smart home hubs and fabrics cleanly in the Matter Ecosystem card on the dashboard, displaying ecosystem names, roles, fabric indices, labels, and expandable technical details (VID, Node ID, Fabric ID), backed by an extensible, zero-allocation telemetry pipeline in the firmware.

---

## 2. Architecture & System Boundaries

Following Hexagonal Architecture (Ports & Adapters), the feature is split into domain models, outbound ports, protocol adapters, REST formatting, and frontend presentation:

```
[rs-matter Stack] (Fabrics Table in MatterState)
        │
        ▼ (attached via static atomic reference)
[intercom-adapter-matter] (ClusterReadinessBarrier implements EcosystemInfoDriver)
        │
        ▼ (reads status on demand under state lock)
[intercom-core] (DiagnosticsService aggregates EcosystemStatus with FabricInfo value objects)
        │
        ▼ (queried on GET /api/status)
[intercom-adapter-http] (format_status_json serializes fabrics array to JSON)
        │
        ▼ (HTTP REST response over Wi-Fi port 8080)
[Solid.js Dashboard] (EcosystemTab renders Option 1 clean hub list with FE VID mapping)
```

### Key Architectural Decisions:
- **No Periodic Polling**: The Matter stack already persists and updates the authoritative Fabric table in RAM (`state.fabrics`) and flash (`matter_kvs`). The HTTP status snapshot queries the table on demand when a client requests `/api/status`.
- **Zero Microcontroller ROM Wasted on String Dictionaries**: The firmware sends raw protocol identifiers (`vendor_id`, `fab_idx`, `fabric_id`, `node_id`, `label`). The frontend maps `vendor_id` to ecosystem names, roles, and icons.
- **Precision Preservation for 64-bit Integers**: `fabric_id` (64-bit) and `node_id` (64-bit) are serialized as hexadecimal strings (`"0x..."`) to avoid JavaScript floating-point truncation.

---

## 3. Detailed Component Specifications

### 3.1. Core Domain (`crates/intercom-core`)

#### `crates/intercom-core/src/domain/ecosystem.rs`
Introduce `FabricInfo` and update `EcosystemStatus`:

```rust
/// Maximum number of active fabrics reported in ecosystem telemetry snapshot.
pub const MAX_ECOSYSTEM_FABRICS: usize = 5;

/// Telemetry snapshot for an individual connected Matter fabric / smart home hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FabricInfo {
    /// 1-based fabric index within the device's fabric table (1..=254)
    pub fab_idx: u8,
    /// Vendor ID of the commissioner or ecosystem (e.g. 0x1349, 0x1384, 0x130D)
    pub vendor_id: u16,
    /// 64-bit Fabric ID assigned by the operational CA
    pub fabric_id: u64,
    /// 64-bit Node ID assigned to this accessory on the fabric
    pub node_id: u64,
    /// Fabric label configured by user or ecosystem (up to 32 bytes)
    pub label: heapless::String<32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcosystemStatus {
    pub ecosystem_type: EcosystemId,
    pub paired: bool,
    pub properties: [Option<(&'static str, EcosystemValue)>; MAX_ECOSYSTEM_PROPERTIES],
    pub fabrics: [Option<FabricInfo>; MAX_ECOSYSTEM_FABRICS],
}

impl EcosystemStatus {
    pub const fn new(ecosystem_type: EcosystemId, paired: bool) -> Self {
        Self {
            ecosystem_type,
            paired,
            properties: [None; MAX_ECOSYSTEM_PROPERTIES],
            fabrics: [None; MAX_ECOSYSTEM_FABRICS],
        }
    }

    pub fn with_fabric(mut self, fabric: FabricInfo) -> Self {
        for slot in self.fabrics.iter_mut() {
            if slot.is_none() {
                *slot = Some(fabric);
                break;
            }
        }
        self
    }

    pub fn iter_fabrics(&self) -> impl Iterator<Item = &FabricInfo> {
        self.fabrics.iter().filter_map(|f| f.as_ref())
    }
}
```

---

### 3.2. Matter Protocol Adapter (`crates/intercom-adapter-matter`)

#### `crates/intercom-adapter-matter/src/readiness.rs`
- Add an atomic static/reference slot for `&'static rs_matter::Matter<'static>`.
- Provide `attach_matter(&self, matter: &'static rs_matter::Matter<'static>)`.
- In `EcosystemInfoDriver::status(&self)`, if `matter` is attached, acquire `matter.with_state(|state| ...)` and iterate over `state.fabrics`:
  ```rust
  for fab in state.fabrics.iter() {
      let mut label = heapless::String::new();
      let _ = label.push_str(fab.label());
      status = status.with_fabric(FabricInfo {
          fab_idx: fab.fab_idx().get(),
          vendor_id: fab.vendor_id(),
          fabric_id: fab.fabric_id(),
          node_id: fab.node_id(),
          label,
      });
  }
  ```

#### `crates/intercom-adapter-matter/src/controller.rs`
When `stack` is initialized in `MatterController::run()`:
```rust
self.readiness.attach_matter(stack.matter());
```

---

### 3.3. HTTP REST API (`crates/intercom-adapter-http`)

#### `crates/intercom-adapter-http/src/router.rs`
In `format_status_json`:
Serialize `"fabrics"` array inside `"active_ecosystem"`:
```rust
write!(writer, "}},\"fabrics\":[").map_err(|_| ())?;

for (idx, fab) in active.iter_fabrics().enumerate() {
    if idx > 0 {
        write!(writer, ",").map_err(|_| ())?;
    }
    write!(
        writer,
        "{{\"fab_idx\":{},\"vendor_id\":{},\"fabric_id\":\"0x{:016X}\",\"node_id\":\"0x{:016X}\",\"label\":\"{}\"}}",
        fab.fab_idx,
        fab.vendor_id,
        fab.fabric_id,
        fab.node_id,
        fab.label.as_str()
    )
    .map_err(|_| ())?;
}

write!(writer, "]}}").map_err(|_| ())?;
```

---

### 3.4. Frontend Web Dashboard (`crates/intercom-adapter-http/frontend`)

#### `frontend/src/types.ts`
```typescript
export interface FabricInfo {
  fab_idx: number;
  vendor_id: number;
  fabric_id: string; // 64-bit hex
  node_id: string;   // 64-bit hex
  label: string;
}

export interface ActiveEcosystem {
  type: string;
  paired: boolean;
  properties: {
    door_lock_subscribed?: boolean;
    ring_sensor_subscribed?: boolean;
    [key: string]: boolean | number | string | undefined;
  };
  fabrics?: FabricInfo[];
}
```

#### `frontend/src/components/EcosystemTab.tsx`
- Add Vendor ID mapping helper:
  - `0x1384` (4996) $\to$ Name: "Apple Home", Role: "Resident Hub", Icon: Apple
  - `0x1349` (4937) $\to$ Name: "Apple Home", Role: "Commissioner", Icon: Smartphone
  - `0x130D` (4877) $\to$ Name: "Home Assistant", Role: "Matter Server", Icon: Home
  - `0x18B4` (6324) $\to$ Name: "Google Home", Role: "Matter Controller", Icon: Globe
  - `0x111D` (4381) $\to$ Name: "Amazon Alexa", Role: "Echo / Hub", Icon: Box
  - `0x1160` (4448) $\to$ Name: "SmartThings", Role: "SmartThings Hub", Icon: Cpu
  - `0xFFF1`..`0xFFF4` $\to$ Name: "Test / Development", Role: "Matter Controller", Icon: Terminal
  - Default $\to$ Name: `Vendor 0x${vid.toString(16).toUpperCase()}`, Role: "Matter Controller", Icon: Layers
- Header badge: Show `N Hubs Connected` (e.g. `2 Hubs Connected`, `3 Hubs Connected`).
- Sub-header: Show capacity `N of 5 fabrics used`.
- Hub List: Render clean cards for each fabric with ecosystem icon, title, role pill, fabric #, label, and collapsible technical details (`Vendor ID`, `Fabric ID`, `Node ID`, `Fabric Index`).
- When no fabrics exist: Render `"Waiting for commissioning (BLE / Wi-Fi)"`.

---

## 4. Verification Plan

### Automated Unit Tests
1. **Core domain tests (`intercom-core`)**:
   - Verify `EcosystemStatus::with_fabric` and `iter_fabrics()` with 0 to 5 fabrics.
   - Verify fabric capacity boundary handling.
2. **HTTP Router tests (`intercom-adapter-http`)**:
   - Verify `format_status_json` formats empty fabrics (`"fabrics":[]`) and multiple populated fabrics (`"fabrics":[{...}]`).
   - Verify 64-bit hex formatting of `fabric_id` and `node_id`.
   - Verify buffer bounds.
3. **Frontend linting & build**:
   - `cargo fe-check` (Biome linter and formatter).
   - `cargo fe-build` (Solid.js production compilation into `assets/dashboard.html`).
4. **Host tests**:
   - `cargo test -p intercom-core -p intercom-adapter-http -p intercom-adapter-ota --target x86_64-apple-darwin`.
5. **Firmware compile check**:
   - `mise exec -- cargo build -p intercom-firmware`.

### Manual Verification
- Launch local frontend development server (`cargo fe-dev`) with mock telemetry containing multiple fabrics (Apple Home hub + iPhone + Home Assistant).
- Verify visual rendering, badge counts, expandable detail toggles, and responsive styling.
