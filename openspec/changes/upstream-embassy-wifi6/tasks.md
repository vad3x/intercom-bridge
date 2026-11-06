# Tasks: Upstream Wi-Fi 6 (802.11ax) & Fast Scanning in rs-matter-embassy

- [ ] 1.1 In `rs-matter-embassy`, fetch `upstream` (`git fetch upstream`) and cut clean feature branch `feat/embassy-wifi6-fast-scan` directly from `upstream/master`
- [ ] 1.2 Implement 802.11ax protocol configuration in `rs-matter-embassy/src/wifi/esp.rs` with `Protocols::default().with_2_4(Protocol::B | Protocol::G | Protocol::N | Protocol::AX)`
- [ ] 1.3 Configure fast scan method (`ScanMethod::Fast`) and failure retry count in `StationConfig`
- [ ] 1.4 Implement descending RSSI sorting and SSID deduplication in AP scan reporting
- [ ] 1.5 Verify compilation on host target and cross-compilation for `riscv32imac-unknown-none-elf` (`cargo check --target riscv32imac-unknown-none-elf`)
- [ ] 1.6 Run `cargo clippy` and `cargo fmt` on `rs-matter-embassy`, and create PR commit on `feat/embassy-wifi6-fast-scan` ready for upstream review

## Workflow follow-up

- Submit PR to `sysgrok/rs-matter-embassy` repository.
- Archive the OpenSpec change with `openspec archive upstream-embassy-wifi6`.
