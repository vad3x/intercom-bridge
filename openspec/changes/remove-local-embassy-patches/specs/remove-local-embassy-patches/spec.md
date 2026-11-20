# Spec Delta: Application-Owned Embassy Stack with PreexistingWireless & Zero Local Patches

## Purpose

Eliminates local `rs-matter-embassy` patches (`0782d36` `global_stack`, custom socket constants, `OnceCell<Box<...>>` in `eth.rs`, and uncommitted S31 `wifi-only` edits) by owning `embassy_net::Stack` in `intercom-firmware` via `PreexistingWireless` and using upstream `enet-extra-sockets-4`.

## ADDED Requirements

### Requirement: Application-Owned `embassy_net::Stack` via `PreexistingWireless`
`intercom-firmware` SHALL initialize the `embassy_net::Stack` and `Runner` directly via `create_enet_stack` (or `create_enet_stack_with_hostname`) and pass `PreexistingWireless` (`EnetStack`, `EnetNetif`, `EspWifiController`, `BuiltinMdns`, `EspGattPeripheral`) to Matter while passing the same `embassy_net::Stack` directly to `HttpServer` and `WebRtcMediaEngine` without calling `global_stack()` or `wait_global_stack()`.

#### Scenario: Concurrent Matter, HTTP, and WebRTC over shared `embassy_net::Stack`
- **WHEN** `intercom-firmware` boots on ESP32-C6 or ESP32-S3
- **THEN** `create_enet_stack` creates a single `embassy_net::Stack` shared across `PreexistingWireless`, `HttpServer`, and `WebRtcMediaEngine`
- **AND** `rs-matter-embassy/src/enet.rs` contains no `global_stack` or `wait_global_stack` functions

### Requirement: Upstream Extra Socket Feature & Stock `EmbassyNetContext`
`intercom-adapter-matter` SHALL enable `rs-matter-embassy/enet-extra-sockets-4`, and `rs-matter-embassy/src/eth.rs` SHALL match `upstream/master` without `OnceCell<Box<...>>` patches while `intercom-firmware` preserves >= `48,240` bytes of linked `.stack` on ESP32-C6.

#### Scenario: ESP32-C6 release build with stock `eth.rs` and `enet-extra-sockets-4`
- **WHEN** `intercom-firmware` is built for ESP32-C6 (`--release` and `--features devkit`) against stock `rs-matter-embassy/src/eth.rs`
- **THEN** the linked ELF reserves at least `48,240` bytes for `.stack`

### Requirement: Post-Merge Zero Local `rs-matter-embassy` Patches
Once `upstream-embassy-wifi-scan-config`, `upstream-embassy-dhcpv4-hostname`, and `upstream-embassy-ipv6-compat` merge into `sysgrok/rs-matter-embassy`, `../rs-matter-embassy` SHALL have zero local commits ahead of `upstream/master` and `intercom-matter` SHALL consume the upstream revision directly.

#### Scenario: Verifying zero local patches after upstream PRs merge
- **WHEN** all three upstream PRs are merged and `../rs-matter-embassy` is synced with `upstream/master`
- **THEN** `git -C ../rs-matter-embassy diff --exit-code upstream/master` succeeds with zero differences
