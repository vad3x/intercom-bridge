# Spec Delta: Configurable ESP Wi-Fi behavior

## Purpose

Allow applications to opt into ESP Wi-Fi scan, connection, and result-reporting behavior while preserving existing behavior by default.

## ADDED Requirements

### Requirement: Aggregate Wi-Fi configuration

The ESP Wi-Fi API SHALL provide a `WifiConfig` accepted by `EspWifiRadio` and `EspWifiController` through `with_wifi_config`. The radio SHALL forward the configuration through both Wi-Fi startup paths.

#### Scenario: Caller omits the configuration

- **WHEN** an existing caller constructs `EspWifiRadio` without `with_wifi_config`
- **THEN** Matter scans SHALL use `ScanConfig::default()`
- **AND** station association SHALL retain the `StationConfig::default()` scan method and retry count
- **AND** scan results SHALL be reported in driver order, including duplicate BSSIDs and empty SSIDs

#### Scenario: Caller supplies default configuration

- **WHEN** a caller supplies `WifiConfig::default()`
- **THEN** observable scan, connection, and reporting behavior SHALL match the omitted-config scenario

### Requirement: Matter scan configuration

The adapter SHALL use the configured driver `ScanConfig` for Matter network scans, or `ScanConfig::default()` when no override is present.

#### Scenario: Matter requests a specific SSID

- **WHEN** a Matter scan request contains a non-empty SSID
- **THEN** the adapter SHALL use that SSID as the driver filter
- **AND** SHALL preserve the other configured scan fields

#### Scenario: Matter request has no SSID

- **WHEN** the Matter scan request contains no SSID or an empty SSID
- **THEN** the adapter SHALL preserve the configured scan SSID filter

### Requirement: Optional station connection overrides

The connection section SHALL allow callers to set an association `ScanMethod` and failure retry count independently. Unset fields SHALL retain their `StationConfig::default()` values.

#### Scenario: Caller sets one connection override

- **WHEN** the caller sets either association scan method or retry count
- **THEN** the adapter SHALL apply that field and preserve the driver default for the other unset field

### Requirement: Independent result handling options

The result section SHALL independently allow RSSI sorting, SSID deduplication, filtering empty SSIDs during open scans, and a maximum number of reported results. Each option SHALL be disabled or unset by default.

#### Scenario: Caller enables RSSI sorting

- **WHEN** RSSI sorting is enabled
- **THEN** reported results SHALL be ordered by descending RSSI

#### Scenario: Caller enables SSID deduplication

- **WHEN** multiple BSSIDs advertise the same SSID
- **THEN** the strongest RSSI result SHALL be retained for that SSID
- **AND** its position SHALL be the position where that SSID first appeared in driver results when RSSI sorting is disabled

#### Scenario: Caller enables hidden SSID filtering

- **WHEN** hidden SSID filtering is enabled for an open scan
- **THEN** results with an empty SSID SHALL be omitted
- **AND** when filtering is disabled those results SHALL be reported

#### Scenario: Caller sets a result cap

- **WHEN** a maximum result count is configured
- **THEN** the cap SHALL apply after enabled sorting, filtering, and deduplication
