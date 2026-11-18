# Proposal: Upstream Wi-Fi 6 (802.11ax) & Fast Scanning in rs-matter-embassy

## Why

Upstream `sysgrok/rs-matter-embassy` currently defaults to 802.11b/g/n when configuring ESP-family Wi-Fi radios (`esp-radio`). For modern Wi-Fi 6 microcontrollers like the ESP32-C6, enabling 802.11ax negotiation improves spectrum efficiency, reduces contention latency, and ensures compatibility with AX-only or WPA3-enterprise networks. Furthermore, implementing fast BSS re-association and RSSI deduplication minimizes network drop recovery times.

## What Changes

- In `rs-matter-embassy/src/wifi/esp.rs`:
  - Pass `Protocol::AX` in station configuration when building for 802.11ax-capable targets (ESP32-C6).
  - Use `ScanMethod::Fast` for immediate reconnect on the last known channel before all-channel scan.
  - Sort scan results descending by RSSI and deduplicate identical SSIDs, capping results to prevent heap exhaustion.

## Capabilities

### New Capabilities
- `embassy-wifi6-fast-scan`: 802.11ax protocol negotiation, fast channel re-association scanning, and RSSI deduplication in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/src/wifi/esp.rs`)
- **API Surface**:
  - Internal wireless driver tuning; public `EspWifiRadio` and `EspWifiDriver` signatures remain backward-compatible.
- **Breaking Changes**: None.
