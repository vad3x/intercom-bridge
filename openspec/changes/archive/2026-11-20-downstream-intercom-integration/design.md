# Design: Downstream Intercom Bridge Integration & Local Patch Elimination

## Context

`intercom-matter` currently links against a local sibling `rs-matter-embassy` checkout that carries 12 commits ahead of `upstream/master` (`f9e2c30`) plus uncommitted S31 `wifi-only` edits. Our audit split those local modifications into:
- **Bucket A (Drop locally, 0 upstream PRs needed)**:
  1. Uncommitted S31 `wifi-only` WIP (8 files) and `cf19288` (`Protocol::AX`, superseded by `#96` `with_protocols`).
  2. Custom `ENET_EXTRA_TCP_SOCKETS = 2` / `ENET_EXTRA_UDP_SOCKETS = 2` (`e2495cd`, `11d40d6`), which accidentally replaced upstream `#66`'s `enet-extra-sockets-4` feature during rebase.
  3. `OnceCell<Box<...>>` heap wrappers in `rs-matter-embassy/src/eth.rs` (`11d40d6`, `1665e20`), which moved `EnetMatterUdpBuffers`, `EnetMatterTcpBuffers`, and `EnetMatterTcpTransportBuffers` (~18.5 KiB) out of `.bss` into the 120 KiB C6 heap at startup.
- **Bucket B (4 upstream PRs to `sysgrok/rs-matter-embassy`)**:
  1. Wi-Fi scan config (`with_scan_method`), active scan timing, RSSI sort/dedup/cap=10, and `Wpa2Personal`/`WpaWpa2Personal` auth (`upstream-embassy-wifi6`).
  2. Opt-in `dhcpv4-hostname` (DHCP Option 12).
  3. Opt-in `mld-v1-compat` and `nd-learn-from-frames` shims.
  4. Exposing `embassy_net::Stack` to coexisting application tasks (HTTP/WebSocket/OTA, WebRTC, IPv4 diagnostics).

## Decisions

### 1. Replace Custom Extra-Socket Constants with Upstream `enet-extra-sockets-4`
- **Decision**: Restore `upstream/master`'s `ENET_EXTRA_SOCKET_SET` in `rs-matter-embassy/src/enet.rs` and add `"enet-extra-sockets-4"` to `rs-matter-embassy` features in `crates/intercom-adapter-matter/Cargo.toml`.
- **Rationale**: Uses the existing upstream feature flag from `sysgrok/rs-matter-embassy#66` to reserve the 4 extra `embassy-net` sockets needed for concurrent HTTP REST, WebSocket, OTA, and WebRTC UDP sockets.

### 2. Restore Stock `EmbassyNetContext` Without Shrinking C6 `.stack` or Free Heap
- **Decision**: Revert `OnceCell<Box<...>>` in `rs-matter-embassy/src/eth.rs` so `EmbassyNetContext` matches `upstream/master` (inline zero-alloc `init!()`), and either allocate `MatterStackStorage` on the C6 heap via `Box::new_uninit()` (with `HEAP_SIZE` increased by the non-buffer portion of `MatterStackStorage`) or adjust C6 `HEAP_SIZE` by the exact size of the inline `EmbassyNetContext` buffers moved from the startup heap to `.bss`.
- **Rationale**: On ESP32-C6 (no PSRAM), `.bss` and the internal heap share the same `RWDATA` SRAM pool before `.stack`. Previously, `OnceCell<Box<...>>` permanently carved ~18.5 KiB out of the 120 KiB heap at boot. Moving those fixed-lifetime buffers out of `eth.rs` while keeping `.bss + HEAP_SIZE` constant preserves both the >= 48 KiB `.stack` allocation and the exact post-boot free heap headroom without patching `rs-matter-embassy/src/eth.rs`.

### 3. Drop S31 `wifi-only` WIP from `rs-matter-embassy`
- **Decision**: Discard the uncommitted `wifi-only` changes in `rs-matter-embassy` (`Cargo.toml`, `ble.rs`, `enet.rs`, `eth.rs`, `lib.rs`, `wifi/esp.rs`, `wireless.rs`, `wireless/wifi.rs`).
- **Rationale**: ESP32-C6 and ESP32-S3 both use `trouble` + `tcp`; keeping S31 out avoids maintaining custom BLE-stub patches in `rs-matter-embassy`.

## Migration Plan

1. Revert uncommitted S31 `wifi-only` edits and drop Bucket A commits in `rs-matter-embassy`.
2. Enable `enet-extra-sockets-4` in `crates/intercom-adapter-matter/Cargo.toml` and rebalance C6 `MatterStackStorage` / `HEAP_SIZE` in `crates/intercom-firmware`.
3. Verify host tests and C6/DevKit release ELF `.stack` >= 48,240 bytes.
4. Once Bucket B PRs land upstream, switch `Cargo.toml` from local paths to upstream git/crates.io revisions and verify on hardware.
5. Archive change with `openspec archive downstream-intercom-integration`.
