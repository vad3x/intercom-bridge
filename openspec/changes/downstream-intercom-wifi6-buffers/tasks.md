# Tasks: Downstream Intercom Adapter Wi-Fi 6, Upstream Sockets & On-Demand Buffers

- [ ] 3.1 Configure `rs-matter-embassy` dependency to consume upstream `sysgrok/rs-matter-embassy` with `features = ["enet-extra-sockets-2"]`, verifying compilation and socket headroom for REST and WebSocket servers
- [ ] 3.2 Implement `IntercomWifiDriver` in `intercom-adapter-matter/src/wireless.rs` supporting 802.11ax negotiation, fast AP scanning (`ScanMethod::Fast`), and RSSI deduplication
- [ ] 3.3 Implement adapter-level on-demand TCP socket buffer allocation and teardown in `intercom-adapter-matter`, freeing 16–24 KiB of SRAM during idle standby
- [ ] 3.4 Verify all 107 host tests pass (`cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool --target x86_64-apple-darwin`)
- [ ] 3.5 Build release DevKit firmware with `cargo build-devkit` and verify the binary links cleanly within the 48 KiB stack budget

## Workflow follow-up

- Archive the OpenSpec change with `openspec archive downstream-intercom-wifi6-buffers`.
