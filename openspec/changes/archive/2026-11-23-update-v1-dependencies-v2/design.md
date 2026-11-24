# Design

## Context

See `proposal.md - Why`. Currently, `v1` (`intercom-matter-v1-diag`, branch `v1`) uses the pre-refactor `rs-matter-embassy` API:
- `intercom-adapter-matter/src/controller.rs` constructs `MatterStack<'static, BUMP_SIZE, EmbassyWifiNetwork>` and calls `stack.run_wifi(EmbassyWifi::new(EspWifiRadio::new(...), ...).with_hostname(hostname), ...)`.
- `intercom-firmware/src/main.rs` awaits `rs_matter_embassy::enet::wait_global_stack().await` to obtain `embassy_net::Stack<'static>` for `HttpServer`.
- `Cargo.toml` manifests on `v1` enable `dhcpv4-hostname`, `mld-v1-compat`, and `nd-learn-from-frames` on `rs-matter-embassy`, which have been removed in the upstreamed v2 `rs-matter-embassy` and replaced by `edf_matter::ipv6_compat` and firmware-owned `embassy-net` configuration.

## Goals / Non-Goals

**Goals:**
- Make `v1` compile and run cleanly against the v2 `rs-matter`, `rs-matter-stack`, `rs-matter-embassy`, and `embedded-device-foundation` (`edf-*`) repositories.
- Move `embassy-net` stack ownership, DHCPv4 hostname configuration, and `edf_matter::ipv6_compat::wrap_ipv6_compat` wrapping into `v1`'s `intercom-firmware` composition root.
- Adopt `PreexistingWireless` + `select_wireless_boot_mode` (`run_wifi_oper` when commissioned, `run_wifi` when uncommissioned) in `v1`'s `intercom-adapter-matter`.
- Preserve `v1`'s existing Door Lock + Occupancy + HTTP diagnostics/OTA functionality and host-testability.

**Non-Goals:**
- Porting v2's `platform/{c6,s3,s31}` multi-chip abstraction, WebRTC, Matter-over-TCP (`tcp`), or Audio Doorbell clusters to `v1`.
- Changing `v1`'s Matter endpoints, flash partition layout, or `intercom-core` domain logic.

## Decisions

1. **Keep `v1` single-target (ESP32-C6) without porting `platform/{s3,s31}`**
   - *Choice*: Introduce `MatterWirelessDriver` (or `PreexistingWireless` wiring) in `v1`'s `intercom-adapter-matter/src/controller.rs` and wire ESP32-C6 `WifiController`, `EnetMatterStackResources`, `EnetStack`, `EnetNetif`, `BuiltinMdns`, and `EspGattPeripheral` directly in `v1`'s `intercom-firmware/src/main.rs` (or a small `wireless.rs` helper).
   - *Rationale*: `v1` only targets ESP32-C6. Porting the full `platform/` trait hierarchy from `main` would pull in unnecessary complexity while yielding the exact same runtime behavior.
   - *Alternative considered*: Cherry-picking `platform/c6` wholesale; rejected to keep `v1` minimal and easy to diff against `v1` history.

2. **Use `EnetStack::new` (UDP-only) with `enet-extra-sockets-4` on `v1`**
   - *Choice*: Use `rs_matter_embassy::enet::EnetStack::new(enet_stack, udp_buffers)` rather than `EnetStack::new_with_tcp` and omit the `tcp` feature on `rs-matter-embassy` in `v1`.
   - *Rationale*: `v1` only exposes Door Lock and Occupancy over Matter/UDP + HTTP on port 8080; it does not use Matter-over-TCP. Avoiding `EnetMatterTcpBuffers` saves static RAM on ESP32-C6. `enet-extra-sockets-4` ensures `embassy-net` has sufficient socket slots for `HttpServer` alongside Matter.

3. **Adopt `select_wireless_boot_mode` (`run_wifi_oper` vs `run_wifi`)**
   - *Choice*: Port `WirelessBootMode` and `select_wireless_boot_mode(is_commissioned)` into `v1`'s `intercom-adapter-matter/src/commissioning.rs`, calling `stack.run_wifi_oper(...)` when commissioned and `stack.run_wifi(...)` when uncommissioned.
   - *Rationale*: Matches `v2`'s operational behavior and avoids running BLE GATT + radio coexistence when a fabric is already commissioned.

4. **Workspace `default-features = false` for `edf-gpio` and `edf-ota`, and remove `rs-matter-stack` patch**
   - *Choice*: Set `default-features = false` on `edf-gpio` and `edf-ota` in `v1`'s root `Cargo.toml`, enable `esp32c6` explicitly in `intercom-adapter-gpio/Cargo.toml` and `intercom-firmware/Cargo.toml` (under `cfg(target_arch = "riscv32")`), and remove `rs-matter-stack` from `[patch.crates-io]` in both `v1` and `main` (v2) root `Cargo.toml` files.
   - *Rationale*: Matches `main` (v2) and prevents ESP32-C6 HAL features from leaking into host `x86_64-apple-darwin` test builds; `rs-matter-stack` is deprecated and merged into `rs-matter`, so `rs-matter-embassy` no longer depends on it on either branch.

5. **Set `--baud 921600` on ESP32-C6 `espflash` runner**
   - *Choice*: Add `--baud 921600` to `[target.riscv32imac-unknown-none-elf]` `runner` in `.cargo/config.toml` on both `v1` and `main` (v2), matching the existing `--baud 921600` setting on `xtensa-esp32s3-none-elf`.
   - *Rationale*: Speeds up flashing over USB-to-UART bridges while remaining a no-op on native USB-Serial/JTAG (`/dev/cu.usbmodem*`).

## Risks / Trade-offs

- **Static buffer / RAM budget on ESP32-C6** → `v1` allocates `EnetMatterStackResources` (`StaticCell`) and `EnetMatterBuffers` (`UdpBuffers`, `BtpGattBuffers`) explicitly in `intercom-firmware` instead of inside `MatterStack`. Keeping UDP-only (`EnetStack::new`, no `matter-tcp` buffers) keeps RAM usage at or below `v1`'s previous baseline; verify with `cargo build -p intercom-firmware --release` and check `HEAP_SIZE` / `.bss` fit.
- **Sibling repo path alignment** → `v1` (`intercom-matter-v1-diag`) currently references `../embedded-device-foundation`, `../rs-matter`, and `../../../rs-matter-embassy`. Ensure the `v1` `Cargo.toml` paths resolve to the active v2 checkouts of those dependencies.

## Migration Plan

1. Apply all dependency and code updates on the `v1` branch (`/Users/vadim/personal/projects/intercom-matter-v1-diag`).
2. Verify host unit tests (`intercom-core`, `intercom-adapter-http`, `intercom-pairing-tool`, `intercom-adapter-matter` if host-testable, `intercom-firmware --lib`) pass on `x86_64-apple-darwin`.
3. Verify `mise exec -- cargo build -p intercom-firmware --release` succeeds for `riscv32imac-unknown-none-elf`.
4. Flash or OTA-push to a `v1` ESP32-C6 device and verify Wi-Fi association, DHCPv4 hostname, HTTP `/api/status`, and Matter operational reachability.
