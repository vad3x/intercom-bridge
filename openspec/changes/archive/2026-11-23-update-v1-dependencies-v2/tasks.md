# Tasks

## 1. Align Workspace and Crate Dependency Manifests on `v1`

- [x] 1.1 Update `Cargo.toml` on `v1` to set `default-features = false` on `edf-gpio` and `edf-ota`, remove the deprecated `rs-matter-stack` entry from `[patch.crates-io]` (now part of `rs-matter`), and update `crates/intercom-adapter-gpio/Cargo.toml` to enable `edf-gpio/esp32c6`, verifying workspace metadata resolves cleanly with: `mise exec -- cargo metadata --no-deps --format-version 1 > /dev/null`
- [x] 1.2 Remove the deprecated `rs-matter-stack` entry from `[patch.crates-io]` in `main` (v2) `Cargo.toml`, verifying workspace metadata resolves cleanly with: `mise exec -- cargo metadata --no-deps --format-version 1 > /dev/null`
- [x] 1.3 Update `crates/intercom-adapter-matter/Cargo.toml` on `v1` to remove `dhcpv4-hostname`, `mld-v1-compat`, and `nd-learn-from-frames` from `rs-matter-embassy` features and enable `enet-extra-sockets-4` and `max-im-buffers-4` (keeping `esp`, `trouble`, `embassy-net`, `log`, `rustcrypto`, `kv-blob-store-8192`), verifying host unit tests still compile with: `mise exec -- cargo test -p intercom-core --target x86_64-apple-darwin`
- [x] 1.4 Update `crates/intercom-firmware/Cargo.toml` on `v1` to add `embassy-net = { workspace = true, features = ["dhcpv4", "dhcpv4-hostname"] }`, enable `esp32c6` on `edf-ota` and `edf-gpio` under `[target.'cfg(target_arch = "riscv32")'.dependencies]`, and remove `mld-v1-compat` and `nd-learn-from-frames` from `rs-matter-embassy` features (enabling `enet-extra-sockets-4` and `max-im-buffers-4`), verifying manifest parsing with: `mise exec -- cargo metadata --no-deps --format-version 1 > /dev/null`
- [x] 1.5 Add `--baud 921600` to `[target.riscv32imac-unknown-none-elf]` `runner` in `.cargo/config.toml` on both `v1` and `main` (v2), verifying Cargo config parsing with: `mise exec -- cargo metadata --no-deps --format-version 1 > /dev/null`

## 2. Wireless Boot-Mode Selection and `MatterWirelessDriver` in `intercom-adapter-matter`

- [x] 2.1 [RED] Write failing unit tests in `crates/intercom-core/src/domain/ecosystem.rs` asserting `select_wireless_boot_mode(true)` returns `WirelessBootMode::OperationalWifiOnly` and `select_wireless_boot_mode(false)` returns `WirelessBootMode::BleWifiCoex`, verifying failure with: `mise exec -- cargo test -p intercom-core --lib ecosystem --target x86_64-apple-darwin`
- [x] 2.2 [GREEN] Implement `WirelessBootMode` and `select_wireless_boot_mode(is_commissioned: bool)` in `crates/intercom-core/src/domain/ecosystem.rs` and re-export them in `crates/intercom-adapter-matter/src/commissioning.rs`, verifying tests pass with: `mise exec -- cargo test -p intercom-core --lib ecosystem --target x86_64-apple-darwin`
- [x] 2.3 Refactor `crates/intercom-adapter-matter/src/controller.rs` on `v1` to replace `EmbassyWifi` / `EmbassyWifiNetwork` / `MatterPeripherals` with `MatterWirelessDriver` (matching `main`'s `init_entropy` and `run_stack` trait) and `MatterStack<0, WifiNetworks>`, verifying `intercom-adapter-matter` compiles for ESP32-C6 with: `mise exec -- cargo check -p intercom-adapter-matter`

## 3. Firmware-Owned Network Stack, DHCPv4 Hostname, and IPv6 Compat Shim in `intercom-firmware`

- [x] 3.1 [RED] Write failing unit tests in `crates/intercom-firmware/src/network.rs` (exposed via `crates/intercom-firmware/src/lib.rs`) asserting `dhcp_config_with_hostname("intercom-bridge-7844")` populates `DhcpConfig::hostname` and `dhcp_config_with_hostname("")` leaves `hostname` as `None`, verifying failure with: `mise exec -- cargo test -p intercom-firmware --lib network --target x86_64-apple-darwin`
- [x] 3.2 [GREEN] Implement `dhcp_config_with_hostname` in `crates/intercom-firmware/src/network.rs` and expose `pub mod network;` in `crates/intercom-firmware/src/lib.rs`, verifying tests pass with: `mise exec -- cargo test -p intercom-firmware --lib network --target x86_64-apple-darwin`
- [x] 3.3 Implement ESP32-C6 `BoardWireless` and network initialization in `crates/intercom-firmware/src/main.rs` (or `crates/intercom-firmware/src/network.rs`) wrapping `esp_radio::wifi::Interface::station()` with `edf_matter::ipv6_compat::wrap_ipv6_compat`, creating `(stack, runner)` via `rs_matter_embassy::enet::create_enet_stack`, applying `dhcp_config_with_hostname`, constructing `PreexistingWireless::new(EnetStack::new(...), EnetNetif::new(...), EspWifiController::new(...), BuiltinMdns::new(), EspGattPeripheral::new(...))`, dispatching `run_wifi_oper` / `run_wifi` via `select_wireless_boot_mode`, and passing `enet_stack` directly to `HttpServer::new` (removing `wait_global_stack()`), verifying compilation with: `mise exec -- cargo build -p intercom-firmware --release`

## 4. Integration Verification

- [x] 4.1 Run all `v1` host-capable tests on macOS and confirm clean pass: `mise exec -- cargo test -p intercom-core -p intercom-adapter-http -p intercom-pairing-tool -p intercom-firmware --lib --target x86_64-apple-darwin`
- [x] 4.2 Build the `v1` release firmware ELF for ESP32-C6 and confirm clean exit code `0`: `mise exec -- cargo build -p intercom-firmware --release`
