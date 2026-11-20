# Spec Delta: Downstream Intercom Bridge Integration & Local Patch Elimination

## Purpose

Eliminates unneeded local patches in `rs-matter-embassy` (Bucket A) and integrates upstream releases of `project-chip/rs-matter` and `sysgrok/rs-matter-embassy` into the `intercom-matter` workspace (Bucket B) while preserving ESP32-C6 and ESP32-S3 memory and stack budgets.

## ADDED Requirements

### Requirement: Upstream Extra Socket Feature Usage
`intercom-adapter-matter` SHALL configure application socket headroom using upstream `rs-matter-embassy`'s `enet-extra-sockets-4` feature flag rather than custom `ENET_EXTRA_TCP_SOCKETS` / `ENET_EXTRA_UDP_SOCKETS` source patches.

#### Scenario: Enabling extra sockets for HTTP, WebSocket, OTA, and WebRTC
- **WHEN** `intercom-adapter-matter` is compiled for ESP32-C6 or ESP32-S3
- **THEN** `rs-matter-embassy/enet-extra-sockets-4` is enabled in `Cargo.toml`
- **AND** `rs-matter-embassy/src/enet.rs` uses upstream `ENET_EXTRA_SOCKET_SET` without local socket-count overrides

### Requirement: Stock Zero-Alloc `EmbassyNetContext` with Preserved C6 Stack Budget
`intercom-firmware` SHALL use upstream `rs-matter-embassy/src/eth.rs`'s inline `EmbassyNetContext` buffer layout without `OnceCell<Box<...>>` patches while maintaining at least `48,240` bytes (`48 KiB`) of linked `.stack` and >= `32,768` bytes minimum free heap during sustained WebRTC Listen on ESP32-C6.

#### Scenario: ESP32-C6 and DevKit release link with stock `eth.rs`
- **WHEN** `rs-matter-embassy/src/eth.rs` is restored to `upstream/master` and `intercom-firmware` is built in `--release` (and `--features devkit`)
- **THEN** the linked ELF reserves at least `48,240` bytes for `.stack`

### Requirement: Upstream Dependency Alignment
Once Bucket B PRs merge upstream, the workspace `Cargo.toml` and `crates/intercom-adapter-matter/Cargo.toml` SHALL consume upstream release tags or branches of `rs-matter` and `rs-matter-embassy` with zero local directory path patches.

#### Scenario: Building without local repository patches
- **WHEN** local path overrides are removed from `Cargo.toml` and `crates/intercom-adapter-matter/Cargo.toml`
- **THEN** Cargo resolves all dependencies cleanly from upstream git/crates.io and all host tests pass
