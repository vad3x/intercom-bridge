# Spec Delta

## Purpose

Defines the firmware-owned Embassy network stack, DHCPv4 hostname configuration, and wireless Matter boot-mode selection for the `v1` ESP32-C6 Intercom Bridge firmware, aligning its dependency architecture with `main` (v2).

## ADDED Requirements

### Requirement: Firmware-Owned Embassy Network Stack on v1
The `v1` `intercom-firmware` crate SHALL initialize and own the `embassy_net::Stack` and `embassy_net::Runner` via `rs_matter_embassy::enet::create_enet_stack`, running the network task concurrently with the Matter and HTTP tasks rather than awaiting a global stack inside `rs-matter-embassy`.

#### Scenario: HTTP server receives firmware-owned network stack
- **WHEN** `v1` `intercom-firmware` starts the HTTP diagnostics and OTA server
- **THEN** it passes the `embassy_net::Stack<'static>` created in `intercom-firmware` directly to `HttpServer::new` without calling `rs_matter_embassy::enet::wait_global_stack()`

### Requirement: Firmware-Configured DHCPv4 Hostname on v1
The `v1` `intercom-firmware` crate SHALL configure the DHCPv4 Option 12 hostname (`intercom-bridge-<mac-suffix>` or custom override) on `embassy_net::ConfigV4::Dhcp` using `embassy-net`'s `dhcpv4-hostname` feature, without relying on any `dhcpv4-hostname` feature in `rs-matter-embassy`.

#### Scenario: DHCPv4 hostname is set on the firmware network stack
- **WHEN** `v1` `intercom-firmware` initializes IPv4 networking on the ESP32-C6 station interface
- **THEN** `stack.set_config_v4(ConfigV4::Dhcp(...))` sets `DhcpConfig::hostname` to the device hostname derived from the hardware MAC address

### Requirement: Preexisting Wireless Matter Runner and Boot-Mode Selection on v1
`intercom-adapter-matter` on `v1` SHALL drive `rs_matter_embassy::stack::wireless::PreexistingWireless` (composed of `EnetStack`, `EnetNetif`, `EspWifiController`, `BuiltinMdns`, and `EspGattPeripheral`) and select `run_wifi_oper` when a commissioned fabric is present or `run_wifi` (BLE+Wi-Fi coex) when uncommissioned.

#### Scenario: Commissioned v1 device boots in Wi-Fi-only operational mode
- **WHEN** `v1` `intercom-firmware` boots with an active commissioned controller fabric in Matter KV storage
- **THEN** `MatterController` launches the Matter stack via `run_wifi_oper` on `PreexistingWireless`

#### Scenario: Uncommissioned v1 device boots in concurrent BLE+Wi-Fi commissioning mode
- **WHEN** `v1` `intercom-firmware` boots with no active commissioned controller fabric
- **THEN** `MatterController` launches the Matter stack via `run_wifi` on `PreexistingWireless`
