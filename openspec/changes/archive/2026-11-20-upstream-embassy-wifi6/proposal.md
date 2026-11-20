# Proposal: Upstream Configurable Scan Method, Scan Filtering & WPA2/3 Auth in rs-matter-embassy

## Why

Upstream `sysgrok/rs-matter-embassy` (`#96`) added opt-in `with_protocols(Protocols)` for 802.11ax, but station scanning and connection in `src/wifi/esp.rs` still have three gaps for real-world deployments:

1. Hardcoding `ScanMethod::AllChannels` fails in dense RF environments with many surrounding networks because the full-channel scan overflows the device's AP scan buffer, whereas `ScanMethod::Fast` avoids that overflow on known channels. Callers need to configure `ScanMethod` (`Fast` vs `AllChannels`) from the library consumer side.
2. Unsorted, non-deduplicated scan results can exceed Matter's scan list limits (`CHIP_DEVICE_CONFIG_MAX_SCAN_NETWORKS_RESULTS = 10`) or surface weaker BSSIDs and hidden/empty SSIDs ahead of stronger ones.
3. Standard WPA2 and WPA2/WPA3 transition-mode access points require `AuthenticationMethodConfig::Wpa2Personal`, explicit `WpaWpa2Personal` security bitmap mapping, and connection retry tuning (`failure_retry_cnt(3)`).

## What Changes

- In `rs-matter-embassy/src/wifi/esp.rs` and `rs-matter-embassy/src/wireless/wifi/esp_wifi.rs`:
  - Add an optional `with_scan_method(ScanMethod)` builder method to `EspWifiRadio` and `EspWifiController` (and re-export `ScanMethod` from `wireless::esp`), leaving `StationConfig::default()` untouched when unset.
  - Configure active scan dwell times (`ScanTypeConfig::Active { min: 30ms, max: 100ms }`), `AuthenticationMethodConfig::Wpa2Personal` for non-empty passwords, `WpaWpa2Personal` security bitmap reporting, and `failure_retry_cnt(3)` during station connect.
  - Sort scan results descending by RSSI, filter hidden/empty SSIDs on open scans, deduplicate identical SSIDs, and cap reported results at `MAX_SCAN_RESULTS = 10`.

## Capabilities

### New Capabilities
- `embassy-wifi6-fast-scan`: Configurable `ScanMethod` (`Fast` / `AllChannels`) builder, active scan timing, WPA2/WPA3 transition auth & retry count, and RSSI-sorted/deduplicated AP scan reporting in `rs-matter-embassy` (802.11ax `with_protocols` is already covered upstream by `#96`).

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/src/wifi/esp.rs`, `rs-matter-embassy/src/wireless/wifi/esp_wifi.rs`)
- **API Surface**:
  - Backward-compatible `const fn with_scan_method(mut self, scan_method: ScanMethod) -> Self` builder on `EspWifiRadio` and `EspWifiController` (matching `#96`'s `with_protocols` pattern).
- **Breaking Changes**: None.
