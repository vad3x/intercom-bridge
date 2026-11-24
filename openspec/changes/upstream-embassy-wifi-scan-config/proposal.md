# Proposal: Configurable, backward-compatible ESP Wi-Fi behavior

## Why

The ESP Wi-Fi adapter needs configurable scan, connection, and result-handling behavior for deployments with different access points, while existing callers must retain their current behavior by default.

## What Changes

- Add one optional `WifiConfig` accepted by `EspWifiRadio` and `EspWifiController`.
- Include driver scan settings, optional station association scan method and retry count, and independently configurable result handling.
- Preserve current behavior when callers do not supply the config or leave individual result options disabled.
- Let a non-empty Matter SSID request override only the configured driver scan SSID filter.

## Capabilities

### New Capabilities

- `embassy-wifi-scan-config` allows applications to configure ESP Matter Wi-Fi scans, station connection retries, and scan result reporting.

## Impact

The change affects `rs-matter-embassy`'s ESP Wi-Fi adapter. Existing callers keep driver scan and station connection defaults, driver result order, duplicate BSSIDs, and unfiltered results unless they opt in to changes.
