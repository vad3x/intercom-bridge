# Spec Delta: Feature-Gated IPv6 MLDv1 & Neighbor-Learning Shims

## Purpose

Provides opt-in, feature-gated `embassy-net` driver wrappers (`mld-v1-compat` and `nd-learn-from-frames`) in `rs-matter-embassy` to ensure reliable IPv6 multicast snooping and unicast neighbor cache learning on Wi-Fi networks without affecting default builds.

## ADDED Requirements

### Requirement: Strictly Opt-In Feature Flags
`rs-matter-embassy` SHALL provide non-default `mld-v1-compat` and `nd-learn-from-frames` Cargo features; when neither feature is enabled, `EnetDriverChain<D>` SHALL alias `D` directly with zero wrapper overhead.

#### Scenario: Default feature build
- **WHEN** `rs-matter-embassy` is compiled with default features
- **THEN** neither `mld-v1-compat` nor `nd-learn-from-frames` is active and `create_enet_stack` passes the raw driver `D` directly to `embassy_net::new`

### Requirement: MLDv2 to MLDv1 TX Conversion (`mld-v1-compat`)
When `mld-v1-compat` is enabled, `Mldv1ConvertingDriver` SHALL rewrite outgoing single-record `MODE_IS_EXCLUDE` / `CHANGE_TO_EXCLUDE` MLDv2 Membership Reports (ICMPv6 type 143) into valid RFC 2710 MLDv1 Membership Reports (ICMPv6 type 131) with updated IPv6 length and ICMPv6 checksum, leaving all other frames untouched.

#### Scenario: Outgoing MLDv2 join report
- **WHEN** `smoltcp` transmits an Ethernet frame carrying a single-group MLDv2 join report for `ff02::1:ffa6:7844`
- **THEN** `Mldv1ConvertingDriver` transmits a 78-byte MLDv1 Membership Report (type 131) for `ff02::1:ffa6:7844` with a valid ICMPv6 checksum

### Requirement: Passive Unicast Neighbor Learning (`nd-learn-from-frames`)
When `nd-learn-from-frames` is enabled, `NeighborLearningDriver` SHALL inspect incoming unicast IPv6 Ethernet frames and, upon observing a new or updated `(src_ip, src_mac)` pair, yield a synthetic Neighbor Solicitation frame to `smoltcp` before delivering the original data frame.

#### Scenario: First unicast IPv6 frame from a un-cached peer
- **WHEN** an incoming unicast IPv6 UDP/TCP frame arrives from a peer MAC/IP not yet in `NeighborLearningDriver`'s table
- **THEN** `poll_receive` first yields a valid synthetic Neighbor Solicitation with SLLA option for `(src_ip, src_mac)` and yields the original data frame on the next poll
