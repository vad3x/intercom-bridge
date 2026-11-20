# Proposal: Upstream Opt-In DHCPv4 Hostname (Option 12) Support in rs-matter-embassy

## Why

By default, `rs-matter-embassy` creates its `embassy_net::Config` with `Config::dhcpv4(Default::default())` without setting a DHCPv4 client hostname (DHCP Option 12). Without Option 12, home routers and local DHCP/DNS servers list the Matter device as an unnamed/anonymous lease and fail to register a local DNS hostname (e.g. `intercom-bridge.lan`) for reaching coexisting HTTP dashboards or pushing OTA updates.

## What Changes

- In `rs-matter-embassy/Cargo.toml`, `src/enet.rs`, `src/eth.rs`, and `src/wireless/wifi.rs`:
  - Add an opt-in Cargo feature `dhcpv4-hostname = ["embassy-net?/dhcpv4-hostname"]` (disabled by default).
  - Add `create_enet_stack_with_hostname` and `create_enet_config_with_hostname` in `src/enet.rs` (gated behind `#[cfg(feature = "dhcpv4-hostname")]`), validating and passing the hostname (`<= 32` characters via `heapless::String<32>`) to `DhcpConfig.hostname`.
  - Add `pub const fn with_hostname(mut self, hostname: &'a str) -> Self` builder methods on `EmbassyEthernet`, `EmbassyWifi`, and `EnetWifiRunner` (gated behind `#[cfg(feature = "dhcpv4-hostname")]`).
- Post-merge cleanup: Rebase local `rs-matter-embassy` onto `upstream/master` to drop local commits `987c697` and `1a12a72`.

## Capabilities

### New Capabilities
- `embassy-dhcpv4-hostname`: Opt-in `dhcpv4-hostname` Cargo feature and `with_hostname` builder configuring DHCPv4 Option 12 in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/Cargo.toml`, `rs-matter-embassy/src/enet.rs`, `rs-matter-embassy/src/eth.rs`, `rs-matter-embassy/src/wireless/wifi.rs`)
- **API Surface**:
  - Opt-in `dhcpv4-hostname` Cargo feature exposing `create_enet_stack_with_hostname`, `create_enet_config_with_hostname`, and `.with_hostname(&'a str)` builders.
- **Breaking Changes**: None (feature is disabled by default; existing signatures are untouched).
