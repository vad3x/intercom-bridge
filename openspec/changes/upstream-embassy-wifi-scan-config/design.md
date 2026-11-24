# Design: Configurable ESP Wi-Fi behavior

## Configuration API

Add one public `WifiConfig` on the ESP wireless API and an optional `with_wifi_config(WifiConfig)` builder on `EspWifiRadio` and `EspWifiController`. The radio forwards the same configuration to the controller in both `run()` and `run_coex()`.

The config groups three concerns:

- Driver network scans use `esp_radio::wifi::scan::ScanConfig`.
- Station association has optional `ScanMethod` and failure retry count overrides.
- Matter scan result reporting has independent options for RSSI sorting, SSID deduplication, filtering empty SSIDs on open scans, and a final result cap.

Provide defaults for the aggregate config and each result option. With no config, or with default values, preserve existing behavior. In particular, keep the driver's `ScanConfig::default()`, `StationConfig::default()` scan method and retry count, driver result order, duplicate BSSIDs, and reporting of empty SSIDs.

## Scan configuration and SSID precedence

Build each driver scan from the supplied `ScanConfig`, or from `ScanConfig::default()` when the aggregate config is absent. If Matter requests a non-empty SSID, apply it as the SSID filter while preserving all other scan settings. If the request has no SSID, retain the configured SSID filter.

## Connection configuration

Apply association scan method and retry count only when their config fields are set. Leave the remaining `StationConfig` fields at their driver defaults. Keep connection settings independent of `ScanConfig`, which controls Matter's network discovery scan.

## Result processing

Apply only the result options that callers enable. If RSSI sorting is disabled, preserve driver order. If deduplication is enabled, keep the strongest RSSI result for each SSID. Preserve the first occurrence's position when replacing it with a stronger duplicate. Filter empty SSIDs only when that option is enabled and the Matter request is an open scan.

Apply `max_results` to the final list after enabled filtering and deduplication. An unset cap reports all resulting entries.

## Testing

Use host tests for the pure result-processing logic. Cover every option independently, their interactions, strongest-BSSID selection without sorting, result ordering, final-list capping, and error propagation. Add an ESP32-C6 compile test for the public aggregate config and both builders. Keep a RED/GREEN pair for each implementation task.
