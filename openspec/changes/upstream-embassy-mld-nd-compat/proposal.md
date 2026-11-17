# Proposal: Upstream MLDv1 & IPv6 Neighbor Learning Compatibility Wrappers in rs-matter-embassy

## Why

On many consumer Wi-Fi APs/routers and in Apple Home Matter deployments:
1. APs that only snoop MLDv1 do not recognize `smoltcp`'s MLDv2 Reports, so the AP never learns that the Matter accessory joined its solicited-node multicast group and drops Neighbor Solicitations.
2. `smoltcp` only populates its IPv6 neighbor cache via NDP Neighbor Advertisements, causing stalls when a peer initiates IPv6 communication without completing an unsolicited/solicited NA exchange first.

Porting `Mldv1ConvertingDriver` and `NeighborLearningDriver` as feature-gated `embassy-net::driver::Driver` wrappers in `rs-matter-embassy` resolves both IPv6 reachability issues.

## What Changes

- In `rs-matter-embassy/Cargo.toml`:
  - Add features `mld-v1-compat` and `nd-learn-from-frames` (decide with upstream whether to enable in `default` or keep opt-in).
- In `rs-matter-embassy/src/enet.rs`:
  - Add `Mldv1ConvertingDriver<D>` (rewrites outgoing MLDv2 Reports into MLDv1 Membership Reports on TX).
  - Add `NeighborLearningDriver<D>` (injects a synthetic Neighbor Solicitation into `smoltcp`'s RX path when an incoming unicast IPv6 data frame arrives from an unseen peer).
  - Alias `MatterEnetDriver<D>` composing the enabled wrappers and wire it into `create_enet_stack` / `EnetNetif`.

## Capabilities

### New Capabilities
- `embassy-mld-nd-compat`: Feature-gated MLDv1 report conversion and RX-frame IPv6 neighbor learning `Driver` wrappers in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`Cargo.toml`, `src/enet.rs`)
- **API Surface**:
  - `MatterEnetDriver<D>`, `Mldv1ConvertingDriver<D>`, `NeighborLearningDriver<D>`, `solicited_node_multicast_ipv6`.
- **Breaking Changes**: None when gated or composed transparently via `MatterEnetDriver<D>`.
