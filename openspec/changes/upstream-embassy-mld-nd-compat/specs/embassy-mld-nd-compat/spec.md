# Spec Delta: MLDv1 & IPv6 Neighbor Learning Compatibility (`mld-v1-compat`, `nd-learn-from-frames`)

## Purpose

Provides `embassy-net::driver::Driver` compatibility wrappers in `rs-matter-embassy` so Matter devices remain reachable over IPv6 on MLDv1-only snooping APs and with `smoltcp`'s neighbor cache.

## ADDED Requirements

### Requirement: MLDv1 Report Conversion (`mld-v1-compat`)
When `mld-v1-compat` is enabled, `Mldv1ConvertingDriver` SHALL rewrite outgoing MLDv2 Report frames into MLDv1 Membership Reports on the TX path.

#### Scenario: Joining solicited-node multicast group on MLDv1 AP
- **WHEN** `smoltcp` transmits an MLDv2 Report for a joined IPv6 multicast address
- **THEN** `Mldv1ConvertingDriver` emits an equivalent MLDv1 Membership Report

### Requirement: Frame-Based IPv6 Neighbor Learning (`nd-learn-from-frames`)
When `nd-learn-from-frames` is enabled, `NeighborLearningDriver` SHALL inject a synthetic Neighbor Solicitation into `smoltcp`'s RX path when an incoming unicast IPv6 data frame arrives from an unseen peer.

#### Scenario: Receiving unicast IPv6 traffic from an uncached peer
- **WHEN** a unicast IPv6 frame arrives from a peer not yet in the neighbor cache
- **THEN** `NeighborLearningDriver` synthesizes a Neighbor Solicitation so `smoltcp` records the peer's link-layer address immediately
