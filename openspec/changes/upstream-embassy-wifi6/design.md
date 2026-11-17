# Design: Upstream Wi-Fi 6 (802.11ax), Scan Tuning & RSSI Filtering in rs-matter-embassy

## Context

`rs-matter-embassy` provides `EspWifiController` wrapping `esp-radio` (`WifiController`) in `rs-matter-embassy/src/wifi/esp.rs`. Upstream currently uses `StationConfig::default()` for scanning and only sets `ssid` + `authentication` for connecting.

## Decisions

### 1. 802.11ax Protocol Negotiation
- **Decision**: Configure `Protocols::default().with_2_4(Protocol::B | Protocol::G | Protocol::N | Protocol::AX)` on both scan and connect `StationConfig`s (gating or verifying `Protocol::AX` on non-AX ESP targets if `esp-radio` requires it).
- **Rationale**: Enables 802.11ax (Wi-Fi 6) negotiation on capable SoCs like ESP32-C6 while remaining compatible with 802.11b/g/n APs.

### 2. Extended Active Scan Dwell & All-Channel Association
- **Decision**: Set `ScanTypeConfig::Active { min: Duration::from_millis(30), max: Duration::from_millis(100) }` in `scan`, and `ScanMethod::AllChannels` + `with_failure_retry_cnt(3)` in `connect`. (Alias `rs_matter::utils::timers::Duration` as `TimerDuration` to avoid colliding with `esp_hal::time::Duration`.)
- **Rationale**: Longer active dwell reduces missed APs during Matter commissioning `ScanNetworks`, while `AllChannels` + `failure_retry_cnt(3)` ensures reliable association to the strongest AP even across channel changes or transient auth/beacon drops.

### 3. RSSI Sorting, Hidden-SSID Filtering, Deduplication, and Cap
- **Decision**: Sort raw scan results descending by `signal_strength`, skip empty SSIDs on open (`network.is_none()`) scans, deduplicate by SSID using a fixed `[Option<&str>; 10]` stack array, and stop after `MAX_SCAN_RESULTS = 10` (`CHIP_DEVICE_CONFIG_MAX_SCAN_NETWORKS_RESULTS`).
- **Rationale**: Zero-allocation deduplication keeps only the strongest BSSID per SSID and prevents Matter `ScanNetworks` response overflow in dense Wi-Fi environments.

## Migration Plan

1. Cut clean feature branch `feat/esp-wifi6-scan-tuning` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Port the net `src/wifi/esp.rs` changes (omitting temporary password-logging and `WpaWpa2Personal` experiments).
3. Verify compilation across ESP targets (`esp32c6` and at least one non-AX target such as `esp32c3` / `esp32s3`) plus `cargo fmt` and `cargo clippy`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Archive change with `openspec archive upstream-embassy-wifi6`.
