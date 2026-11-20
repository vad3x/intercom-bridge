# Design: Upstream Opt-In DHCPv4 Hostname (Option 12) Support in rs-matter-embassy

## Context

`embassy-net` supports DHCPv4 Option 12 (Client Hostname) when its `dhcpv4-hostname` feature is enabled, adding `hostname: Option<heapless::String<32>>` to `embassy_net::DhcpConfig`. `rs-matter-embassy` currently constructs `DhcpConfig::default()` inside `create_enet_config` without exposing a way for callers to supply a hostname.

## Decisions

### 1. Feature-Gated `dhcpv4-hostname` Flag
- **Decision**: Add `dhcpv4-hostname = ["embassy-net?/dhcpv4-hostname"]` to `rs-matter-embassy/Cargo.toml` outside `default` features.
- **Rationale**: Keeps `embassy-net`'s default feature set and binary footprint unchanged for callers that do not need DHCP Option 12.

### 2. Builder Methods & Stack Helper with Length Validation
- **Decision**: Expose `create_enet_config_with_hostname(mac: &[u8; 6], hostname: Option<&str>) -> Config`, `create_enet_stack_with_hostname(...)`, and `pub const fn with_hostname(mut self, hostname: &'a str) -> Self` on `EmbassyEthernet`, `EmbassyWifi`, and `EnetWifiRunner` under `#[cfg(feature = "dhcpv4-hostname")]`. Ignore empty hostnames (`None`) and warn-and-skip hostnames exceeding `heapless::String<32>` capacity rather than panicking.
- **Rationale**: Matches `rs-matter-embassy`'s builder conventions (`with_tcp_config`, `with_protocols`), works for both `EmbassyWifi` and `create_enet_stack_with_hostname` (`PreexistingWireless`), and fails gracefully on oversized strings.

## Migration Plan

1. Cut clean feature branch `feat/dhcpv4-hostname` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Port `dhcpv4-hostname` feature, helpers, builders, and unit tests to `Cargo.toml`, `src/enet.rs`, `src/eth.rs`, and `src/wireless/wifi.rs`.
3. Verify host unit tests (both with and without `--features dhcpv4-hostname`) and cross-compilation for `riscv32imac-unknown-none-elf`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Post-merge: Fetch `upstream/master` in `../rs-matter-embassy`, rebase local branch to drop commits `987c697` and `1a12a72`, confirm zero diff for `dhcpv4-hostname` against `upstream/master`, and archive change with `openspec archive upstream-embassy-dhcpv4-hostname`.
