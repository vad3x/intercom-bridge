# Design: Eliminate Local rs-matter-embassy Patches via PreexistingWireless & Upstream Features

## Context

`intercom-firmware` runs three coexisting network consumers on a single ESP32 Wi-Fi interface:
1. Matter over Wi-Fi (UDP 5540, TCP 5540, mDNS 5353, BLE commissioning)
2. `HttpServer` (TCP port 8080 for Web Dashboard, REST API, WebSocket, and OTA)
3. `WebRtcMediaEngine` (UDP port 50000 for ICE, STUN, DTLS-SRTP audio)

Previously, `intercom-firmware` used `EmbassyWifi::new(EspWifiRadio::new(wifi, bt), ...)`, which created `embassy_net::Stack<'a>` privately inside `EnetWifiRunner::run` / `run_coex`. That forced `rs-matter-embassy` to carry local patches for `global_stack()` (`0782d36`) and `dhcpv4-hostname` (`987c697`, `1a12a72`).

## Decisions

### 1. Application-Owned `embassy_net::Stack` with `PreexistingWireless` & `Stack::set_config_v4`
- **Decision**: Construct `WifiController` and call `rs_matter_embassy::enet::create_enet_stack` in `intercom-firmware`, producing `(stack, mut runner)`. Configure DHCPv4 Option 12 hostname directly on `stack` via `stack.set_config_v4(embassy_net::ConfigV4::Dhcp(dhcp_config))` (with `embassy-net/dhcpv4-hostname` enabled in `intercom-firmware`). Pass `stack` directly to `HttpServer`, `WebRtcMediaEngine`, and `PreexistingWireless::new(EnetStack::new_with_tcp(stack, ...), EnetNetif::new(stack, InterfaceTypeEnum::WiFi), EspWifiController::new(controller), BuiltinMdns, EspGattPeripheral::new(bt, ...))`, and run `runner.run()` alongside them. Cache the IPv4 address from `stack.config_v4()` in an atomic for `FirmwareHardwareInfo::ip_address()`.
- **Rationale**: Uses 100% public upstream APIs from `rs-matter-stack`, `rs-matter-embassy`, and `embassy-net`, keeps `stack` alive continuously across commissioning and operational transitions, eliminates `unsafe` `Stack<'static>` transmutation, and allows dropping commits `0782d36`, `987c697`, and `1a12a72` from `rs-matter-embassy`.

### 2. Upstream `enet-extra-sockets-4` & Stock `eth.rs`
- **Decision**: Enable `"enet-extra-sockets-4"` (and remove `"dhcpv4-hostname"` and `"wifi-only"`) in `crates/intercom-adapter-matter/Cargo.toml` and drop commits `e2495cd`, `11d40d6`, and `1665e20` (plus the uncommitted S31 `wifi-only` WIP) from `../rs-matter-embassy`. Rebalance C6's `HEAP_SIZE` / buffer placement in `crates/intercom-firmware` so `.stack` retains >= `48,240` bytes (`48 KiB`).
- **Rationale**: Restores `rs-matter-embassy/src/eth.rs` and the socket pool logic in `src/enet.rs` to exact `upstream/master` parity.

## Migration Plan

1. Revert uncommitted S31 `wifi-only` edits in `../rs-matter-embassy`.
2. Switch `intercom-firmware` and `intercom-adapter-matter` to `PreexistingWireless` + `stack.set_config_v4(...)` + `enet-extra-sockets-4` and rebalance C6 `HEAP_SIZE`.
3. Drop local commits `0782d36`, `987c697`, `1a12a72`, `e2495cd`, `11d40d6`, and `1665e20` from `../rs-matter-embassy` and verify `src/eth.rs` matches `upstream/master`.
4. After the remaining upstream PR(s) merge, remove the local `rs-matter-embassy` path patch from `Cargo.toml` and archive with `openspec archive remove-local-embassy-patches`.
