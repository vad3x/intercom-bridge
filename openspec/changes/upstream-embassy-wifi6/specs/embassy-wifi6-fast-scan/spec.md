# Spec Delta: Wi-Fi 6 (802.11ax) Protocol Support & Fast Channel Scanning

## Purpose

Enables 802.11ax (Wi-Fi 6) protocol negotiation, fast channel re-association, and RSSI-sorted AP scan reporting in `rs-matter-embassy` for ESP32-C6 and Wi-Fi 6 capable SoCs.

## ADDED Requirements

### Requirement: Wi-Fi 6 (802.11ax) Protocol Negotiation
The `EspWifiController` SHALL include 802.11ax capability parameters in its station configuration during Wi-Fi association on 802.11ax-capable SoCs.

#### Scenario: Associating to 802.11ax access point
- **WHEN** the device scans and associates to a Wi-Fi 6 compatible access point
- **THEN** the station configuration includes `Protocol::AX` alongside legacy protocols
- **AND** the association completes with 802.11ax modulation active

### Requirement: Fast AP Association and All-Channel Scanning
The Wi-Fi controller SHALL use `ScanMethod::Fast` for immediate reconnect on the last known channel before triggering an all-channel fallback scan.

#### Scenario: Immediate reconnect after AP beacon loss
- **WHEN** the device loses association to the configured access point
- **THEN** the connection manager initiates fast scanning on the last known channel
- **AND** re-establishes network connectivity within the fast scan window

### Requirement: RSSI Sorting and BSSID Deduplication
The Wi-Fi scan reporting logic SHALL sort scanned access points descending by signal strength (RSSI), deduplicate multiple BSSIDs advertising the same SSID, and limit the reported list to prevent memory bloat.

#### Scenario: Scan results sorting and deduplication
- **WHEN** an AP scan detects multiple beacons with identical SSIDs across channels
- **THEN** the highest RSSI entry is preserved and subsequent duplicates are filtered out
- **AND** results are returned in descending order of signal strength
