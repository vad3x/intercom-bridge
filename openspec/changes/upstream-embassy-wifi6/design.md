# Design: Upstream Wi-Fi 6 (802.11ax) & Fast Scanning in rs-matter-embassy

## Context

`rs-matter-embassy` provides `EspWifiRadio` and `EspWifiDriver` wrapping `esp-radio` for ESP32 devices. In `src/wifi/esp.rs`, default station configuration omits `Protocol::AX`, preventing the radio from negotiating Wi-Fi 6 rates.

## Decisions

### 1. Conditional 802.11ax Protocol Negotiation
- **Decision**: Configure `Protocols::default().with_2_4(Protocol::B | Protocol::G | Protocol::N | Protocol::AX)` on targets with 802.11ax support (e.g. ESP32-C6).
- **Rationale**: Enables OFDMA and Target Wake Time (TWT) benefits on modern APs while maintaining full backward-compatibility with Wi-Fi 4/5 access points.

### 2. Fast Scan Reconnection with All-Channel Fallback
- **Decision**: Set `StationConfig::with_scan_method(ScanMethod::Fast)` with a 3-retry count before falling back to full-channel scan.
- **Rationale**: Re-associates within milliseconds when an AP briefly drops or re-keys, avoiding high-latency full-band dwells.

### 3. RSSI Sorting and Deduplication
- **Decision**: Sort scanned APs by descending RSSI and deduplicate entries with identical SSIDs.
- **Rationale**: Prevents buffer overflow and ensures commissioners are presented with the strongest BSSID.

## Migration Plan

1. Cut clean feature branch `feat/embassy-wifi6-fast-scan` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Port changes to `src/wifi/esp.rs`.
3. Verify compilation on host and `riscv32imac-unknown-none-elf`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Archive change with `openspec archive upstream-embassy-wifi6`.
