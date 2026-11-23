# Design: Upstream Feature-Gated IPv6 MLDv1 & Neighbor-Learning Shims in rs-matter-embassy

## Context

`embassy-net` uses `smoltcp` for IPv6 networking. In `rs-matter-embassy/src/enet.rs`, `create_enet_stack` joins the mDNS (`ff02::fb`) and solicited-node (`ff02::1:ffxx:xxxx`) IPv6 multicast groups. However, `smoltcp` emits MLDv2 reports (ignored by MLDv1-only Wi-Fi AP snoopers) and does not populate its neighbor cache from incoming unicast L2 data frames.

## Decisions

### 1. Strictly Opt-In Cargo Feature Flags (`mld-v1-compat`, `nd-learn-from-frames`)
- **Decision**: Define `mld-v1-compat = []` and `nd-learn-from-frames = []` in `rs-matter-embassy/Cargo.toml` outside `default = ["rustcrypto", "trouble"]`. Use a conditional `pub type EnetDriverChain<D>` alias so that when both features are off, `EnetDriverChain<D>` resolves directly to `D` with zero runtime or memory overhead.
- **Rationale**: Upstream users on Ethernet or OpenThread do not pay a single byte of RAM/ROM unless they opt into one or both Wi-Fi compatibility flags.

### 2. Driver-Layer TX/RX Interception
- **Decision**: Implement `Mldv1ConvertingDriver<D>` (rewriting outgoing single-group `CHANGE_TO_EXCLUDE` / `MODE_IS_EXCLUDE` MLDv2 frames into 78-byte MLDv1 Membership Reports with recomputed IPv6 payload length and ICMPv6 checksum) and `NeighborLearningDriver<D>` (maintaining a small 8-entry LRU table of `(Ipv6Addr, [u8; 6])` learned from incoming unicast IPv6 Ethernet frames and yielding a 86-byte synthetic Neighbor Solicitation before the data frame when a new or changed mapping is observed).
- **Rationale**: Operates purely at the `embassy_net::driver::Driver` trait boundary inside `rs-matter-embassy` without requiring forks or patches to `smoltcp` or `embassy-net`.

## Migration Plan

1. Cut clean feature branch `feat/ipv6-mldv1-nd-compat` from `upstream/master` in `sysgrok/rs-matter-embassy`.
2. Port `Mldv1ConvertingDriver`, `NeighborLearningDriver`, `EnetDriverChain<D>`, and their unit tests to `src/enet.rs`, keeping `default = ["rustcrypto", "trouble"]` in `Cargo.toml`.
3. Verify host unit tests across all 4 feature combinations (`none`, `mld-v1-compat`, `nd-learn-from-frames`, `both`) and cross-compilation for `riscv32imac-unknown-none-elf`.
4. Submit PR to `sysgrok/rs-matter-embassy`.
5. Post-merge: Fetch `upstream/master` in `../rs-matter-embassy`, rebase local branch to drop commit `3455ba4`, verify zero diff for the compatibility shims against `upstream/master`, and archive change with `openspec archive upstream-embassy-ipv6-compat`.
