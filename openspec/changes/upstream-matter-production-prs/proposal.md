# Proposal: Upstream Production PRs for rs-matter and Downstream Adapter Integration

## Why

The Intercom Bridge relies on key Matter 1.4 features and transport improvements currently maintained across local branches (`CameraAvStreamManagement` cluster 0x0551 and embedded Matter-over-TCP transport framing). Upstreaming these changes directly into `project-chip/rs-matter` eliminates maintenance friction, ensures long-term alignment with the official Matter standard, and provides the embedded Rust community with first-class audio doorbell and low-memory TCP transport support.

To minimize external review dependencies and avoid multi-repository PR lockstep, `sysgrok/rs-matter-embassy` is consumed as-is directly from upstream using its existing `enet-extra-sockets-2` feature. Hardware and platform-specific policies—including ESP32-C6 Wi-Fi 6 (802.11ax) negotiation, fast BSS re-association scanning, and on-demand TCP buffer lifecycle management—are isolated directly within the `intercom-adapter-matter` layer in accordance with Hexagonal Architecture.

## What Changes

- **feat(dm): Camera AV Stream Management cluster (0x0551) & Audio Doorbell device type (0x0141)**:
  - Add standard cluster ID `0x0551` and device type `0x0141` in `rs-matter-dm` and `rs-matter-dm-clusters`.
  - Implement cluster handlers, attribute read/write, and command dispatch (`AllocateAudioStream`, `DeallocateAudioStream`).
  - Enforce Matter global cluster attributes (`0xFFF8` to `0xFFFD`) with zero heap allocation and saturation-safe reference counting.
- **feat(transport/stack): embedded Matter-over-TCP operational runner**:
  - Integrate Matter TCP into the embedded stack using the specification's 4-byte little-endian length prefix. Preserve existing host TCP framing in `rs-matter-transport`.
  - Integrate TCP listener on port 5540 into `rs-matter-stack` alongside UDP MRP.
  - Advertise TCP capability (`T=6`) when the embedded TCP server is enabled (TCP client 2 + TCP server 4).
  - Introduce `WifiOnly` runner option for devices commissioned without BLE GATT overhead.
- **downstream(adapter): Intercom Wi-Fi 6 driver, upstream socket configuration & on-demand buffers**:
  - Consume upstream `rs-matter-embassy` directly with `features = ["enet-extra-sockets-2"]` for application HTTP/WebSocket headroom without custom fork patches.
  - Implement `IntercomWifiDriver` in `intercom-adapter-matter` with 802.11ax negotiation, fast AP scanning, and RSSI deduplication.
  - Implement on-demand allocation and teardown of TCP socket buffers in `intercom-adapter-matter`, saving 16–24 KiB of SRAM during idle standby.

## Capabilities

### New Capabilities
- `camera-av-stream-cluster`: Standard Matter 1.4 Camera AV Stream Management cluster (0x0551) and Audio Doorbell device type (0x0141) data model abstractions and command handling in `rs-matter`.
- `matter-over-tcp`: Matter-compatible operational TCP framing, session lifecycle, mDNS T=6 advertisement, and non-blocking co-runner for embedded targets in `rs-matter`.

### Modified Capabilities
<!-- None: existing access-control domain capability remains unchanged -->

## Impact

- **Affected Repositories**:
  - `project-chip/rs-matter` (`rs-matter-dm`, `rs-matter-dm-clusters`, `rs-matter-transport`, `rs-matter-stack`)
  - `intercom-matter` (`intercom-adapter-matter`, `intercom-firmware`, simplifying `Cargo.toml` patch section)
- **API Surface**:
  - New public cluster `CameraAvStreamManagement` in `rs_matter::dm::clusters`.
  - New transport configuration options in `rs_matter_stack::stack::MatterStack`.
  - New `IntercomWifiDriver` in `intercom-adapter-matter`.
- **Breaking Changes**: None. All new capabilities are additive, backward-compatible, and feature-gated where appropriate.
