# Design: Downstream Intercom Adapter Wi-Fi 6, Upstream Sockets & On-Demand Buffers

## Context

The ESP32-C6 has 512 KiB SRAM (~320 KiB usable). Stack space is tuned to 48 KiB for Opus Silk encoding, leaving ~12 KiB of safe heap headroom. By isolating Wi-Fi 6 PHY policies and dynamic TCP buffer lifecycles in `intercom-adapter-matter`, we avoid maintaining a separate fork of `sysgrok/rs-matter-embassy`.

## Decisions

### 1. Zero-Fork Embassy Consumption
- **Decision**: Consume upstream `sysgrok/rs-matter-embassy` directly with `features = ["enet-extra-sockets-2"]`.
- **Rationale**: Upstream `master` already includes `enet-extra-sockets-1..8`.

### 2. Custom `IntercomWifiDriver`
- **Decision**: Implement `IntercomWifiDriver` in `intercom-adapter-matter` wrapping `esp-radio`/`esp-wifi`, configuring `Protocol::AX`, `ScanMethod::Fast`, and RSSI deduplication.
- **Rationale**: Keeps RF tuning and scan policies inside our adapter layer without requiring upstream PRs to `rs-matter-embassy`.

### 3. On-Demand TCP Buffers
- **Decision**: Allocate TCP RX/TX buffers on the heap when an incoming connection starts, returning them upon teardown.
- **Rationale**: Keeps 16–24 KiB of SRAM available for Opus encoding and Wi-Fi DMA during idle standby.

## Migration Plan

1. Update `Cargo.toml` dependency to upstream `rs-matter-embassy` with `enet-extra-sockets-2`.
2. Implement `IntercomWifiDriver` in `intercom-adapter-matter`.
3. Wire on-demand TCP buffer lifecycle into PR 2's TCP operational runner.
4. Verify all host tests pass and release firmware links within stack budgets.
5. Archive change with `openspec archive downstream-intercom-wifi6-buffers`.
