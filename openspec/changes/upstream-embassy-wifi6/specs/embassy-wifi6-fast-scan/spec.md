# Spec Delta: Wi-Fi 6 (802.11ax), Scan Tuning & RSSI Filtering

## Purpose

Enables 802.11ax (Wi-Fi 6) protocol negotiation, tuned active/all-channel scanning with retries, and RSSI-sorted deduplicated AP scan reporting in `rs-matter-embassy` (`EspWifiController`).

## ADDED Requirements

### Requirement: Wi-Fi 6 (802.11ax) Protocol Negotiation
The `EspWifiController` SHALL include `Protocol::B | Protocol::G | Protocol::N | Protocol::AX` in its 2.4 GHz station configuration during both scanning and association on 802.11ax-capable SoCs.

#### Scenario: Scanning and associating with 802.11ax support
- **WHEN** `EspWifiController` configures `StationConfig` for `scan` or `connect`
- **THEN** 2.4 GHz protocols include `Protocol::B | Protocol::G | Protocol::N | Protocol::AX`

### Requirement: Active Scan Dwell and All-Channel Association Retries
The `EspWifiController` SHALL use active scan dwell (`min = 30 ms`, `max = 100 ms`) during `scan`, and `ScanMethod::AllChannels` with `failure_retry_cnt = 3` during `connect`.

#### Scenario: Commissioning scan and AP connection
- **WHEN** a commissioner triggers `scan` or `connect`
- **THEN** `scan` uses `ScanTypeConfig::Active { min: 30ms, max: 100ms }`
- **AND** `connect` uses `ScanMethod::AllChannels` and `with_failure_retry_cnt(3)` without logging the plaintext Wi-Fi password

### Requirement: RSSI Sorting, Hidden-SSID Filtering, Deduplication, and Cap
The Wi-Fi scan reporting logic SHALL sort scanned access points descending by RSSI, skip empty SSIDs on open scans (`network.is_none()`), deduplicate identical SSIDs (keeping the strongest BSSID), and cap reported results at 10.

#### Scenario: Scan results sorting and deduplication
- **WHEN** an AP scan detects multiple beacons with identical SSIDs or more than 10 APs
- **THEN** only the highest-RSSI entry per non-empty SSID is reported
- **AND** results are emitted in descending RSSI order up to `MAX_SCAN_RESULTS = 10`
