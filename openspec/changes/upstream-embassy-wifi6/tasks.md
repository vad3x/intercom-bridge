# Tasks: Upstream Wi-Fi 6 (802.11ax), Scan Tuning & RSSI Filtering in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, cut clean feature branch `feat/esp-wifi6-scan-tuning` from `upstream/master`
- [ ] 1.2 In `rs-matter-embassy/src/wifi/esp.rs`, configure `Protocols::default().with_2_4(Protocol::B | Protocol::G | Protocol::N | Protocol::AX)` on scan and connect `StationConfig`s (check whether `Protocol::AX` needs chip-feature gating in `esp-radio` for non-AX chips like ESP32/C3/S3)
- [ ] 1.3 Configure `ScanTypeConfig::Active { min: 30 ms, max: 100 ms }` in `scan` (aliasing `timers::Duration as TimerDuration`) and `ScanMethod::AllChannels` + `with_failure_retry_cnt(3)` in `connect` (preserving `Wpa2Personal` and no password logging)
- [ ] 1.4 Sort scan results descending by `signal_strength`, skip empty SSIDs on open scans, deduplicate by SSID, and cap at `MAX_SCAN_RESULTS = 10`
- [ ] 1.5 Verify `cargo fmt --all -- --check`, `cargo clippy`, and cross-compilation on `riscv32imac-unknown-none-elf` (and non-AX ESP feature if applicable)
- [ ] 1.6 Open PR to `sysgrok/rs-matter-embassy`

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-wifi6`.
