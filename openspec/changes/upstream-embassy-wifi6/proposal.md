# Proposal: Upstream Wi-Fi 6 (802.11ax), Scan Tuning & RSSI Filtering in rs-matter-embassy

## Why

Upstream `sysgrok/rs-matter-embassy` leaves `esp-radio` station configuration on defaults, omitting `Protocol::AX` (802.11ax), active scan dwell tuning, `ScanMethod::AllChannels` + `failure_retry_cnt`, and AP scan result filtering. On ESP32-C6 and crowded 2.4 GHz environments, this prevents 802.11ax negotiation, causes missed APs during short default scans, and can flood Matter `NetworkCommissioning` `ScanNetworks` responses with duplicate BSSIDs or hidden SSIDs.

## What Changes

- In `rs-matter-embassy/src/wifi/esp.rs`:
  - Configure `Protocols::default().with_2_4(Protocol::B | Protocol::G | Protocol::N | Protocol::AX)` on both scan and connect `StationConfig`s (verify `Protocol::AX` availability across `esp-radio` chip features).
  - Configure `ScanTypeConfig::Active { min: 30 ms, max: 100 ms }` in `scan` for reliable beacon discovery.
  - Configure `ScanMethod::AllChannels` and `with_failure_retry_cnt(3)` in `connect`.
  - Sort `scan_async` results descending by RSSI, skip empty/hidden SSIDs on open scans, deduplicate by SSID (keeping the strongest BSSID), and cap reported results at `MAX_SCAN_RESULTS = 10` (`CHIP_DEVICE_CONFIG_MAX_SCAN_NETWORKS_RESULTS`).

## Capabilities

### New Capabilities
- `embassy-wifi6-fast-scan`: 802.11ax protocol negotiation, tuned active/all-channel scanning with retry count, and RSSI-sorted deduplicated scan reporting in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/src/wifi/esp.rs`)
- **API Surface**:
  - Internal wireless driver tuning; public `EspWifiRadio`, `EspWifiDriver`, and `EspWifiController` signatures remain unchanged.
- **Breaking Changes**: None.
