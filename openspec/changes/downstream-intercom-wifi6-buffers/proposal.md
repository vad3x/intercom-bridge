# Proposal: Downstream Intercom Adapter Wi-Fi 6, Upstream Sockets & On-Demand Buffers

## Why

The Intercom Bridge runs on ESP32-C6 (Wi-Fi 6 capable) alongside an embedded HTTP server and WebRTC Opus audio engine. To prevent stack/heap memory exhaustion while avoiding custom fork patches against `sysgrok/rs-matter-embassy`, we encapsulate ESP32-C6 Wi-Fi 6 tuning, upstream socket capacity, and on-demand TCP buffer management within `intercom-adapter-matter`.

## What Changes

- Consume upstream `sysgrok/rs-matter-embassy` directly with `features = ["enet-extra-sockets-2"]` for application HTTP/WebSocket headroom.
- Implement `IntercomWifiDriver` in `intercom-adapter-matter` supporting 802.11ax negotiation, fast AP scanning, and RSSI deduplication.
- Implement adapter-level on-demand TCP socket buffer allocation and release, freeing 16–24 KiB of SRAM during idle standby.
- Verify release DevKit firmware builds cleanly within the 48 KiB execution stack budget.

## Capabilities

### New Capabilities
- `adapter-wifi6-and-buffers`: Hardware-optimized Wi-Fi 6 driver, upstream socket capacity configuration, and dynamic on-demand TCP buffer lifecycle for Intercom Bridge.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `intercom-matter` (`intercom-adapter-matter`, `intercom-firmware`, `Cargo.toml`)
- **API Surface**:
  - `IntercomWifiDriver` in `intercom-adapter-matter`.
- **Breaking Changes**: None.
