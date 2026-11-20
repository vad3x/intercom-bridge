# Proposal: Eliminate Local rs-matter-embassy Patches via PreexistingWireless & Upstream Features

## Why

The `intercom-matter` workspace currently carries local patches in `../rs-matter-embassy` (`0782d36` `global_stack`, `e2495cd` / `11d40d6` custom socket counts, `11d40d6` / `1665e20` `OnceCell<Box<...>>` buffer pools, and uncommitted S31 `wifi-only` edits) that do not need upstream PRs because `rs-matter-stack` and `rs-matter-embassy` already provide public APIs (`PreexistingWireless`, `create_enet_stack`, `enet-extra-sockets-4`) to accomplish the same goals more safely.

## What Changes

- **Replace `0782d36` (`global_stack` / `wait_global_stack`) with `PreexistingWireless`**:
  - Create the ESP Wi-Fi controller and call `rs_matter_embassy::enet::create_enet_stack` (or `create_enet_stack_with_hostname`) directly in `intercom-firmware`, owning `stack: embassy_net::Stack` in the application.
  - Pass `stack` directly to `HttpServer`, `WebRtcMediaEngine`, and `PreexistingWireless::new(EnetStack::new_with_tcp(stack, ...), EnetNetif::new(stack, InterfaceTypeEnum::WiFi), EspWifiController::new(controller), BuiltinMdns, EspGattPeripheral::new(bt, ...))`, and cache the active IPv4 address in an atomic for `FirmwareHardwareInfo::ip_address()`.
  - Drop commit `0782d36` from `../rs-matter-embassy`.
- **Replace custom extra-socket constants (`e2495cd`, `11d40d6`) with `enet-extra-sockets-4`**:
  - Enable `"enet-extra-sockets-4"` in `crates/intercom-adapter-matter/Cargo.toml` and restore upstream `ENET_EXTRA_SOCKET_SET` in `../rs-matter-embassy/rs-matter-embassy/src/enet.rs`.
- **Replace `OnceCell<Box<...>>` in `eth.rs` (`11d40d6`, `1665e20`) and revert S31 `wifi-only` WIP**:
  - Discard the uncommitted S31 `wifi-only` working-tree edits in `../rs-matter-embassy`.
  - Restore `../rs-matter-embassy/rs-matter-embassy/src/eth.rs` to `upstream/master` and rebalance C6 `HEAP_SIZE` / network buffer placement in `crates/intercom-firmware` so `.stack` preserves >= `48,240` bytes (`48 KiB`).
- **Post-merge final patch removal**:
  - Once `upstream-embassy-wifi-scan-config`, `upstream-embassy-dhcpv4-hostname`, and `upstream-embassy-ipv6-compat` merge, point `intercom-matter` at upstream `rs-matter-embassy` with zero local patches.

## Capabilities

### New Capabilities
- `remove-local-embassy-patches`: Application-owned `embassy_net::Stack` with `PreexistingWireless` and zero local `rs-matter-embassy` patches.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `intercom-matter` (`Cargo.toml`, `crates/intercom-adapter-matter/`, `crates/intercom-firmware/`)
  - `rs-matter-embassy` (reverting uncommitted S31 `wifi-only` WIP and dropping local commits `0782d36`, `e2495cd`, `11d40d6`, `1665e20`)
- **API Surface**: Internal wireless stack wiring between `intercom-firmware` and `intercom-adapter-matter`.
- **Breaking Changes**: None.
