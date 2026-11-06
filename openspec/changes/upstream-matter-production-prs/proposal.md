# Proposal: Upstream Production PRs for rs-matter and rs-matter-embassy

## Why

The Intercom Bridge relies on key Matter 1.4 features and transport improvements currently maintained across local branches (`CameraAvStreamManagement` cluster 0x0551, Matter-over-TCP transport framing, and Embassy network memory optimizations). Upstreaming these changes into `project-chip/rs-matter` and `sysgrok/rs-matter-embassy` eliminates maintenance friction, ensures long-term alignment with the official Matter standard, and provides the broader embedded Rust community with first-class audio doorbell and low-memory TCP transport support.

## What Changes

- **feat(dm): Camera AV Stream Management cluster (0x0551) & Audio Doorbell device type (0x0141)**:
  - Add standard cluster ID `0x0551` and device type `0x0141` in `rs-matter-dm` and `rs-matter-dm-clusters`.
  - Implement cluster handlers, attribute read/write, and command dispatch (`AllocateAudioStream`, `DeallocateAudioStream`).
  - Enforce Matter global cluster attributes (`0xFFF8` to `0xFFFD`) with zero heap allocation and saturation-safe reference counting.
- **feat(transport/stack): embedded Matter-over-TCP operational runner**:
  - Integrate Matter TCP into the embedded stack using the specification's 4-byte little-endian length prefix. The current upstream `rs-matter-transport` already implements host TCP framing and should retain that behavior.
  - Integrate TCP listener on port 5540 into `rs-matter-stack` alongside UDP MRP.
  - Advertise TCP capability (`T=6`) when the embedded TCP server is enabled; this bitmap means TCP client (2) plus TCP server (4).
  - Introduce `WifiOnly` runner option for devices commissioned without BLE GATT overhead.
- **feat(embassy): On-demand socket buffer allocation & network tuning**:
  - Support on-demand allocation of Embassy TCP buffers for Matter port 5540, releasing 16–24 KiB of SRAM when TCP is idle.
  - Expose configurable `ENET_EXTRA_TCP_SOCKETS` sizing features.
  - Add 802.11ax (Wi-Fi 6) negotiation flags and fast BSS re-association hooks.

## Capabilities

### New Capabilities
- `camera-av-stream-cluster`: Standard Matter 1.4 Camera AV Stream Management cluster (0x0551) and Audio Doorbell device type (0x0141) data model abstractions and command handling.
- `matter-over-tcp`: Matter-compatible operational TCP framing, session lifecycle, mDNS T=6 advertisement, and non-blocking co-runner for embedded targets.
- `embassy-network-wiring`: On-demand TCP buffer lifecycle, configurable network socket allocation, and Wi-Fi 6 association tuning for the Embassy async runtime.

### Modified Capabilities
<!-- None: existing access-control domain capability remains unchanged -->

## Impact

- **Affected Repositories**:
  - `project-chip/rs-matter` (`rs-matter-dm`, `rs-matter-dm-clusters`, `rs-matter-transport`, `rs-matter-stack`)
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy`)
  - `intercom-matter` (simplifies `Cargo.toml` patch section to track upstream releases)
- **API Surface**:
  - New public cluster `CameraAvStreamManagement` in `rs_matter::dm::clusters`.
  - New transport configuration options in `rs_matter_stack::stack::MatterStack`.
  - Configurable features in `rs-matter-embassy/Cargo.toml`.
- **Breaking Changes**: None. All new capabilities are additive, backward-compatible, and feature-gated where appropriate.
