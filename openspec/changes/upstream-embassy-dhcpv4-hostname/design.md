# Design: Upstream Opt-in `dhcpv4-hostname` Feature in rs-matter-embassy

## Context

`embassy-net` supports DHCP Option 12 hostname injection via its `dhcpv4-hostname` feature and `DhcpConfig::hostname`. `rs-matter-embassy` constructs `DhcpConfig` inside `create_enet_stack` (`src/enet.rs`), so applications cannot currently set a DHCPv4 hostname without modifying `rs-matter-embassy`.

## Decisions

### 1. Opt-in Cargo Feature
- **Decision**: Gate all new fields, builder methods, and `create_enet_stack_with_hostname` behind `#[cfg(feature = "dhcpv4-hostname")]` forwarding `embassy-net?/dhcpv4-hostname`.
- **Rationale**: Zero flash/RAM/dependency impact when disabled; `const fn with_hostname` preserves `static`/const construction patterns.

### 2. Bounded `heapless::String` Conversion
- **Decision**: Truncate or convert the `&str` hostname into `embassy-net`'s `DhcpConfig::hostname` buffer inside `create_enet_stack_with_hostname` and cover edge cases with host unit tests.
- **Rationale**: Avoids fallible builder errors on `with_hostname` while safely handling hostnames longer than DHCP Option 12 / `heapless::String` capacity.

## Migration Plan

1. Cut clean feature branch `feat/dhcpv4-hostname` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Cherry-pick/port `d8c6186` + `with_hostname` `const` fix from `db2b483`.
3. Run `cargo fmt`, `cargo clippy`, and `cargo test` with and without `--features dhcpv4-hostname`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Archive change with `openspec archive upstream-embassy-dhcpv4-hostname`.
