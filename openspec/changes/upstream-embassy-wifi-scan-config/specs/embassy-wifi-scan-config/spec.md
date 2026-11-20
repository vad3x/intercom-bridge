# Spec Delta: Configurable Scan Method, Scan Result Filtering & WPA2/3 Auth

## Purpose

Enables consumer-configurable `ScanMethod` (`Fast` vs `AllChannels`), active scan dwell tuning, WPA2/WPA3 transition authentication with connect retry configuration, and RSSI-sorted/deduplicated AP scan reporting in `rs-matter-embassy` (complementing `with_protocols` merged in `#96`).

## ADDED Requirements

### Requirement: Configurable Station Scan Method & Connect Retries
`EspWifiRadio` and `EspWifiController` SHALL provide an opt-in `with_scan_method(ScanMethod)` builder method (and re-export `ScanMethod` from `wireless::esp`), leaving `StationConfig::default()`'s scan method unchanged when unset and applying the configured `ScanMethod` alongside `AuthenticationMethodConfig::Wpa2Personal` (for non-empty passwords) and `.with_failure_retry_cnt(3)` during station connection.

#### Scenario: Caller leaves scan method unset
- **WHEN** `EspWifiRadio::new(...)` or `EspWifiController::new(...)` is used without calling `with_scan_method`
- **THEN** `StationConfig::default()`'s scan method is preserved without overriding it

#### Scenario: Caller selects `ScanMethod::Fast` or `ScanMethod::AllChannels`
- **WHEN** the caller invokes `.with_scan_method(ScanMethod::Fast)` (or `ScanMethod::AllChannels`) on `EspWifiRadio`
- **THEN** `EspWifiRadio` forwards that `ScanMethod` to `EspWifiController` for both `run()` and `run_coex()`
- **AND** `EspWifiController::connect()` applies it to `StationConfig` alongside `.with_failure_retry_cnt(3)`

### Requirement: RSSI Sorting, SSID Deduplication, and Result Cap
The Wi-Fi scan reporting logic SHALL use active scan dwell times (`30ms`–`100ms`), sort scanned access points descending by signal strength (RSSI), filter hidden/empty SSIDs on open scans, deduplicate multiple BSSIDs advertising the same SSID, map `WpaWpa2Personal` to `WPA_PERSONAL | WPA_2_PERSONAL`, and cap reported entries at `MAX_SCAN_RESULTS = 10`.

#### Scenario: Scan results sorting and deduplication
- **WHEN** an AP scan detects multiple beacons with identical SSIDs across channels
- **THEN** the highest RSSI entry is preserved and subsequent duplicates are filtered out
- **AND** results are returned in descending order of signal strength, capped at 10 entries
