# Proposal: Eliminate Local rs-matter-embassy Patches via PreexistingWireless & Upstream Features

## Why

The `intercom-matter` workspace currently carries local patches in `../rs-matter-embassy` (`0782d36` `global_stack`, `987c697` / `1a12a72` `dhcpv4-hostname`, `e2495cd` / `11d40d6` custom socket counts, `11d40d6` / `1665e20` `OnceCell<Box<...>>` buffer pools, and uncommitted S31 `wifi-only` edits) that do not need upstream PRs because `rs-matter-stack`, `rs-matter-embassy`, and `embassy-net` already provide public APIs (`PreexistingWireless`, `create_enet_stack`, `Stack::set_config_v4`, `enet-extra-sockets-4`) to accomplish the same goals directly in `intercom-firmware`.

## What Changes

- **Replace `0782d36` (`global_stack` / `wait_global_stack`) and `987c697` / `1a12a72` (`dhcpv4-hostname`) with `PreexistingWireless` & `Stack::set_config_v4`**:
  - Create the ESP Wi-Fi controller and call `rs_matter_embassy::enet::create_enet_stack` directly in `intercom-firmware`, owning `stack: embassy_net::Stack` in the application.
  - Configure DHCPv4 Option 12 hostname directly on `stack` via `stack.set_config_v4(embassy_net::ConfigV4::Dhcp(dhcp_config))` (with `embassy-net/dhcpv4-hostname` enabled in `intercom-firmware/Cargo.toml`), eliminating the need for `dhcpv4-hostname` in `rs-matter-embassy`.
  - Pass `stack` directly to `HttpServer`, `WebRtcMediaEngine`, and `PreexistingWireless::new(EnetStack::new_with_tcp(stack, ...), EnetNetif::new(stack, InterfaceTypeEnum::WiFi), EspWifiController::new(controller), BuiltinMdns, EspGattPeripheral::new(bt, ...))`, and cache the active IPv4 address in an atomic for `FirmwareHardwareInfo::ip_address()`.
  - Drop commits `0782d36`, `987c697`, and `1a12a72` from `../rs-matter-embassy`.
- **Replace custom extra-socket constants (`e2495cd`, `11d40d6`) with `enet-extra-sockets-4`**:
  - Enable `"enet-extra-sockets-4"` in `crates/intercom-adapter-matter/Cargo.toml` and restore upstream `ENET_EXTRA_SOCKET_SET` in `../rs-matter-embassy/rs-matter-embassy/src/enet.rs`.
- **Replace `OnceCell<Box<...>>` in `eth.rs` (`11d40d6`, `1665e20`) and revert S31 `wifi-only` WIP**:
  - Discard the uncommitted S31 `wifi-only` working-tree edits in `../rs-matter-embassy`.
  - Restore `../rs-matter-embassy/rs-matter-embassy/src/eth.rs` to `upstream/master` and rebalance C6 `HEAP_SIZE` / network buffer placement in `crates/intercom-firmware` so `.stack` preserves >= `48,240` bytes (`48 KiB`).
- **Post-merge final patch removal**:
  - Once the remaining upstream `rs-matter-embassy` PR(s) merge, point `intercom-matter` at upstream `rs-matter-embassy` with zero local patches.

## Capabilities

### New Capabilities
- `remove-local-embassy-patches`: Application-owned `embassy_net::Stack` with `PreexistingWireless`, direct `Stack::set_config_v4` DHCPv4 hostname configuration, and zero local `rs-matter-embassy` patches.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `intercom-matter` (`Cargo.toml`, `crates/intercom-adapter-matter/`, `crates/intercom-firmware/`)
  - `rs-matter-embassy` (reverting uncommitted S31 `wifi-only` WIP and dropping local commits `0782d36`, `987c697`, `1a12a72`, `e2495cd`, `11d40d6`, `1665e20`)
- **API Surface**: Internal wireless stack wiring between `intercom-firmware` and `intercom-adapter-matter`.
- **Breaking Changes**: None.
