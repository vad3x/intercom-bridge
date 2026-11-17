# Proposal: Upstream Opt-in `dhcpv4-hostname` Feature in rs-matter-embassy

## Why

Upstream `sysgrok/rs-matter-embassy` creates its `embassy-net` `DhcpConfig` with defaults and does not expose DHCP Option 12 (hostname). Devices on consumer routers and local DNS servers then appear as unnamed/MAC-only DHCP leases. Adding an opt-in `dhcpv4-hostname` feature lets applications configure a DHCPv4 hostname cleanly without changing default builds.

## What Changes

- In `rs-matter-embassy/Cargo.toml`:
  - Add opt-in feature `dhcpv4-hostname = ["embassy-net?/dhcpv4-hostname"]`.
- In `rs-matter-embassy/src/enet.rs`:
  - Add `create_enet_stack_with_hostname(..., hostname: Option<&str>)` (feature-gated under `dhcpv4-hostname`) that populates `DhcpConfig::hostname` via `heapless::String`, plus unit tests.
- In `rs-matter-embassy/src/eth.rs` and `rs-matter-embassy/src/wireless/wifi.rs`:
  - Add `with_hostname(self, hostname: &'a str)` on `EmbassyEthernet` and `EmbassyWifi` (feature-gated under `dhcpv4-hostname`) and wire it into stack creation.

## Capabilities

### New Capabilities
- `embassy-dhcpv4-hostname`: Opt-in DHCPv4 Option 12 hostname configuration on `EmbassyEthernet` and `EmbassyWifi`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`Cargo.toml`, `src/enet.rs`, `src/eth.rs`, `src/wireless/wifi.rs`)
- **API Surface**:
  - Additive `#[cfg(feature = "dhcpv4-hostname")]` `with_hostname` builder method and `create_enet_stack_with_hostname`.
- **Breaking Changes**: None (opt-in feature, off by default).
