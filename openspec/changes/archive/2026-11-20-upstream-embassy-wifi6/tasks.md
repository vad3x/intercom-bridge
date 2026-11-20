# Tasks: Upstream Configurable Scan Method, Scan Filtering & WPA2/3 Auth in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, fetch `upstream` (`git fetch upstream`) and cut clean feature branch `feat/embassy-wifi-scan-config` directly from `upstream/master` (which already includes `#96` `with_protocols`)
- [ ] 1.2 [RED] Write unit tests in `rs-matter-embassy/src/wifi/esp.rs` (or a host-testable scan helper module) asserting descending RSSI sort, hidden-SSID filtering on open scans, SSID deduplication keeping the strongest BSSID, `MAX_SCAN_RESULTS = 10` capping, and `with_scan_method` builder storage, verifying failure with: `cargo test --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --lib wifi::tests --target x86_64-apple-darwin`
- [ ] 1.3 [GREEN] Implement `with_scan_method(ScanMethod)` on `EspWifiRadio` (`rs-matter-embassy/src/wireless/wifi/esp_wifi.rs`) and `EspWifiController` (`rs-matter-embassy/src/wifi/esp.rs`), re-export `ScanMethod` in `wireless::esp`, forward it in `run()`/`run_coex()`, apply active scan dwell (`30ms`–`100ms`), RSSI sort/dedup/cap=10, `WpaWpa2Personal` bitmap, `Wpa2Personal` auth, and `.with_failure_retry_cnt(3)`, verifying tests pass with: `cargo test --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --lib wifi::tests --target x86_64-apple-darwin`
- [ ] 1.4 Verify compilation on host and cross-compilation for `riscv32imac-unknown-none-elf` with: `cargo check --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --target riscv32imac-unknown-none-elf --features esp,embassy-net,tcp,trouble,rustcrypto`
- [ ] 1.5 Run `cargo clippy --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --no-deps -- -Dwarnings` and `cargo fmt --manifest-path ../rs-matter-embassy/rs-matter-embassy/Cargo.toml --check`, and create the PR commit on `feat/embassy-wifi-scan-config` ready for upstream review

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-wifi6`.
