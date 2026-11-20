# Spec Delta: Application-Owned Embassy Stack with PreexistingWireless & Zero Local Patches

## Purpose

Eliminates local `rs-matter-embassy` patches (`0782d36` `global_stack`, `987c697` / `1a12a72` `dhcpv4-hostname`, custom socket constants, `OnceCell<Box<...>>` in `eth.rs`, and uncommitted S31 `wifi-only` edits) by owning `embassy_net::Stack` in `intercom-firmware` via `PreexistingWireless`, configuring DHCPv4 Option 12 via `Stack::set_config_v4`, and using upstream `enet-extra-sockets-4`.

## ADDED Requirements

### Requirement: Application-Owned `embassy_net::Stack` via `PreexistingWireless`
`intercom-firmware` SHALL initialize `embassy_net::Stack` directly via `create_enet_stack` and share that `Stack` across `PreexistingWireless`, `HttpServer`, and `WebRtcMediaEngine` without `global_stack()` or `wait_global_stack()`.

#### Scenario: Concurrent Matter, HTTP, and WebRTC over shared `embassy_net::Stack`
- **WHEN** `intercom-firmware` boots on ESP32-C6 or ESP32-S3
- **THEN** `create_enet_stack` creates a single `embassy_net::Stack` shared across `PreexistingWireless`, `HttpServer`, and `WebRtcMediaEngine`
- **AND** `rs-matter-embassy` contains no `global_stack` or `wait_global_stack` patches

### Requirement: Firmware-Side DHCPv4 Hostname Configuration (Option 12)
`intercom-firmware` SHALL enable `embassy-net/dhcpv4-hostname` and configure the DHCPv4 Option 12 client hostname on its `embassy_net::Stack` via `stack.set_config_v4(ConfigV4::Dhcp(...))` for non-empty hostnames up to 32 bytes, ignoring empty or `>32`-byte hostnames without panicking or patching `rs-matter-embassy`.

#### Scenario: Valid hostname up to 32 bytes
- **WHEN** `intercom-firmware` configures DHCPv4 with hostname `"intercom-bridge"`
- **THEN** `DhcpConfig.hostname` is set to `"intercom-bridge"` and applied via `stack.set_config_v4`

#### Scenario: Empty or oversized hostname (`>32` bytes)
- **WHEN** the DHCPv4 config helper is called with `""` or a hostname longer than 32 bytes
- **THEN** `DhcpConfig.hostname` remains `None` without panicking

### Requirement: Upstream Extra Socket Feature & Stock `EmbassyNetContext`
`intercom-adapter-matter` SHALL enable `rs-matter-embassy/enet-extra-sockets-4`, and `rs-matter-embassy/src/eth.rs` SHALL match `upstream/master` without `OnceCell<Box<...>>` patches while `intercom-firmware` preserves >= `48,240` bytes of linked `.stack` on ESP32-C6.

#### Scenario: ESP32-C6 release build with stock `eth.rs` and `enet-extra-sockets-4`
- **WHEN** `intercom-firmware` is built for ESP32-C6 (`--release` and `--features devkit`) against stock `rs-matter-embassy/src/eth.rs`
- **THEN** the linked ELF reserves at least `48,240` bytes for `.stack`

### Requirement: Post-Merge Zero Local `rs-matter-embassy` Patches
Once the remaining upstream `rs-matter-embassy` PR(s) merge into `sysgrok/rs-matter-embassy`, `../rs-matter-embassy` SHALL have zero local commits ahead of `upstream/master` and `intercom-matter` SHALL consume the upstream revision directly.

#### Scenario: Verifying zero local patches after upstream PRs merge
- **WHEN** the upstream PR(s) are merged and `../rs-matter-embassy` is synced with `upstream/master`
- **THEN** `git -C ../rs-matter-embassy diff --exit-code upstream/master` succeeds with zero differences
