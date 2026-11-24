# Proposal

## Why

The `v1` branch of `intercom-matter` (`intercom-matter-v1-diag`) still depends on the legacy `rs-matter-embassy`-owned network stack and removed Cargo feature flags (`dhcpv4-hostname`, `mld-v1-compat`, `nd-learn-from-frames`), making it operationally and dependency-incompatible with the updated `rs-matter`, `rs-matter-stack`, `rs-matter-embassy`, and `embedded-device-foundation` (`edf-*`) crates used on `main` (v2). Updating `v1` to adopt the v2 firmware-owned network stack and `edf-matter` IPv6 compatibility shims keeps `v1` buildable and operationally aligned with `main` without pulling in v2's WebRTC, audio, or multi-target (S3/S31) changes.

## What Changes

- **Firmware-owned Embassy network stack on `v1`**: Move `embassy-net` stack creation, DHCPv4 Option 12 hostname configuration (`embassy-net` `dhcpv4-hostname` feature + `DhcpConfig::hostname`), and `embassy_net::Runner` execution into `intercom-firmware` using `rs_matter_embassy::enet::create_enet_stack`, `EnetStack`, `EnetNetif`, and `rs_matter_embassy::stack::wireless::PreexistingWireless`.
- **Replace `rs-matter-embassy` IPv6/DHCP feature flags with `edf-matter` shims**: Remove `dhcpv4-hostname`, `mld-v1-compat`, and `nd-learn-from-frames` from `intercom-adapter-matter` and `intercom-firmware` `Cargo.toml` manifests; wrap `esp_radio::wifi::Interface::station()` with `edf_matter::ipv6_compat::wrap_ipv6_compat(...)` in `intercom-firmware` before passing it to `create_enet_stack`.
- **Replace `EmbassyWifi` and `wait_global_stack()`**: Refactor `intercom-adapter-matter`'s `MatterController` on `v1` to accept a `MatterWirelessDriver` (or pre-initialized `WifiController` + `embassy_net::Stack` + `EnetMatterBuffers`) and run `PreexistingWireless` via `run_wifi` / `run_wifi_oper` (Wi-Fi-only when commissioned, BLE+Wi-Fi coex when uncommissioned), while passing the firmware-owned `embassy_net::Stack` directly to `HttpServer` instead of awaiting `rs_matter_embassy::enet::wait_global_stack()`.
- **Align `Cargo.toml` dependency features and remove `rs-matter-stack`**: Update `v1` workspace and crate `Cargo.toml` files so `edf-gpio` and `edf-ota` use `default-features = false` with explicit `esp32c6` features in `intercom-adapter-gpio` / `intercom-firmware`, remove the deprecated `rs-matter-stack` entry from `[patch.crates-io]` on both `v1` and `main` (v2) (now part of `rs-matter`), and align `rs-matter-embassy` features (`esp`, `trouble`, `embassy-net`, `enet-extra-sockets-4`, `log`, `rustcrypto`, `kv-blob-store-8192`, `max-im-buffers-4`) with `main`.
- **Faster ESP32-C6 flashing**: Add `--baud 921600` to the `[target.riscv32imac-unknown-none-elf]` `espflash` runner in `.cargo/config.toml` on both `v1` and `main` (v2).

## Capabilities

### New Capabilities
- `v1-firmware-network-stack`: Firmware-owned `embassy-net` stack, DHCPv4 hostname setup, and `PreexistingWireless` Matter runner wiring on the `v1` ESP32-C6 branch, preserving `v1`'s Door Lock + Occupancy + HTTP diagnostics scope.

### Modified Capabilities
- `embassy-ipv6-compat`: Extend adoption of `edf_matter::ipv6_compat::wrap_ipv6_compat` to the `v1` firmware network initialization path (previously specified only for `main` / v2 `init_esp_network`).

## Impact

- **Target branches / checkouts**: `v1` (`/Users/vadim/personal/projects/intercom-matter-v1-diag`, branch `v1`) and `main` (v2) root `Cargo.toml` for removing the stale `rs-matter-stack` patch entry.
- **Affected files on `v1`**:
  - `.cargo/config.toml` on `v1` and `main` (add `--baud 921600` to `riscv32imac-unknown-none-elf` `espflash` runner)
  - `Cargo.toml` (workspace dependencies: `edf-gpio`, `edf-ota`, `embassy-net`; remove deprecated `rs-matter-stack` from `[patch.crates-io]`)
  - `crates/intercom-adapter-gpio/Cargo.toml` (`edf-gpio/esp32c6`)
  - `crates/intercom-adapter-matter/Cargo.toml` (remove `dhcpv4-hostname`, `mld-v1-compat`, `nd-learn-from-frames`; align `rs-matter-embassy` features)
  - `crates/intercom-adapter-matter/src/controller.rs` (replace `EmbassyWifi` / `EmbassyWifiNetwork` with `MatterWirelessDriver` + `WifiNetworks` + `select_wireless_boot_mode`)
  - `crates/intercom-adapter-matter/src/commissioning.rs` (add `WirelessBootMode` / `select_wireless_boot_mode` if absent on `v1`)
  - `crates/intercom-firmware/Cargo.toml` (add `embassy-net` with `dhcpv4` + `dhcpv4-hostname`, `edf-ota/esp32c6`, `edf-gpio/esp32c6`; align `rs-matter-embassy` features)
  - `crates/intercom-firmware/src/main.rs` (initialize `WifiController`, `wrap_ipv6_compat`, `create_enet_stack`, `DhcpConfig::hostname`, spawn/join `net.run()`, pass `enet_stack` directly to `HttpServer`)
- **Non-goals**: No WebRTC, audio pipeline, or ESP32-S3 / ESP32-S31 multi-target refactoring on `v1`.
