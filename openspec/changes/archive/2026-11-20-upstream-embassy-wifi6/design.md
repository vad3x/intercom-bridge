# Design: Upstream Configurable Scan Method, Scan Filtering & WPA2/3 Auth in rs-matter-embassy

## Context

`rs-matter-embassy` provides `EspWifiRadio` (`src/wireless/wifi/esp_wifi.rs`) and `EspWifiController` (`src/wifi/esp.rs`) wrapping `esp-radio` for ESP32 devices. Upstream PR `#96` (`1cc8be2`) already added an opt-in `with_protocols(Protocols)` builder on both types while keeping `StationConfig::default()` intact.

To eliminate all remaining local `src/wifi/esp.rs` diffs (`2cc1f75`, `33d93ae`, `0adec51`, `cf19288` dwell, `020b97d`), station scanning and association need:
1. Consumer-configurable `ScanMethod` (`Fast` vs `AllChannels`), because `ScanMethod::AllChannels` overflows the device AP buffer in dense RF environments with many neighboring SSIDs.
2. Predictable AP scan reporting (active scan dwell times, descending RSSI ordering, hidden-SSID filtering on open scans, SSID deduplication, `WpaWpa2Personal` security bitmap mapping, and `MAX_SCAN_RESULTS = 10` cap).
3. `AuthenticationMethodConfig::Wpa2Personal` and `.with_failure_retry_cnt(3)` during station connect so standard WPA2 and WPA2/WPA3 transition-mode networks associate reliably.

## Decisions

### 1. Opt-In `with_scan_method(ScanMethod)` Builder (Matching `#96`)
- **Decision**: Add `Option<ScanMethod>` (default `None`) and `pub const fn with_scan_method(mut self, scan_method: ScanMethod) -> Self` to both `EspWifiRadio` and `EspWifiController`, forwarding `self.radio.scan_method` in `run()` and `run_coex()`. Re-export `ScanMethod` alongside `Protocol`/`Protocols` in `wireless::esp`.
- **Rationale**: Preserves `StationConfig::default()` for existing callers while letting downstream applications choose `ScanMethod::Fast` (to avoid scan-buffer overflows in dense Wi-Fi environments) or `ScanMethod::AllChannels` (for multi-AP/roaming setups) alongside `.with_failure_retry_cnt(3)`.

### 2. Active Scan Timing, RSSI Sorting, Deduplication, and Cap
- **Decision**: Extract a pure, host-testable scan-filtering helper (or test the sorting/deduplication/cap logic) and configure `ScanTypeConfig::Active { min: Duration::from_millis(30), max: Duration::from_millis(100) }`, sort scanned APs descending by `signal_strength`, skip empty SSIDs on open scans (`network.is_none()`), deduplicate by SSID keeping the strongest RSSI entry, map `AuthenticationMethod::WpaWpa2Personal` to `WPA_PERSONAL | WPA_2_PERSONAL`, and stop at `MAX_SCAN_RESULTS = 10`.
- **Rationale**: Prevents buffer bloat, matches Matter's `CHIP_DEVICE_CONFIG_MAX_SCAN_NETWORKS_RESULTS`, and presents commissioners with the strongest BSSID per SSID.

## Migration Plan

1. Cut clean feature branch `feat/embassy-wifi-scan-config` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Implement `with_scan_method`, scan filtering/dedup/cap, and WPA2/3 retry handling in `src/wifi/esp.rs` and `src/wireless/wifi/esp_wifi.rs`.
3. Verify host unit tests and cross-compilation for `riscv32imac-unknown-none-elf`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Archive change with `openspec archive upstream-embassy-wifi6`.
