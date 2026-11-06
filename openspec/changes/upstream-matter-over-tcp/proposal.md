# Proposal: Embedded Matter-over-TCP Integration & Operational Runner in rs-matter

## Why

Matter specification defines operational messaging over TCP (port 5540) using 4-byte little-endian length prefix framing. While `project-chip/rs-matter` has host TCP framing in `rs-matter-transport`, it lacks an embedded operational TCP runner and port 5540 listener in `rs-matter-stack`. Integrating embedded Matter-over-TCP into `rs-matter-stack` enables reliable, connection-oriented Matter operational communication alongside UDP MRP for embedded microcontrollers.

## What Changes

- Implement or reuse `no_std` Matter TCP framing for the embedded listener using 4-byte little-endian length prefix framing. Preserve existing host framing in `rs-matter-transport`.
- Add TCP accept/listen support to `rs-matter-stack`'s network abstraction and wire its port 5540 listener into the operational runner cooperatively with UDP MRP so TCP activity cannot starve UDP.
- Set operational device `tcp_supported` flag when embedded TCP server is enabled to advertise `T=6` in operational mDNS TXT records.
- Add `WifiOnly` runner path in `rs-matter-stack` bypassing BLE GATT initialization for already-provisioned devices.
- Add integration tests covering single-connection concurrency control, idle connection timeouts, and connection teardown.

## Capabilities

### New Capabilities
- `matter-over-tcp`: Matter-compatible operational TCP framing, session lifecycle, mDNS T=6 advertisement, and non-blocking co-runner for embedded targets in `rs-matter`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `project-chip/rs-matter` (`rs-matter-transport`, `rs-matter-stack`)
- **API Surface**:
  - Embedded TCP listener and runner integration in `rs_matter_stack::stack::MatterStack`.
  - `WifiOnly` runner configuration in `rs_matter_stack::wireless::wifi`.
- **Breaking Changes**: None. Additive transport functionality.
