# Proposal: Upstream Feature-Gated IPv6 MLDv1 & Neighbor-Learning Shims in rs-matter-embassy

## Why

When running `embassy-net` (`smoltcp`) over Wi-Fi on real-world home networks (reported in [sysgrok/rs-matter-embassy#37](https://github.com/sysgrok/rs-matter-embassy/issues/37)), two IPv6 compatibility issues cause Apple Home / Matter discovery and CASE/MRP session stalls:

1. **MLDv1-only AP snooping (`mld-v1-compat`)**: `smoltcp` only emits MLDv2 Membership Reports (ICMPv6 type 143). Wi-Fi access points whose multicast-to-unicast / MLD snooping bridges only inspect MLDv1 Membership Reports (ICMPv6 type 131) drop solicited-node multicast (`ff02::1:ffxx:xxxx`) and mDNS (`ff02::fb`) after group expiry.
2. **Unicast NDP resolution stalls (`nd-learn-from-frames`)**: When a Matter controller (Apple Home Hub / Home Assistant) sends a unicast IPv6 packet before `smoltcp` has its MAC in the neighbor cache, `smoltcp` queues a multicast Neighbor Solicitation before replying—which lossy Wi-Fi multicast frequently drops, causing 5–30s MRP/CASE timeouts.

## What Changes

- In `rs-matter-embassy/Cargo.toml` and `rs-matter-embassy/src/enet.rs`:
  - Add two strictly opt-in Cargo feature flags (not included in `default` features): `mld-v1-compat = []` and `nd-learn-from-frames = []`.
  - When `mld-v1-compat` is enabled, wrap the `embassy_net::driver::Driver` in `Mldv1ConvertingDriver` inside `create_enet_stack`, rewriting outgoing single-record MLDv2 reports into RFC 2710 MLDv1 Membership Reports on the TX path.
  - When `nd-learn-from-frames` is enabled, wrap the driver in `NeighborLearningDriver` inside `create_enet_stack`, injecting a synthetic Neighbor Solicitation into `smoltcp`'s RX path when an incoming unicast IPv6 frame arrives from a new peer MAC/IP pair.
  - When neither feature is enabled, `EnetDriverChain<D>` is a zero-cost transparent alias for `D`.
- Post-merge cleanup: Rebase local `rs-matter-embassy` onto `upstream/master` to drop local commit `3455ba4`.

## Capabilities

### New Capabilities
- `embassy-ipv6-compat`: Strictly opt-in `mld-v1-compat` and `nd-learn-from-frames` Cargo feature flags and `embassy-net` driver wrappers in `rs-matter-embassy`.

### Modified Capabilities
<!-- None -->

## Impact

- **Affected Repositories**:
  - `sysgrok/rs-matter-embassy` (`rs-matter-embassy/Cargo.toml`, `rs-matter-embassy/src/enet.rs`)
- **API Surface**:
  - Two opt-in Cargo features (`mld-v1-compat`, `nd-learn-from-frames`) and `EnetDriverChain<D>` type alias returned by `create_enet_stack`.
- **Breaking Changes**: None (both features are disabled by default; default `EnetDriverChain<D>` is `D`).
